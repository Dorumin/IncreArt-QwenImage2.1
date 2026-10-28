//! The user's `config.rsn` (DESIGN.md, Config file).

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::rsnfmt;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename = "Config", default)]
pub struct Config {
    /// Libraries opened before, most recent first (offered at startup).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub recent_libraries: Vec<String>,
    /// Older configs' single last library: read, folded into
    /// `recent_libraries` by [`normalize`](Self::normalize), never written.
    #[serde(skip_serializing)]
    pub library: Option<String>,
    pub resolutions: Vec<Resolution>,
    /// The command that opens a folder in the file manager (the folder is
    /// appended). Empty: try xdg-open, gio, thunar, nautilus, dolphin, nemo,
    /// pcmanfm.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub file_manager: String,
    /// Model presets for the Generate panel (recipe + model files + sampling
    /// defaults).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub recipes: Vec<RecipePreset>,
    /// Named server setups, chosen in the startup dialog.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub server_presets: Vec<ServerPreset>,
    /// The preset chosen last time (preselected in the startup dialog).
    #[serde(serialize_with = "crate::rsnfmt::bare", skip_serializing_if = "Option::is_none")]
    pub last_preset: Option<String>,
    #[serde(skip_serializing_if = "is_default")]
    pub sdserver: SdServerConfig,
    /// How the job queue shares the servers (Jobs: spread all / sweep by
    /// sweep / one at a time).
    #[serde(skip_serializing_if = "is_default")]
    pub job_order: crate::scheduler::Order,
    /// Sounds when a GPU is idle or the review queue is long.
    #[serde(skip_serializing_if = "is_default")]
    pub notifications: crate::notify::Notifications,
    /// The contact sheet's cell size in points (0: the default).
    #[serde(skip_serializing_if = "is_default")]
    pub sheet_cell: u32,
    /// The contact sheet's row headers: beside the row, or a caption above.
    #[serde(skip_serializing_if = "is_default")]
    pub sheet_rows: SheetRows,
    /// Review's Grid tile size in points (0: the default).
    #[serde(skip_serializing_if = "is_default")]
    pub review_grid_cell: u32,
    /// The Output library Grid's tile size in points (0: the default).
    #[serde(skip_serializing_if = "is_default")]
    pub output_grid_cell: u32,
    /// The Input library Grid's card width in points (0: the default).
    #[serde(skip_serializing_if = "is_default")]
    pub library_grid_cell: u32,
    /// The interface zoom in percent (0: 100%).
    #[serde(skip_serializing_if = "is_default")]
    pub ui_zoom: u32,
    /// The Tile dialog's settings, as last used.
    #[serde(skip_serializing_if = "is_default")]
    pub tile: TileSettings,
    /// The project tree doesn't list the selected project's runs.
    #[serde(skip_serializing_if = "is_default")]
    pub hide_project_runs: bool,
    /// How the project tree and grid order projects.
    #[serde(skip_serializing_if = "is_default")]
    pub project_order: TreeOrder,
    /// How the project tree orders folders (by the projects anywhere in
    /// them; empty folders last).
    #[serde(skip_serializing_if = "is_default")]
    pub folder_order: FolderOrder,
    /// Older configs' project order (A–Z): read, folded into
    /// `project_order` by [`normalize`](Self::normalize), never written.
    #[serde(skip_serializing)]
    pub projects_by_name: bool,
    /// Older configs' folder order (newest first): read, folded into
    /// `folder_order` by [`normalize`](Self::normalize), never written.
    #[serde(skip_serializing)]
    pub folders_by_newest: bool,
    /// How Review's scope tree orders folders and projects.
    #[serde(skip_serializing_if = "is_default")]
    pub review_order: ReviewOrder,
    /// Older configs' Review order (oldest to review): read, folded into
    /// `review_order` by [`normalize`](Self::normalize), never written.
    #[serde(skip_serializing)]
    pub review_oldest_first: bool,
    /// Review's scope tree also lists folders, projects, runs and sweeps
    /// with nothing to review (**hide reviewed** unticked).
    #[serde(skip_serializing_if = "is_default")]
    pub review_show_reviewed: bool,
    /// Prompt enhance: the chat servers that suggest prompt text.
    #[serde(skip_serializing_if = "is_default")]
    pub pe: crate::pe::PeConfig,
    /// How the sweep's **Choose styles** window orders the presets.
    #[serde(skip_serializing_if = "is_default")]
    pub style_sort: StyleSort,
    /// Selecting a project never narrows the project grid (**narrow to
    /// siblings** unticked).
    #[serde(skip_serializing_if = "is_default")]
    pub project_click_keeps_scope: bool,
    /// Collapsed folders of the project tree (relative to `projects/`), per
    /// library path.
    #[serde(skip_serializing_if = "is_default")]
    pub collapsed_folders: std::collections::BTreeMap<String, Vec<PathBuf>>,
}

/// A model preset: which sd-server recipe and model files to load, and the
/// Generate panel's defaults for it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename = "RecipePreset")]
pub struct RecipePreset {
    pub name: String,
    pub recipe: String,
    /// Slot → model file ID (as sd-server's `model-files` lists them).
    pub files: std::collections::BTreeMap<String, String>,
    pub steps: u32,
    pub cfg: f32,
    #[serde(default, serialize_with = "crate::rsnfmt::bare", skip_serializing_if = "Option::is_none")]
    pub flow_shift: Option<f32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom_sigmas: Vec<f32>,
}

pub fn builtin_recipes() -> Vec<RecipePreset> {
    let files = [
        ("diffusion_model", "diffusion_models/qwen_image_2.1_int8_convrot.safetensors"),
        ("llm", "text_encoders/qwen3vl_8b_int8_convrot.safetensors"),
        ("vae", "vae/qwen_image_2.1_vae_bf16.safetensors"),
    ]
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .into();
    vec![RecipePreset {
        name: "Qwen-Image 2.1".into(),
        recipe: "qwen-image-21".into(),
        files,
        steps: 20,
        cfg: 2.0,
        flow_shift: None,
        custom_sigmas: vec![],
    }]
}

/// A named set of servers (e.g. "Both GPUs", "Remote box"): sd-servers,
/// and at most one chat server for prompt enhance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename = "ServerPreset")]
pub struct ServerPreset {
    pub name: String,
    pub servers: Vec<ServerConfig>,
}

/// One server of a preset: an sd-server started by the app or a running one
/// to connect to, or the chat server prompt enhance uses.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServerConfig {
    Start {
        name: String,
        #[serde(default = "localhost")]
        host: String,
        port: u16,
        /// `--backend` device, e.g. `ROCm0`.
        device: String,
        /// Extra arguments for this server only (e.g. `--offload-to-cpu`).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        args: Vec<String>,
    },
    Connect {
        name: String,
        /// e.g. `http://10.0.0.2:1234`.
        url: String,
        /// The device it runs on, for recipes (optional).
        #[serde(default, skip_serializing_if = "String::is_empty")]
        device: String,
    },
    /// The chat server for prompt enhance (not an sd-server).
    Chat(crate::pe::PeServer),
}

impl ServerConfig {
    pub fn name(&self) -> &str {
        match self {
            ServerConfig::Start { name, .. } | ServerConfig::Connect { name, .. } => name,
            ServerConfig::Chat(s) => &s.name,
        }
    }

    pub fn device(&self) -> &str {
        match self {
            ServerConfig::Start { device, .. } | ServerConfig::Connect { device, .. } => device,
            ServerConfig::Chat(_) => "",
        }
    }

    pub fn base_url(&self) -> String {
        match self {
            ServerConfig::Start { host, port, .. } => format!("http://{host}:{port}"),
            ServerConfig::Connect { url, .. } => url.trim_end_matches('/').to_string(),
            ServerConfig::Chat(s) => s.base_url(),
        }
    }

    pub fn is_chat(&self) -> bool {
        matches!(self, ServerConfig::Chat(_))
    }
}

impl ServerPreset {
    /// Its sd-servers, in order.
    pub fn sd_servers(&self) -> Vec<ServerConfig> {
        self.servers.iter().filter(|s| !s.is_chat()).cloned().collect()
    }

    /// Its chat server.
    pub fn chat(&self) -> Option<&crate::pe::PeServer> {
        self.servers.iter().find_map(|s| match s {
            ServerConfig::Chat(c) => Some(c),
            _ => None,
        })
    }
}

impl Config {
    pub fn preset(&self, name: &str) -> Option<&ServerPreset> {
        self.server_presets.iter().find(|p| p.name == name)
    }

    /// The presets' chat servers, one per process (the first preset's name
    /// for it).
    pub fn chat_servers(&self) -> Vec<&crate::pe::PeServer> {
        let mut list: Vec<&crate::pe::PeServer> = vec![];
        for c in self.server_presets.iter().filter_map(ServerPreset::chat) {
            if !list.iter().any(|x| x.same_process(c)) {
                list.push(c);
            }
        }
        list
    }

    /// PE servers from before they were preset entries: the one used goes
    /// into every preset (its system prompts and sampling are now
    /// everyone's), each other one into a preset of its own.
    fn move_pe_servers(&mut self) -> Option<String> {
        use crate::pe::PeServer;
        let old = std::mem::take(&mut self.pe.servers);
        let chosen = std::mem::take(&mut self.pe.server);
        let used = old.iter().position(|s| s.name == chosen).unwrap_or(0);
        let first = old.get(used)?;
        if self.pe.system_prompts.is_empty() && self.pe.sampling.is_empty() {
            self.pe.system_prompts = first.system_prompts.clone();
            self.pe.sampling = first.sampling.clone();
        }
        let server = |o: &crate::pe::OldPeServer| PeServer { name: o.name.clone(), run: o.run.clone(), start: o.start };
        let had_presets = !self.server_presets.is_empty();
        for p in self.server_presets.iter_mut().filter(|p| p.chat().is_none()) {
            p.servers.push(ServerConfig::Chat(server(first)));
        }
        let mut alone = vec![];
        for (_, o) in old.iter().enumerate().filter(|(i, _)| *i != used || !had_presets) {
            let name = (1..)
                .map(|n| if n == 1 { o.name.clone() } else { format!("{} {n}", o.name) })
                .find(|n| self.preset(n).is_none())
                .unwrap();
            alone.push(name.clone());
            self.server_presets.push(ServerPreset { name, servers: vec![ServerConfig::Chat(server(o))] });
        }
        let mut said = vec![];
        if had_presets {
            said.push(format!("{} is now in every server preset", first.name));
        }
        if !alone.is_empty() {
            said.push(format!("new presets with only a chat server: {}", alone.join(", ")));
        }
        let text = format!("Prompt enhance servers moved into the server presets: {}", said.join("; "));
        Some(text)
    }
}

/// Settings shared by every sd-server.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename = "SdServer", default)]
pub struct SdServerConfig {
    pub binary: String,
    /// `--model-dir` roots, in order.
    pub model_dirs: Vec<String>,
    /// Arguments for every server.
    pub args: Vec<String>,
    /// Preview mode requested with each job: `none`, `proj`, `tae` or `vae`.
    pub preview: String,
    /// The sd.cpp commit the binary was built from, recorded in recipes.
    pub commit: String,
    /// Keep servers the app started running when it exits (the model stays
    /// loaded for the next session, which finds them on their ports).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub leave_running: bool,
}

impl Default for SdServerConfig {
    fn default() -> Self {
        SdServerConfig {
            binary: "sd-server".into(),
            model_dirs: vec![],
            args: ["--fa", "--conditioning-cache-size", "32"].map(String::from).to_vec(),
            preview: "none".into(),
            commit: String::new(),
            leave_running: false,
        }
    }
}

pub const MAX_RECENT_LIBRARIES: usize = 10;

/// How Review's Tile… lays images out (DESIGN.md, Tiling).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename = "TileSettings", default)]
pub struct TileSettings {
    /// Space between tiles and around them, in output pixels.
    pub gap: u32,
    /// RGBA; alpha 0 is transparent.
    pub background: [u8; 4],
    /// Crop to fill the cell (else fit inside it).
    pub fill: bool,
    /// Caption text height.
    pub caption_text: TextSize,
    pub caption_color: [u8; 4],
    /// The last caption format applied to every tile ("{style} · seed {seed}").
    pub caption_format: String,
    /// The title's text height (the subtitle's is half).
    pub title_text: TextSize,
    /// Output scale in percent of the images' own size.
    pub scale: u32,
}

/// A Tile text's height (DESIGN.md, Tiling).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename = "TextSize")]
pub enum TextSize {
    /// To suit the tiling: captions 5 % of the cell's height; the title
    /// 4 % of the image's width, at least 10 % of the cell's height; a text
    /// block as large as fits its cells, up to the title's size.
    Auto,
    /// Percent of the cell's height.
    Cell(f32),
    /// Output pixels.
    Px(u32),
}

impl Default for TileSettings {
    fn default() -> TileSettings {
        TileSettings {
            gap: 16,
            background: [30, 30, 30, 255],
            fill: false,
            caption_text: TextSize::Auto,
            caption_color: [235, 235, 235, 255],
            caption_format: "{style} · seed {seed}".into(),
            title_text: TextSize::Auto,
            scale: 100,
        }
    }
}

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

fn localhost() -> String {
    "127.0.0.1".into()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename = "Resolution")]
pub struct Resolution {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    pub width: u32,
    pub height: u32,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub default: bool,
}

impl Resolution {
    pub fn new(name: &str, width: u32, height: u32) -> Resolution {
        Resolution { name: name.into(), width, height, default: false }
    }

    /// `W × H` followed by the name, as the size dropdown shows it.
    pub fn label(&self) -> String {
        if self.name.is_empty() {
            format!("{} × {}", self.width, self.height)
        } else {
            format!("{} × {}  {}", self.width, self.height, self.name)
        }
    }
}

pub fn builtin_resolutions() -> Vec<Resolution> {
    let mut list = vec![
        Resolution::new("Square", 1024, 1024),
        Resolution::new("Square L", 1344, 1344),
        Resolution::new("Square XL", 2048, 2048),
        Resolution::new("Portrait 2:3", 896, 1344),
        Resolution::new("Landscape 3:2", 1344, 896),
        Resolution::new("Wide 16:9", 1664, 928),
    ];
    list[0].default = true;
    list
}

/// Nearest multiple of 32 (halves round up), at least 32.
pub fn snap32(v: u32) -> u32 {
    (v.saturating_add(16) / 32 * 32).max(32)
}

impl Config {
    /// `$XDG_CONFIG_HOME/increart/config.rsn`, else `~/.config/increart/config.rsn`.
    pub fn default_path() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(base.join("increart").join("config.rsn"))
    }

    /// Reads and [normalizes](Self::normalize) a config; a missing file is
    /// the default config. Returns the warnings to log. Old PE servers moved
    /// into the presets are saved so at once, the file as it was kept
    /// beside it (`config.rsn.before-chat-presets`).
    pub fn load(path: &Path) -> Result<(Config, Vec<String>)> {
        let mut config: Config = if path.exists() { rsnfmt::read(path)? } else { Config::default() };
        let moving = !config.pe.servers.is_empty();
        let mut warnings = config.normalize();
        if moving {
            let mut backup = path.as_os_str().to_owned();
            backup.push(".before-chat-presets");
            let saved = fs::copy(path, &backup)
                .map_err(|e| e.to_string())
                .and_then(|_| config.save(path).map_err(|e| e.to_string()));
            match saved {
                Ok(()) => warnings.push(format!("the config as it was is kept in {}", Path::new(&backup).display())),
                Err(e) => warnings.push(format!("the moved PE servers weren't saved: {e}")),
            }
        }
        Ok((config, warnings))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        rsnfmt::write(path, self)
    }

    /// Applies the load rules: old PE servers moved into the server presets,
    /// `library` folded into the recent libraries,
    /// built-in list if empty, sizes snapped to multiples of 32, exactly one
    /// default (the first marked, else the first).
    pub fn normalize(&mut self) -> Vec<String> {
        let mut warnings = Vec::new();
        warnings.extend(self.move_pe_servers());
        if let Some(l) = self.library.take().filter(|l| !l.is_empty()) {
            self.add_recent_library(l);
        }
        if std::mem::take(&mut self.projects_by_name) {
            self.project_order = TreeOrder::Name;
        }
        if std::mem::take(&mut self.folders_by_newest) {
            self.folder_order = FolderOrder(TreeOrder::Created);
        }
        if std::mem::take(&mut self.review_oldest_first) {
            self.review_order = ReviewOrder::OldestToReview;
        }
        if self.recipes.is_empty() {
            self.recipes = builtin_recipes();
        }
        if self.resolutions.is_empty() {
            self.resolutions = builtin_resolutions();
            return warnings;
        }
        for r in &mut self.resolutions {
            let (w, h) = (snap32(r.width), snap32(r.height));
            if (w, h) != (r.width, r.height) {
                warnings.push(format!(
                    "resolution {:?}: {} × {} is not a multiple of 32, using {w} × {h}",
                    r.name, r.width, r.height
                ));
                (r.width, r.height) = (w, h);
            }
        }
        let first = self.resolutions.iter().position(|r| r.default).unwrap_or(0);
        for (i, r) in self.resolutions.iter_mut().enumerate() {
            r.default = i == first;
        }
        warnings
    }

    /// Puts `root` first in the recent libraries (once), keeping
    /// [`MAX_RECENT_LIBRARIES`].
    pub fn add_recent_library(&mut self, root: String) {
        self.recent_libraries.retain(|r| *r != root);
        self.recent_libraries.insert(0, root);
        self.recent_libraries.truncate(MAX_RECENT_LIBRARIES);
    }

    pub fn default_resolution(&self) -> Option<&Resolution> {
        self.resolutions.iter().find(|r| r.default).or(self.resolutions.first())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn older_tree_orders_fold_in() {
        let mut c: Config = rsnfmt::from_str("Config { projects_by_name: true, folders_by_newest: true }").unwrap();
        c.normalize();
        assert_eq!((c.project_order, c.folder_order.0), (TreeOrder::Name, TreeOrder::Created));
        let text = rsnfmt::to_string(&c);
        assert!(!text.contains("_by_"), "{text}");
        assert_eq!(rsnfmt::from_str::<Config>(&text).unwrap(), c);
        assert_eq!(Config::default().folder_order.0, TreeOrder::Name, "folders A–Z by default");
        assert_eq!(Config::default().review_order, ReviewOrder::Name, "Review A–Z by default");
        let mut c: Config = rsnfmt::from_str("Config { review_oldest_first: true }").unwrap();
        c.normalize();
        assert_eq!(c.review_order, ReviewOrder::OldestToReview);
        assert!(!rsnfmt::to_string(&c).contains("oldest"));
    }

    #[test]
    fn old_pe_servers_move_into_the_presets() {
        let text = r#"Config {
            server_presets: [
                ServerPreset { name: "Both", servers: [Start { name: "gpu0", port: 1234, device: "ROCm0" }] },
                ServerPreset { name: "CPU", servers: [] },
            ],
            pe: Pe {
                server: "B",
                servers: [
                    PeServer { name: "A", run: Connect { url: "http://a:1" } },
                    PeServer {
                        name: "B",
                        run: Connect { url: "http://b:1" },
                        start: AtLaunch,
                        system_prompts: { "rewrite": "mine" },
                    },
                ],
            },
        }"#;
        let mut c: Config = rsnfmt::from_str(text).unwrap();
        let w = c.normalize();
        assert!(w[0].contains("B is now in every server preset") && w[0].contains("only a chat server: A"), "{w:?}");
        let names: Vec<_> =
            c.server_presets.iter().map(|p| (p.name.as_str(), p.chat().map(|s| s.name.as_str()))).collect();
        assert_eq!(names, [("Both", Some("B")), ("CPU", Some("B")), ("A", Some("A"))]);
        assert_eq!(c.server_presets[0].sd_servers().len(), 1);
        assert_eq!(c.server_presets[0].chat().unwrap().start, crate::pe::PeStart::AtLaunch);
        assert_eq!(c.pe.system_prompt(crate::pe::REWRITE), "mine", "the used one's prompts are everyone's");
        assert_eq!(c.chat_servers().len(), 2, "one per process");
        // written the new way, and read back the same
        let out = rsnfmt::to_string(&c);
        assert!(out.contains("Chat(") && !out.contains("servers: []") && !out.contains("server:"), "{out}");
        assert_eq!(rsnfmt::from_str::<Config>(&out).unwrap(), c);
        assert!(c.clone().normalize().is_empty(), "moved once");
    }

    #[test]
    fn loading_saves_the_moved_pe_servers_with_a_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.rsn");
        let old = r#"Config { pe: Pe { servers: [PeServer { name: "A", run: Connect { url: "http://a:1" } }] } }"#;
        fs::write(&path, old).unwrap();
        let (c, w) = Config::load(&path).unwrap();
        assert!(w.iter().any(|w| w.contains("before-chat-presets")), "{w:?}");
        assert_eq!(fs::read_to_string(dir.path().join("config.rsn.before-chat-presets")).unwrap(), old);
        assert_eq!(Config::load(&path).unwrap(), (c, vec![]));
    }

    #[test]
    fn snapping() {
        assert_eq!(snap32(1024), 1024);
        assert_eq!(snap32(1000), 992);
        assert_eq!(snap32(1008), 1024);
        assert_eq!(snap32(0), 32);
    }

    #[test]
    fn normalize_rules() {
        let mut c: Config = rsnfmt::from_str(
            r#"Config { resolutions: [
                Resolution { width: 1000, height: 1024 },
                Resolution { name: "b", width: 512, height: 512, default: true },
                Resolution { name: "c", width: 640, height: 640, default: true },
            ] }"#,
        )
        .unwrap();
        let w = c.normalize();
        assert_eq!(w.len(), 1);
        assert_eq!(c.resolutions[0].width, 992);
        assert_eq!(c.resolutions.iter().filter(|r| r.default).count(), 1);
        assert_eq!(c.default_resolution().unwrap().name, "b");

        let mut none: Config = rsnfmt::from_str(
            "Config { resolutions: [Resolution { width: 64, height: 64 }, Resolution { width: 96, height: 96 }] }",
        )
        .unwrap();
        none.normalize();
        assert!(none.resolutions[0].default);

        let mut empty: Config = rsnfmt::from_str("Config {}").unwrap();
        empty.normalize();
        assert_eq!(empty.resolutions, builtin_resolutions());
    }

    #[test]
    fn tile_text_sizes() {
        // the old pixel sizes are dropped for auto; the rest kept
        let c: Config =
            rsnfmt::from_str("Config { tile: TileSettings { gap: 8, caption_size: 32, title_size: 64 } }").unwrap();
        assert_eq!((c.tile.gap, c.tile.caption_text, c.tile.title_text), (8, TextSize::Auto, TextSize::Auto));
        let c = Config {
            tile: TileSettings {
                caption_text: TextSize::Cell(4.5),
                title_text: TextSize::Px(90),
                ..Default::default()
            },
            ..Default::default()
        };
        let back: Config = rsnfmt::from_str(&rsnfmt::to_string(&c)).unwrap();
        assert_eq!(back.tile, c.tile);
    }

    #[test]
    fn recent_libraries() {
        let mut c: Config = rsnfmt::from_str(r#"Config { recent_libraries: ["/b", "/a"], library: "/a" }"#).unwrap();
        c.normalize();
        assert_eq!(c.recent_libraries, ["/a", "/b"], "the old last library goes first, once");
        assert!(!rsnfmt::to_string(&c).contains("library:"), "the old field is not written back");
        for i in 0..MAX_RECENT_LIBRARIES + 2 {
            c.add_recent_library(format!("/{i}"));
        }
        assert_eq!(c.recent_libraries.len(), MAX_RECENT_LIBRARIES);
        assert_eq!(c.recent_libraries[0], format!("/{}", MAX_RECENT_LIBRARIES + 1));
    }

    #[test]
    fn server_presets() {
        let c: Config = rsnfmt::from_str(
            r#"Config {
                server_presets: [
                    ServerPreset { name: "Both GPUs", servers: [
                        Start { name: "gpu0", port: 1234, device: "ROCm0" },
                        Start { name: "gpu1", port: 1235, device: "ROCm1", args: ["--offload-to-cpu"] },
                    ] },
                    ServerPreset { name: "Remote", servers: [Connect { name: "box", url: "http://10.0.0.2:1234/" }] },
                ],
                last_preset: "Remote",
                sdserver: SdServer { binary: "/opt/sd-server", model_dirs: ["/models"] },
            }"#,
        )
        .unwrap();
        let both = c.preset("Both GPUs").unwrap();
        assert_eq!(both.servers[0].base_url(), "http://127.0.0.1:1234");
        assert_eq!(both.servers[1].device(), "ROCm1");
        assert_eq!(c.preset("Remote").unwrap().servers[0].base_url(), "http://10.0.0.2:1234");
        assert_eq!(c.sdserver.args, ["--fa", "--conditioning-cache-size", "32"], "defaults for missing fields");
        let again: Config = rsnfmt::from_str(&rsnfmt::to_string(&c)).unwrap();
        assert_eq!(again, c);
    }

    #[test]
    fn roundtrip_and_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("increart/config.rsn");
        let (c, w) = Config::load(&path).unwrap();
        assert!(w.is_empty());
        assert_eq!(c.resolutions, builtin_resolutions());
        c.save(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("Config {\n    resolutions: [\n        Resolution {"), "{text}");
        assert_eq!(Config::load(&path).unwrap().0, c);
    }
}

/// The contact sheet's row headers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SheetRows {
    /// The style, its counts and buttons beside the row.
    #[default]
    Full,
    /// Full while the rows fit one per line; compact once they fold into
    /// bands side by side.
    Auto,
    /// The style as a caption above the row; its buttons in its menu.
    Compact,
}

impl SheetRows {
    pub fn name(self) -> &'static str {
        match self {
            SheetRows::Auto => "Auto",
            SheetRows::Full => "Full",
            SheetRows::Compact => "Compact",
        }
    }

    /// Full → Compact → Auto → Full.
    pub fn next(self) -> SheetRows {
        match self {
            SheetRows::Full => SheetRows::Compact,
            SheetRows::Compact => SheetRows::Auto,
            SheetRows::Auto => SheetRows::Full,
        }
    }
}

/// How the project tree orders projects or folders (a folder by the projects
/// anywhere in it).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TreeOrder {
    /// Newest created first.
    #[default]
    Created,
    /// Last run (seeds queued) first; never run: by when created.
    LastRun,
    Name,
}

impl TreeOrder {
    pub const EVERY: [TreeOrder; 3] = [TreeOrder::Created, TreeOrder::LastRun, TreeOrder::Name];

    pub fn name(self) -> &'static str {
        match self {
            TreeOrder::Created => "Created",
            TreeOrder::LastRun => "Last run",
            TreeOrder::Name => "A–Z",
        }
    }
}

/// How Review's scope tree orders folders and projects: as the project tree
/// can (a folder by the projects anywhere in it), or oldest with images to
/// review first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReviewOrder {
    Created,
    LastRun,
    #[default]
    Name,
    /// The oldest projects with images to review first (a folder by the
    /// oldest such project in it), then those with none.
    OldestToReview,
}

impl ReviewOrder {
    pub const EVERY: [ReviewOrder; 4] =
        [ReviewOrder::Created, ReviewOrder::LastRun, ReviewOrder::Name, ReviewOrder::OldestToReview];

    pub fn name(self) -> &'static str {
        match self.tree() {
            Some(o) => o.name(),
            None => "Oldest to review",
        }
    }

    /// The project tree's order this is, if any.
    pub fn tree(self) -> Option<TreeOrder> {
        match self {
            ReviewOrder::Created => Some(TreeOrder::Created),
            ReviewOrder::LastRun => Some(TreeOrder::LastRun),
            ReviewOrder::Name => Some(TreeOrder::Name),
            ReviewOrder::OldestToReview => None,
        }
    }
}

/// The folders' [`TreeOrder`], A–Z by default.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FolderOrder(pub TreeOrder);

impl Default for FolderOrder {
    fn default() -> Self {
        FolderOrder(TreeOrder::Name)
    }
}

/// How the sweep's **Choose styles** window orders the presets.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StyleSort {
    /// As the presets list them (by task and group).
    #[default]
    Listed,
    Name,
    /// Most Good images first.
    MostGood,
    /// Last used first.
    Recent,
    /// Never used first, then least recently used.
    Untried,
}

impl StyleSort {
    pub const EVERY: [StyleSort; 5] =
        [StyleSort::Listed, StyleSort::Name, StyleSort::MostGood, StyleSort::Recent, StyleSort::Untried];

    pub fn name(self) -> &'static str {
        match self {
            StyleSort::Listed => "As listed",
            StyleSort::Name => "A–Z",
            StyleSort::MostGood => "Most Good",
            StyleSort::Recent => "Recently used",
            StyleSort::Untried => "Never used first",
        }
    }
}
