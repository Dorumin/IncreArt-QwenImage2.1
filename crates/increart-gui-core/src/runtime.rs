//! Carries out [`Effect`]s off the GUI thread and reports back as [`Msg`]s:
//! library operations in order on one worker thread, servers opened in
//! parallel, engine events forwarded on [`Runtime::poll`].

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use increart_core::config::{RecipePreset, SdServerConfig, ServerConfig};
use increart_core::engine::{Endpoint, Engine, EngineOptions};
use increart_core::launcher::{self, ServerProcess};
use increart_core::model::{IdeaSource, Mode, ModelFile, ModelSelection, Sample};
use increart_core::request::GenOptions;
use increart_core::{Id, IdeaLoc, ImageRef, Library, NewIdea, NewRun, Tree, Verdict, modelhash};

use crate::state::{EntryKind, FvEntry, GenerateForm, PickKind};
use crate::update::{DraftSave, Effect, LibOp, Msg, OpOk, TileTarget};

/// How long a started sd-server may take to answer.
const START_WAIT: Duration = Duration::from_secs(180);

#[derive(Default)]
struct Shared {
    library: Option<Arc<Mutex<Library>>>,
    servers: Vec<Option<ServerProcess>>,
    engine: Option<Engine>,
    /// The opening attempt `servers` belongs to.
    attempt: u32,
    /// Set to stop the current opening attempt (and servers it started).
    cancel: Arc<std::sync::atomic::AtomicBool>,
    /// Servers still being opened.
    opening: Arc<std::sync::atomic::AtomicUsize>,
    /// The open library's watch for outside changes.
    watch: Option<crate::watch::LibraryWatch>,
}

type Wake = Arc<dyn Fn() + Send + Sync>;

pub struct Runtime {
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    work: Sender<Effect>,
    shared: Arc<Mutex<Shared>>,
    wake: Wake,
}

/// `$XDG_STATE_HOME/increart/logs`, else `~/.local/state/increart/logs`.
pub fn log_dir() -> PathBuf {
    let base = std::env::var_os("XDG_STATE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("increart").join("logs")
}

impl Runtime {
    /// `wake` is called whenever a message is ready (e.g. request a repaint).
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Runtime {
        let (tx, rx) = mpsc::channel();
        let (work, jobs) = mpsc::channel::<Effect>();
        let shared = Arc::new(Mutex::new(Shared::default()));
        let wake: Wake = Arc::new(wake);
        let (wtx, wshared, wwake) = (tx.clone(), shared.clone(), wake.clone());
        std::thread::Builder::new()
            .name("library".into())
            .spawn(move || {
                for fx in jobs {
                    // Opening goes through this queue so it can't overtake a
                    // StopServers sent just before it.
                    if let Effect::OpenServers { servers, shared, attempt } = fx {
                        open_servers(&wtx, &wwake, &wshared, servers, shared, attempt);
                        continue;
                    }
                    let msgs = work_on(&wshared, &wwake, fx);
                    for m in msgs {
                        let _ = wtx.send(m);
                    }
                    wwake();
                }
            })
            .expect("spawn library worker");
        Runtime { tx, rx, work, shared, wake }
    }

    pub fn run(&self, fx: Effect) {
        match fx {
            Effect::PlaySound { sound, volume, player, test } => {
                let (tx, wake) = (self.tx.clone(), self.wake.clone());
                std::thread::spawn(move || {
                    let _ = tx.send(Msg::SoundPlayed(increart_core::notify::play(&sound, volume, &player), test));
                    wake();
                });
            }
            Effect::ListDevices(binary) => {
                let (tx, wake) = (self.tx.clone(), self.wake.clone());
                std::thread::spawn(move || {
                    let r = launcher::list_devices(&binary).map_err(|e| format!("{binary}: {e}"));
                    let _ = tx.send(Msg::Devices(r));
                    wake();
                });
            }

            Effect::FolderPreviews { root, dir, names } => {
                let (tx, wake) = (self.tx.clone(), self.wake.clone());
                std::thread::spawn(move || {
                    let found = names
                        .into_iter()
                        .map(|n| (n.clone(), previews(&root.join(&dir).join(n), store_of(&dir))))
                        .collect();
                    let _ = tx.send(Msg::FvPreviews(dir, found));
                    wake();
                });
            }
            Effect::CheckUsage => {
                let Some(lib) = library(&self.shared) else { return };
                let (tx, wake, shared) = (self.tx.clone(), self.wake.clone(), self.shared.clone());
                std::thread::Builder::new()
                    .name("usage check".into())
                    .spawn(move || check_usage(&lib, &shared, &tx, &wake))
                    .expect("spawn usage check");
            }
            Effect::WatchLibrary(root) => {
                let w = crate::watch::start(root, self.tx.clone(), self.wake.clone());
                self.shared.lock().unwrap().watch = Some(w);
            }
            Effect::ComposeTiles { root, layout, settings, to } => {
                let (tx, wake) = (self.tx.clone(), self.wake.clone());
                std::thread::spawn(move || {
                    let r = crate::tile::compose(&root, &layout, &settings).and_then(|img| {
                        if let TileTarget::File(path) = &to {
                            img.save(path).map_err(|e| format!("{}: {e}", path.display()))?;
                        }
                        Ok(crate::tile::TileImage(Arc::new(img)))
                    });
                    let _ = tx.send(Msg::TilesDone(to, r));
                    wake();
                });
            }
            Effect::Pick { kind, start } => {
                let (tx, wake) = (self.tx.clone(), self.wake.clone());
                std::thread::spawn(move || {
                    let _ = tx.send(Msg::Picked(kind, pick(kind, start)));
                    wake();
                });
            }
            other => {
                let _ = self.work.send(other);
            }
        }
    }

    /// Messages ready since the last call, including engine events.
    pub fn poll(&self) -> Vec<Msg> {
        let mut out: Vec<Msg> = self.rx.try_iter().collect();
        if let Some(engine) = &self.shared.lock().unwrap().engine {
            out.extend(engine.events().try_iter().map(Msg::Engine));
        }
        out
    }

    /// Carries out a draft save on the calling thread (quitting: the worker
    /// thread may not get to it).
    pub fn save_now(&self, fx: Effect) {
        if matches!(fx, Effect::SaveDraft(_)) {
            for m in work_on(&self.shared, &self.wake, fx) {
                let _ = self.tx.send(m);
            }
        }
    }

    /// Waits up to `timeout` for at least one message (tests).
    pub fn wait(&self, timeout: Duration) -> Vec<Msg> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let m = self.poll();
            if !m.is_empty() || std::time::Instant::now() >= deadline {
                return m;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Stops the engine and the servers the app started, or with
    /// `leave_running` lets started servers outlive the app.
    pub fn shutdown(&self, leave_running: bool) {
        // Stop servers still starting (they would outlive the app), and give
        // their threads a moment to do it.
        let opening = self.shared.lock().map(|s| {
            s.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            s.opening.clone()
        });
        if let Ok(opening) = opening {
            let deadline = std::time::Instant::now() + Duration::from_millis(1500);
            while opening.load(std::sync::atomic::Ordering::Relaxed) > 0 && std::time::Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        if let Ok(mut s) = self.shared.lock() {
            if let Some(engine) = s.engine.take() {
                engine.shutdown();
            }
            if leave_running {
                s.servers.iter_mut().flatten().for_each(ServerProcess::detach);
            }
            s.servers.clear();
        }
    }
}

impl Drop for Runtime {
    /// Stops the engine and the servers the app started, before the process
    /// exits (the worker thread may never get to it). The app calls
    /// [`Runtime::shutdown`] first when started servers should stay up.
    fn drop(&mut self) {
        self.shutdown(false);
    }
}

const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff"];

/// The system file picker: the desktop portal when reachable, else zenity
/// (rfd's fallback). Empty when cancelled.
fn pick(kind: PickKind, start: Option<PathBuf>) -> Vec<PathBuf> {
    let mut d = rfd::FileDialog::new();
    if let Some(s) = start.filter(|s| s.is_dir()) {
        d = d.set_directory(s);
    }
    match kind {
        PickKind::LibraryFolder => d
            .set_title("Library folder (existing, or an empty one for a new library)")
            .pick_folder()
            .into_iter()
            .collect(),
        PickKind::ModelDir => d.set_title("Model folder (sd-server --model-dir)").pick_folder().into_iter().collect(),
        PickKind::SdServerBinary => d.set_title("sd-server binary").pick_file().into_iter().collect(),
        PickKind::IdeaImages => {
            d.set_title("Images for the new idea").add_filter("Images", IMAGE_EXTS).pick_files().unwrap_or_default()
        }
        PickKind::ExportDir => d.set_title("Export to").pick_folder().into_iter().collect(),
        PickKind::TileFile => d
            .set_title("Save the tiled image")
            .set_file_name("tiles.png")
            .add_filter("PNG", &["png"])
            .save_file()
            .into_iter()
            .collect(),
        PickKind::GenerateRefs => {
            d.set_title("Reference images").add_filter("Images", IMAGE_EXTS).pick_files().unwrap_or_default()
        }
        PickKind::ImportFiles => {
            let mut exts = IMAGE_EXTS.to_vec();
            exts.push("txt");
            d.set_title("Import as new ideas").add_filter("Images and text", &exts).pick_files().unwrap_or_default()
        }
    }
}

/// Opens the servers of one attempt in parallel; reports each, then settles.
fn open_servers(
    tx: &Sender<Msg>,
    wake: &Wake,
    shared: &Arc<Mutex<Shared>>,
    servers: Vec<ServerConfig>,
    cfg: SdServerConfig,
    attempt: u32,
) {
    let (cancel, opening) = {
        let mut s = shared.lock().unwrap();
        s.servers = (0..servers.len()).map(|_| None).collect();
        s.attempt = attempt;
        s.cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        s.opening.fetch_add(servers.len(), std::sync::atomic::Ordering::Relaxed);
        (s.cancel.clone(), s.opening.clone())
    };
    let (tx, wake, shared) = (tx.clone(), wake.clone(), shared.clone());
    std::thread::spawn(move || {
        let handles: Vec<_> = servers
            .into_iter()
            .enumerate()
            .map(|(i, server)| {
                let (tx, wake, shared, cfg) = (tx.clone(), wake.clone(), shared.clone(), cfg.clone());
                let (cancel, opening) = (cancel.clone(), opening.clone());
                std::thread::spawn(move || {
                    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
                    if matches!(server, ServerConfig::Start { .. }) {
                        watch_start_log(&server, attempt, i, &tx, &wake, &done);
                    }
                    let opened = launcher::open_cancellable(&server, &cfg, &log_dir(), START_WAIT, &cancel);
                    done.store(true, std::sync::atomic::Ordering::Relaxed);
                    opening.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                    let msg = match opened {
                        Ok(p) => {
                            let mut s = shared.lock().unwrap();
                            // Kept only for the current attempt; otherwise
                            // dropped, which stops a server this app started.
                            if s.attempt == attempt
                                && let Some(slot) = s.servers.get_mut(i)
                            {
                                *slot = Some(p);
                            }
                            Msg::ServerOpened(attempt, i, Ok(()))
                        }
                        Err(e) => Msg::ServerOpened(attempt, i, Err(e.to_string())),
                    };
                    let _ = tx.send(msg);
                    wake();
                })
            })
            .collect();
        for h in handles {
            let _ = h.join();
        }
        let _ = tx.send(Msg::ServersSettled(attempt));
        wake();
    });
}

/// While a server starts: what its log says, twice a second (loading
/// tensors 294/396 …), as [`Msg::ServerLog`].
fn watch_start_log(
    server: &ServerConfig,
    attempt: u32,
    i: usize,
    tx: &Sender<Msg>,
    wake: &Wake,
    done: &Arc<std::sync::atomic::AtomicBool>,
) {
    let log = launcher::log_path(&log_dir(), server);
    let start = std::fs::metadata(&log).map_or(0, |m| m.len());
    let (tx, wake, done) = (tx.clone(), wake.clone(), done.clone());
    std::thread::spawn(move || {
        let mut last = None;
        while !done.load(std::sync::atomic::Ordering::Relaxed) {
            let line = launcher::latest_line(&log, start);
            if line.is_some() && line != last {
                let _ = tx.send(Msg::ServerLog(attempt, i, line.clone().unwrap_or_default()));
                wake();
                last = line;
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    });
}

/// Copies store images to `dest`, named by image ID or by run / revision /
/// seed, with or without IncreArt's and sd-server's metadata chunks.
fn export(lib: &Library, images: &[ImageRef], dest: &Path, keep_metadata: bool, by_id: bool) -> Result<usize, String> {
    std::fs::create_dir_all(dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    let ix = lib.scan().map_err(err)?;
    let mut n = 0;
    for img in images {
        let bytes = std::fs::read(lib.image_path(img)).map_err(|e| e.to_string())?;
        let bytes = if keep_metadata {
            bytes
        } else {
            let keep = |k: &[u8; 4]| !matches!(k, b"tEXt" | b"iTXt" | b"zTXt" | b"eXIf");
            let chunks = increart_core::png::read_chunks(&bytes)?;
            increart_core::png::write_chunks(&chunks.into_iter().filter(|c| keep(&c.kind)).collect::<Vec<_>>())
        };
        let link = ix.projects.iter().flat_map(|p| &p.outputs).find(|o| &o.image == img);
        let base = match (by_id, link) {
            (false, Some(o)) => o.name.file_name(),
            _ => format!("{}.png", img.id),
        };
        let mut target = dest.join(&base);
        let mut k = 2;
        while target.exists() {
            target = dest.join(format!("{} {k}.png", base.trim_end_matches(".png")));
            k += 1;
        }
        std::fs::write(&target, bytes).map_err(|e| format!("{}: {e}", target.display()))?;
        n += 1;
    }
    Ok(n)
}

/// Opens a folder (a file: its folder) in the file manager: `command` if
/// set, else the first of xdg-open, gio, thunar, nautilus, dolphin, nemo,
/// pcmanfm that works.
fn show_in_file_manager(path: &Path, command: &str) -> Result<(), String> {
    use std::process::{Command, Stdio};
    let dir = if path.is_dir() { path.to_path_buf() } else { path.parent().unwrap_or(path).to_path_buf() };
    let spawn = |argv: &[&str], wait: bool| -> bool {
        let Some((prog, args)) = argv.split_first() else { return false };
        let mut c = Command::new(prog);
        c.args(args).arg(&dir).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        if wait { c.status().is_ok_and(|s| s.success()) } else { c.spawn().is_ok() }
    };
    if !command.trim().is_empty() {
        let argv: Vec<&str> = command.split_whitespace().collect();
        return if spawn(&argv, false) { Ok(()) } else { Err(format!("couldn't run `{command}`")) };
    }
    // Openers exit at once (their status says whether a handler was found);
    // file managers keep running.
    let openers: &[&[&str]] = &[&["xdg-open"], &["gio", "open"]];
    let managers: &[&[&str]] = &[&["thunar"], &["nautilus"], &["dolphin"], &["nemo"], &["pcmanfm"]];
    if openers.iter().any(|a| spawn(a, true)) || managers.iter().any(|a| spawn(a, false)) {
        return Ok(());
    }
    Err("No file manager found: install one (e.g. thunar) or set `file_manager` in config.rsn".into())
}

/// A folder of the library for the folder view: folders, projects, runs,
/// image links (with the image they point at) and metadata files.
fn list_dir(root: &Path, rel: &Path) -> Vec<FvEntry> {
    use increart_core::link;
    let dir = root.join(rel);
    let Ok(rd) = std::fs::read_dir(&dir) else { return vec![] };
    // Inside a project?
    let in_project = rel.ancestors().any(|a| root.join(a).join("project.rsn").is_file());
    // ideas link into the input store, everything else into the output store
    let store = store_of(rel);
    let mut out = vec![];
    for e in rd.flatten() {
        let path = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let meta = std::fs::metadata(&path).ok();
        let modified = meta.as_ref().and_then(|m| m.modified().ok()).map(chrono::DateTime::<chrono::Utc>::from);
        let (kind, size, info) = if path.is_dir() {
            let n = std::fs::read_dir(&path).map(|r| r.count() as u64).unwrap_or(0);
            if path.join("project.rsn").is_file() {
                (EntryKind::Project, n, "project".to_string())
            } else if let Ok(id) = name.parse::<increart_core::Id>()
                && path.join("idea.rsn").is_file()
            {
                let title = increart_core::rsnfmt::read::<increart_core::model::Idea>(&path.join("idea.rsn"))
                    .map(|i| i.title)
                    .unwrap_or_default();
                (EntryKind::Idea(id), n, if title.is_empty() { "idea".into() } else { title })
            } else if in_project && rel.file_name().is_some_and(|f| f == "runs") {
                let label = increart_core::rsnfmt::read::<increart_core::model::Run>(&path.join("run.rsn"))
                    .map(|r| format!("run {}", r.label))
                    .unwrap_or_else(|_| "run".into());
                (EntryKind::Run, n, label)
            } else if name == "runs" && dir.join("project.rsn").is_file() {
                (EntryKind::Folder, n, "runs".to_string())
            } else if in_project {
                (EntryKind::Folder, n, "review folder".to_string())
            } else {
                (EntryKind::Folder, n, "folder".to_string())
            }
        } else if name.ends_with(".rsn") || name.ends_with(".jsonl") {
            (EntryKind::Meta, meta.map_or(0, |m| m.len()), "metadata".to_string())
        } else if name.ends_with(".png") {
            match link::read_ref(&path, store) {
                Ok(Some(r)) => {
                    let what = rel.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
                    (EntryKind::Image(r), meta.map_or(0, |m| m.len()), what)
                }
                _ => (EntryKind::File, meta.map_or(0, |m| m.len()), "file".to_string()),
            }
        } else {
            (EntryKind::File, meta.map_or(0, |m| m.len()), "file".to_string())
        };
        out.push(FvEntry { name, kind, size, modified, info });
    }
    out
}

/// Up to three images for a folder's icon: a project's cover first, then the
/// newest image links found breadth-first, looking at no more than a few
/// hundred entries; `bad/` only when nothing else is there. A run's images
/// are its outputs in the project's review folders; `runs/` shows the whole
/// project's.
fn previews(dir: &Path, store: increart_core::Store) -> Vec<ImageRef> {
    let found = previews_from(dir, store, true);
    if found.is_empty() { previews_from(dir, store, false) } else { found }
}

fn previews_from(dir: &Path, store: increart_core::Store, skip_bad: bool) -> Vec<ImageRef> {
    let is_project = |d: &Path| d.join("project.rsn").is_file();
    let parent = dir.parent().unwrap_or(dir);
    if parent.file_name().is_some_and(|f| f == "runs")
        && let Some(project) = parent.parent().filter(|p| is_project(p))
    {
        let prefix = format!("{}_r", dir.file_name().unwrap_or_default().to_string_lossy());
        return walk_images(project, vec![], store, skip_bad, |name| name.starts_with(&prefix));
    }
    let top = if dir.file_name().is_some_and(|f| f == "runs") && is_project(parent) { parent } else { dir };
    let cover = increart_core::rsnfmt::read::<increart_core::model::Project>(&top.join("project.rsn"))
        .ok()
        .and_then(|p| p.cover)
        .into_iter()
        .collect();
    walk_images(top, cover, store, skip_bad, |_| true)
}

/// The store a folder's image links point into (`rel` from the library
/// root): ideas' into the input store, the rest into the output store.
fn store_of(rel: &Path) -> increart_core::Store {
    if rel.starts_with("input") { increart_core::Store::Input } else { increart_core::Store::Output }
}

fn walk_images(
    dir: &Path,
    mut out: Vec<ImageRef>,
    store: increart_core::Store,
    skip_bad: bool,
    keep: impl Fn(&str) -> bool,
) -> Vec<ImageRef> {
    use increart_core::link;
    const WANT: usize = 3;
    const BUDGET: usize = 400;
    let mut queue = std::collections::VecDeque::from([dir.to_path_buf()]);
    let mut seen = 0;
    while let Some(d) = queue.pop_front() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        let mut names: Vec<_> = rd.flatten().map(|e| e.path()).collect();
        // ids start with the date, so newest first
        names.sort_by(|a, b| b.cmp(a));
        for p in names {
            seen += 1;
            if out.len() >= WANT || seen > BUDGET {
                return out;
            }
            let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if name.starts_with('.') || (skip_bad && name == "bad") {
                continue;
            }
            if p.is_dir() {
                queue.push_back(p);
            } else if name.ends_with(".png")
                && keep(&name)
                && let Ok(Some(r)) = link::read_ref(&p, store)
                && !out.contains(&r)
            {
                out.push(r);
            }
        }
    }
    out
}

fn err(e: impl ToString) -> String {
    e.to_string()
}

fn work_on(shared: &Arc<Mutex<Shared>>, wake: &Wake, fx: Effect) -> Vec<Msg> {
    match fx {
        Effect::SaveConfig(config, path) => match config.save(&path) {
            Ok(()) => vec![],
            Err(e) => vec![Msg::OpDone(Err(format!("saving settings: {e}")), None)],
        },
        Effect::OpenLibrary { path, create } => {
            let opened = if create { Library::create(&path, None) } else { Library::open(&path) };
            vec![Msg::LibraryOpened(opened.map_err(err).and_then(|lib| {
                let ix = lib.scan().map_err(err)?;
                let root = lib.root().to_path_buf();
                shared.lock().unwrap().library = Some(Arc::new(Mutex::new(lib)));
                Ok((root, ix))
            }))]
        }
        Effect::SaveDraft(d) => {
            let Some(lib) = library(shared) else { return vec![] };
            let l = lib.lock().unwrap();
            let r = match d {
                DraftSave::Generate(project, draft) => l.save_generate_draft(&project, &draft),
                DraftSave::Idea(draft) => l.save_idea_draft(&draft),
                DraftSave::ClearIdea => l.clear_idea_draft(),
            };
            vec![Msg::DraftSaved(r.map_err(err))]
        }
        Effect::LoadHistory(key) => {
            let Some(lib) = library(shared) else { return vec![] };
            let path = lib.lock().unwrap().history_path(&key);
            let entries = path.map(|p| increart_core::history::read(&p)).unwrap_or_default();
            vec![Msg::HistoryLoaded(key, entries)]
        }
        Effect::RecordHistory { key, mut entry, replace_last } => {
            let Some(lib) = library(shared) else { return vec![] };
            let path = lib.lock().unwrap().history_path(&key);
            entry.time = increart_core::id::now();
            match path.and_then(|p| increart_core::history::record(&p, &entry, replace_last)) {
                Ok(()) => vec![],
                Err(e) => vec![Msg::Problem(format!("prompt history: {e}"))],
            }
        }
        Effect::LoadRecipe(id) => {
            let Some(lib) = library(shared) else { return vec![] };
            let r = lib.lock().unwrap().recipe(&id).ok().flatten();
            vec![Msg::RecipeLoaded(id, r)]
        }
        Effect::LoadInputMeta(ids) => {
            let Some(lib) = library(shared) else { return vec![] };
            let l = lib.lock().unwrap();
            let list = ids
                .into_iter()
                .map(|id| {
                    let source = l.source_info(&id).ok().flatten();
                    let blake3 = l.blake3_of(&ImageRef::input(id.clone())).ok();
                    (id, crate::state::InputMeta { source, blake3 })
                })
                .collect();
            vec![Msg::InputMetaLoaded(list)]
        }
        Effect::LoadImports => {
            let Some(lib) = library(shared) else { return vec![] };
            let l = lib.lock().unwrap();
            let mut ids = l.store_ids(increart_core::Store::Input).unwrap_or_default();
            ids.reverse();
            let list =
                ids.into_iter().take(200).filter_map(|id| l.source_info(&id).ok().flatten().map(|s| (id, s))).collect();
            vec![Msg::Imports(list)]
        }
        Effect::Export { images, dest, keep_metadata, by_id } => {
            let Some(lib) = library(shared) else { return vec![] };
            let l = lib.lock().unwrap();
            vec![Msg::Exported(export(&l, &images, &dest, keep_metadata, by_id))]
        }
        Effect::ShowInFileManager { path, command } => match show_in_file_manager(&path, &command) {
            Ok(()) => vec![],
            Err(e) => vec![Msg::Problem(e)],
        },
        Effect::ListDir(rel) => {
            let Some(lib) = library(shared) else { return vec![] };
            let root = lib.lock().unwrap().root().to_path_buf();
            vec![Msg::FvListed(rel.clone(), list_dir(&root, &rel))]
        }
        Effect::Rescan => match library(shared) {
            Some(lib) => lib.lock().unwrap().scan().map(Msg::Scanned).into_iter().collect(),
            None => vec![],
        },
        Effect::StopEngine => {
            let engine = shared.lock().unwrap().engine.take();
            if let Some(engine) = engine {
                engine.shutdown();
            }
            vec![]
        }
        Effect::SavePresets(presets, path) => match presets.save_dir(&path) {
            Ok(()) => vec![],
            Err(e) => vec![Msg::OpDone(Err(format!("saving prompt presets: {e}")), None)],
        },
        Effect::StopServers => {
            let mut s = shared.lock().unwrap();
            s.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            if let Some(engine) = s.engine.take() {
                engine.shutdown();
            }
            // dropping a process the app started stops it (kill, then wait)
            let names = s.servers.iter().flatten().filter(|p| p.launched()).map(|p| p.name.clone()).collect();
            s.servers.clear();
            vec![Msg::ServersStopped(names)]
        }
        Effect::SetJobOrder(order) => {
            if let Some(e) = &shared.lock().unwrap().engine {
                e.set_order(order);
            }
            vec![]
        }
        Effect::StartEngine { shared: cfg, order } => {
            let Some(lib) = library(shared) else { return vec![] };
            // Runs with seeds left, found before taking any other lock (the
            // library lock is held once, never nested).
            let pending: Vec<_> = {
                let l = lib.lock().unwrap();
                match l.scan() {
                    Ok(ix) => ix
                        .projects
                        .iter()
                        .flat_map(|p| &p.runs)
                        .filter(|r| l.pending_seeds(&r.id).is_ok_and(|s| !s.is_empty()))
                        .map(|r| r.id.clone())
                        .collect(),
                    Err(_) => vec![],
                }
            };
            let endpoints: Vec<Endpoint> = shared
                .lock()
                .unwrap()
                .servers
                .iter()
                .flatten()
                .map(|p| Endpoint { name: p.name.clone(), device: p.device.clone(), client: p.client.clone() })
                .collect();
            let opts = EngineOptions {
                generation: GenOptions { preview: cfg.preview.clone(), ..Default::default() },
                sdcpp: cfg.commit.clone(),
                order,
                ..Default::default()
            };
            let w = wake.clone();
            let engine = Engine::start(lib.clone(), endpoints, opts, Some(Box::new(move || w())));
            for r in &pending {
                engine.queue_run(r);
            }
            shared.lock().unwrap().engine = Some(engine);
            vec![Msg::RunsQueued(pending)]
        }
        Effect::QueueRun(id) => {
            if let Some(e) = &shared.lock().unwrap().engine {
                e.queue_run(&id);
            }
            vec![]
        }
        Effect::CancelRun(id, how) => {
            if let Some(e) = &shared.lock().unwrap().engine {
                e.cancel_run(&id, how);
            }
            vec![]
        }
        Effect::Lib(op) => {
            let Some(lib) = library(shared) else { return vec![Msg::OpDone(Err("no library open".into()), None)] };
            let queue_run = matches!(&op, LibOp::CreateRun { queue: true, .. });
            let op_queue = match &op {
                LibOp::CreateSweep { queue, .. } => Some(*queue),
                _ => None,
            };
            let result = {
                let mut l = lib.lock().unwrap();
                lib_op(&mut l, op)
            };
            let queue_run = queue_run || matches!(&op_queue, Some(true));
            if let Some(e) = &shared.lock().unwrap().engine
                && queue_run
            {
                match &result {
                    Ok(OpOk::Run(id, _)) => e.queue_run(id),
                    // all at once: the scheduler's round robin goes breadth-first
                    Ok(OpOk::Sweep(ids, ..)) => ids.iter().for_each(|id| e.queue_run(id)),
                    _ => {}
                }
            }
            let ix = lib.lock().unwrap().scan().ok();
            vec![Msg::OpDone(result, ix)]
        }
        Effect::ListDevices(_)
        | Effect::PlaySound { .. }
        | Effect::OpenServers { .. }
        | Effect::Pick { .. }
        | Effect::FolderPreviews { .. }
        | Effect::ComposeTiles { .. }
        | Effect::WatchLibrary(_)
        | Effect::CheckUsage => {
            unreachable!("handled on their own threads")
        }
    }
}

/// The usage check after opening, one image at a time so library
/// operations go on meanwhile; stops if another library is opened.
fn check_usage(lib: &Arc<Mutex<Library>>, shared: &Arc<Mutex<Shared>>, tx: &Sender<Msg>, wake: &Wake) {
    let check = lib.lock().unwrap().usage_check();
    let Ok(check) = check else { return };
    let total = check.images.len();
    let mut sent = std::time::Instant::now();
    for (i, r) in check.images.iter().enumerate() {
        if !library(shared).is_some_and(|l| Arc::ptr_eq(&l, lib)) {
            let _ = tx.send(Msg::UsageCheck(total, total));
            wake();
            return;
        }
        lib.lock().unwrap().check_usage(&check, r);
        if sent.elapsed() > std::time::Duration::from_millis(100) {
            sent = std::time::Instant::now();
            let _ = tx.send(Msg::UsageCheck(i + 1, total));
            wake();
        }
    }
    let _ = tx.send(Msg::UsageCheck(total, total));
    wake();
}

fn library(shared: &Arc<Mutex<Shared>>) -> Option<Arc<Mutex<Library>>> {
    shared.lock().unwrap().library.clone()
}

/// The verdict a link has now, from its folder.
fn verdict_of(link: &Path) -> Option<Verdict> {
    match link.parent()?.file_name()?.to_str()? {
        "good" => Some(Verdict::Good),
        "neutral" => Some(Verdict::Neutral),
        "bad" => Some(Verdict::Bad),
        _ => None,
    }
}

pub fn lib_op(lib: &mut Library, op: LibOp) -> Result<OpOk, String> {
    match op {
        LibOp::NewIdea(n) => lib.new_idea(n).map(OpOk::Idea).map_err(err),
        LibOp::NewIdeaPromote(n, folder, name) => {
            let id = lib.new_idea(n).map_err(err)?;
            promote(lib, &[id], &folder, &name)
        }
        LibOp::ImportFiles(paths) => {
            for p in paths {
                let is_text = p.extension().is_some_and(|e| e.eq_ignore_ascii_case("txt"));
                let mut idea = NewIdea {
                    source: IdeaSource::DroppedFile { original_path: p.to_string_lossy().into() },
                    ..NewIdea::text("", "")
                };
                if is_text {
                    idea.text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?.trim().into();
                } else {
                    idea.images.push(lib.import_file(&p).map_err(err)?.image);
                }
                lib.new_idea(idea).map_err(err)?;
            }
            Ok(OpOk::Done)
        }
        LibOp::ImportImages(paths) => {
            let ids = paths.iter().map(|p| lib.import_file(p).map(|i| i.image)).collect::<Result<Vec<_>, _>>();
            ids.map(OpOk::Imported).map_err(err)
        }
        LibOp::ImportRefs(paths) => {
            let ids = paths.iter().map(|p| lib.import_file(p).map(|i| i.image)).collect::<Result<Vec<_>, _>>();
            ids.map(OpOk::ImportedRefs).map_err(err)
        }
        LibOp::ReviewIdea(id, v) => lib.review_idea(&id, v).map(|_| OpOk::Done).map_err(err),
        LibOp::MoveIdea(id, loc) => lib.move_idea(&id, &loc).map(|_| OpOk::Done).map_err(err),
        LibOp::SetStar(id, on) => lib.update_idea(&id, |i| i.starred = on).map(|_| OpOk::Done).map_err(err),
        LibOp::DeleteDiscarded(id) => lib.delete_discarded_idea(&id).map(|_| OpOk::Done).map_err(err),
        LibOp::Promote(ids, folder, name) => promote(lib, &ids, &folder, &name),
        LibOp::CreateFolder(tree, parent) => {
            lib.create_folder(tree, &parent, None).map(|p| OpOk::Created(tree, p)).map_err(err)
        }
        LibOp::CreateProject(parent, name) => {
            lib.create_project(&parent, Some(&name), &[]).map(|(_, p)| OpOk::Created(Tree::Projects, p)).map_err(err)
        }
        LibOp::Rename(tree, path, name) => lib.rename_node(tree, &path, &name).map(|_| OpOk::Done).map_err(err),
        LibOp::MoveNode(tree, path, parent) => lib.move_node(tree, &path, &parent).map(|_| OpOk::Done).map_err(err),
        LibOp::DeleteNode(tree, path) => lib.delete_node(tree, &path).map(|_| OpOk::Done).map_err(err),
        LibOp::CreateSweep { project, runs, model_dir, queued, .. } => {
            let name = runs.first().map(|(_, t, _)| t.name.clone()).unwrap_or_default();
            let mut ids = vec![];
            for (form, tag, preset) in runs {
                let mut run = new_run(lib, &form, &preset, model_dir.as_deref());
                run.sweep = Some(tag);
                ids.push(lib.create_run(&project, run).map_err(err)?);
            }
            Ok(OpOk::Sweep(ids, name, queued))
        }
        LibOp::CreateRun { project, form, preset, model_dir, queued, .. } => {
            let run = new_run(lib, &form, &preset, model_dir.as_deref());
            lib.create_run(&project, run).map(|id| OpOk::Run(id, queued)).map_err(err)
        }
        LibOp::PrepareProject { parent, name, copy_of, form } => {
            let path = match copy_of {
                // the copy, renamed as asked (a taken name gets a number)
                Some(src) => {
                    let copy = lib.copy_node(Tree::Projects, &src, &parent).map_err(err)?;
                    if copy.file_name().is_some_and(|n| n.to_string_lossy() != name.as_str()) {
                        let free = lib.free_name(Tree::Projects, &parent, &name).map_err(err)?;
                        lib.rename_node(Tree::Projects, &copy, &free).map_err(err)?
                    } else {
                        copy
                    }
                }
                None => {
                    let base = increart_core::library::validate_name(&name)
                        .map(|_| name.clone())
                        .unwrap_or_else(|_| "new project".into());
                    let free = lib.free_name(Tree::Projects, &parent, &base).map_err(err)?;
                    lib.create_project(&parent, Some(&free), &[]).map_err(err)?.1
                }
            };
            Ok(OpOk::ProjectReady { path, form: Box::new(form) })
        }
        LibOp::CopyProject(path, name) => lib.duplicate_project(&path, &name).map(OpOk::Duplicated).map_err(err),
        LibOp::IdeaFromTiles { image, sources } => {
            let mut png = std::io::Cursor::new(vec![]);
            image.0.write_to(&mut png, image::ImageFormat::Png).map_err(|e| e.to_string())?;
            let title = format!("Tiled {} images", sources.len());
            lib.idea_from_tiles(png.get_ref(), sources, &title, IdeaLoc::top_library()).map(OpOk::Tiled).map_err(err)
        }
        LibOp::IdeaFromOutput { image, with_prompt, loc } => {
            lib.idea_from_output(&image, with_prompt, loc).map(OpOk::Idea).map_err(err)
        }
        LibOp::AddSeeds(run, n) => lib.add_seeds(&run, n, false).map(|_| OpOk::SeedsAdded(run, n)).map_err(err),
        LibOp::SaveIdea { id, title, text, tags, images } => lib
            .update_idea(&id, |i| {
                (i.title, i.text, i.tags, i.images) = (title, text, tags, images);
            })
            .map(|_| OpOk::Done)
            .map_err(err),
        LibOp::ImportInboxDrop => lib.import_inbox_drop().map(|_| OpOk::Done).map_err(err),
        LibOp::AddToCollection(folder, images) => {
            for img in images.iter().filter(|i| i.store == increart_core::Store::Output) {
                lib.add_to_collection(&folder, &img.id).map_err(err)?;
            }
            Ok(OpOk::Done)
        }
        LibOp::NewCollection(parent, images) => {
            let path = lib.create_folder(Tree::Collections, &parent, None).map_err(err)?;
            for img in images.iter().filter(|i| i.store == increart_core::Store::Output) {
                lib.add_to_collection(&path, &img.id).map_err(err)?;
            }
            Ok(OpOk::Created(Tree::Collections, path))
        }
        LibOp::CopyNode(tree, path, parent) => lib.copy_node(tree, &path, &parent).map(|_| OpOk::Done).map_err(err),
        LibOp::SetCover(project, img) => lib.set_cover(&project, img).map(|_| OpOk::Done).map_err(err),
        LibOp::SetTitle(project, image, title) => {
            lib.set_title(&project, &image, &title).map(|_| OpOk::Done).map_err(err)
        }
        LibOp::DeleteOutputs(links) => {
            for l in links {
                lib.delete_output_link(&l).map_err(err)?;
            }
            Ok(OpOk::Done)
        }
        LibOp::ReviewOutput { link, verdict, undo } => {
            let previous = verdict_of(&link);
            let new = lib.review_output(&link, verdict).map_err(err)?;
            Ok(OpOk::Reviewed { link: new, previous, undo })
        }
    }
}

/// Keeps the ideas (inbox ones move to the top of the input library) and
/// makes one project from them, named after the first idea's title.
fn promote(lib: &mut Library, ids: &[Id], folder: &Path, name: &str) -> Result<OpOk, String> {
    // Create in folder: a new folder is made first
    if !lib.root().join("projects").join(folder).is_dir()
        && let (Some(parent), Some(last)) = (folder.parent(), folder.file_name())
    {
        lib.create_folder(Tree::Projects, parent, Some(&last.to_string_lossy())).map_err(err)?;
    }
    let mut ideas = vec![];
    for id in ids {
        if let (IdeaLoc::Unreviewed | IdeaLoc::Discarded, _) = lib.find_idea(id).map_err(err)? {
            lib.move_idea(id, &IdeaLoc::top_library()).map_err(err)?;
        }
        ideas.push(lib.load_idea(id).map_err(err)?);
    }
    let (project, _) = lib.create_project(folder, Some(name), ids).map_err(err)?;
    let prompt = ideas.iter().map(|i| i.text.trim()).find(|t| !t.is_empty()).unwrap_or("").to_string();
    let refs = ideas.iter().flat_map(|i| i.images.iter().cloned().map(ImageRef::input)).collect();
    Ok(OpOk::Promoted { project, prompt, refs, idea: ids.first().cloned() })
}

/// A run from the Generate form. Model files are hashed (cached in the
/// library's `cache/`) when they can be found under the first model dir.
fn new_run(lib: &Library, f: &GenerateForm, preset: &RecipePreset, model_dir: Option<&str>) -> NewRun {
    let cache = lib.root().join("cache/model-hashes.rsn");
    let files = preset
        .files
        .iter()
        .map(|(slot, id)| {
            let sha256 = match model_dir {
                Some(dir) if !id.starts_with('@') => {
                    modelhash::sha256_cached(&cache, &Path::new(dir).join(id)).unwrap_or_default()
                }
                _ => String::new(),
            };
            (slot.clone(), ModelFile { id: id.clone(), sha256 })
        })
        .collect();
    NewRun {
        sweep: None,
        label: f.label.clone(),
        prompt: f.prompt.clone(),
        negative_prompt: f.negative.clone(),
        // the prompt as typed (the old Transparent wrapper broke edits)
        alpha: false,
        width: f.width,
        height: f.height,
        sample: Sample {
            method: "euler".into(),
            scheduler: String::new(),
            steps: f.steps,
            cfg: f.cfg,
            flow_shift: f.flow_shift,
            custom_sigmas: preset.custom_sigmas.clone(),
        },
        mode: match &f.init() {
            Some(init) => Mode::Edit { init: Some(init.clone()), strength: f.strength },
            None => Mode::Generate,
        },
        refs: f.refs(),
        model: ModelSelection { recipe: preset.recipe.clone(), files },
        source_idea: f.source_idea.clone(),
        first_seed: f.first_seed,
        seeds: f.seeds,
    }
}
