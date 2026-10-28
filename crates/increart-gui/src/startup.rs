//! Start-up: open or create a library, then choose servers (DESIGN.md,
//! Startup: servers).

use eframe::egui::{self, Color32, RichText};
use increart_core::config::ServerConfig;
use increart_gui_core::{
    AppState, Editing, LibraryDialog, LibraryWizard, Msg, PickKind, ServerDialog, ServerProgress, Without,
};

use crate::widgets::{ACCENT, ACCENT_MUTED, BAD, GOOD, NEUTRAL, ic, ph};

/// A start-up screen: a framed box in the middle of the window. Its height
/// is only known after drawing it, so it's kept per `key` and the frame is
/// redone when it changes.
fn centered(ui: &mut egui::Ui, key: &str, width: f32, add: impl FnOnce(&mut egui::Ui)) {
    let id = egui::Id::unique(("startup_height", key));
    let last: Option<f32> = ui.ctx().data(|d| d.get_temp(id));
    ui.vertical_centered(|ui| {
        ui.add_space(((ui.available_height() - last.unwrap_or(0.0)) / 2.0).max(16.0));
        let h = egui::Frame::window(ui.style())
            .inner_margin(20.0)
            .show(ui, |ui| {
                ui.set_width(width);
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), add);
            })
            .response
            .rect
            .height();
        if last.is_none_or(|l| (l - h).abs() > 0.5) {
            ui.ctx().data_mut(|d| d.insert_temp(id, h));
            ui.ctx().request_discard("centring a start-up screen");
        }
    });
}

/// `back`: switching from this open library (offers a way back to it).
pub fn library(
    ui: &mut egui::Ui,
    d: &LibraryDialog,
    recent: &[String],
    back: Option<&std::path::Path>,
    out: &mut Vec<Msg>,
) {
    centered(ui, "library", 560.0, |ui| {
        ui.heading(ic(ph::BOOKS, if back.is_some() { "Switch library" } else { "Open a library" }));
        if let Some(b) = back {
            ui.horizontal(|ui| {
                let name = b.file_name().map_or(b.display().to_string(), |n| n.to_string_lossy().into());
                if ui
                    .add_enabled(!d.busy, egui::Button::new(ic(ph::ARROW_LEFT, &format!("Back to {name}   Esc"))))
                    .clicked()
                    || (d.wizard.is_none() && !d.busy && ui.input(|i| i.key_pressed(egui::Key::Escape)))
                {
                    out.push(Msg::CancelSwitch);
                }
                ui.weak("The servers stay connected.");
            });
        }
        ui.add_space(6.0);
        if recent.is_empty() {
            ui.label("No libraries yet. A library is the folder that holds your ideas, projects and images.");
        } else {
            ui.label(RichText::new("Recent").strong());
            ui.add_enabled_ui(!d.busy, |ui| {
                for (i, path) in recent.iter().enumerate() {
                    let (open, forget) = egui::Sides::new().show(
                        ui,
                        |ui| {
                            // the list is most recent first: the last used one tagged, the tag in a muted accent
                            let label = RichText::new(ic(ph::BOOKS, path));
                            let open = ui.selectable_label(false, label).on_hover_text("Open").clicked();
                            if back.is_some_and(|b| b == std::path::Path::new(path)) {
                                ui.label(RichText::new("open now").color(ACCENT_MUTED));
                            } else if i == 0 {
                                ui.label(RichText::new("last used").color(ACCENT_MUTED));
                            }
                            open
                        },
                        |ui| {
                            let open_now = back.is_some_and(|b| b == std::path::Path::new(path));
                            !open_now
                                && ui
                                    .small_button(ph::X)
                                    .on_hover_text("Remove from the list (the folder stays)")
                                    .clicked()
                        },
                    );
                    if open {
                        out.push(Msg::OpenRecentLibrary(i));
                    }
                    if forget {
                        out.push(Msg::ForgetLibrary(i));
                    }
                }
            });
            ui.separator();
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_enabled_ui(!d.busy, |ui| {
                // the last used library, named, as the main button (and Enter)
                // (switching: the last used other than the open one)
                let main = recent.iter().position(|p| back.is_none_or(|b| b != std::path::Path::new(p)));
                if let Some((i, path)) = main.map(|i| (i, &recent[i])) {
                    let name =
                        std::path::Path::new(path).file_name().map_or(path.clone(), |n| n.to_string_lossy().into());
                    let open = RichText::new(ic(ph::BOOKS, &format!("Open {name}   ⏎"))).strong().color(ACCENT);
                    let enter = d.wizard.is_none()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter))
                        && !ui.ctx().egui_wants_keyboard_input();
                    if ui.button(open).on_hover_text(path).clicked() || enter {
                        out.push(Msg::OpenRecentLibrary(i));
                    }
                }
                let create = RichText::new(ic(ph::FOLDER_PLUS, "Create a new library…"));
                let create = if recent.is_empty() { create.strong() } else { create };
                if ui.button(create).clicked() {
                    let first = suggested_libraries().into_iter().next().map(|(_, p)| p).unwrap_or_default();
                    out.push(Msg::LibraryWizard(Some((LibraryWizard::Create, first))));
                }
                if ui.button(ic(ph::FOLDER_OPEN, "Open an existing library…")).clicked() {
                    let first = found_libraries(recent).into_iter().next().map(|(_, p)| p).unwrap_or_default();
                    out.push(Msg::LibraryWizard(Some((LibraryWizard::Open, first))));
                }
            });
        });
        let loading = egui::Id::unique("library_loading_since");
        if !d.busy {
            ui.ctx().data_mut(|m| m.remove::<f64>(loading));
        } else if d.wizard.is_none() {
            ui.add_space(8.0);
            // reading every project, run and idea: under a second, longer on a cold disk
            ui.add(
                egui::ProgressBar::new(loading_fraction(ui, loading)).text("Opening the library…").desired_width(360.0),
            );
            ui.ctx().request_repaint();
        }
        if let Some(e) = &d.error
            && d.wizard.is_none()
        {
            ui.add_space(8.0);
            ui.colored_label(BAD, e);
        }
    });
    if let Some(w) = d.wizard {
        library_wizard(ui.ctx(), d, w, recent, out);
    }
}

/// Where a new library could go: `~/IncreArt`, and `IncreArt` in the
/// current folder when that is somewhere else.
pub fn suggested_libraries() -> Vec<(&'static str, String)> {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let cwd = std::env::current_dir().ok();
    let mut v = vec![];
    if let Some(h) = &home {
        v.push(("In your home folder", h.join("IncreArt").to_string_lossy().into_owned()));
    }
    if let Some(c) = cwd.filter(|c| Some(c) != home.as_ref()) {
        v.push(("In the current folder", c.join("IncreArt").to_string_lossy().into_owned()));
    }
    v
}

/// Libraries in the suggested places that aren't in the recent list.
pub fn found_libraries(recent: &[String]) -> Vec<(&'static str, String)> {
    suggested_libraries()
        .into_iter()
        .filter(|(_, p)| folder_state(p) == FolderState::Library && !recent.contains(p))
        .map(|(_, p)| ("Found", p))
        .collect()
}

#[derive(PartialEq)]
pub enum FolderState {
    Missing,
    Empty,
    Library,
    /// Has files, but no `library.rsn`.
    Other,
}

pub fn folder_state(path: &str) -> FolderState {
    let path = std::path::Path::new(path.trim());
    if path.join("library.rsn").exists() {
        return FolderState::Library;
    }
    match std::fs::read_dir(path).map(|mut r| r.next().is_none()) {
        Err(_) => FolderState::Missing,
        Ok(true) => FolderState::Empty,
        Ok(false) => FolderState::Other,
    }
}

/// The library wizards: pick a folder, then create a library there or open
/// the one there. Each offers the other when the folder calls for it.
fn library_wizard(ctx: &egui::Context, d: &LibraryDialog, w: LibraryWizard, recent: &[String], out: &mut Vec<Msg>) {
    let create = w == LibraryWizard::Create;
    let choices = if create { suggested_libraries() } else { found_libraries(recent) };
    let state = folder_state(&d.path);
    let path = d.path.trim().to_string();
    let m = egui::Modal::new(egui::Id::unique("library_wizard")).show(ctx, |ui| {
        ui.set_width(520.0);
        if create {
            ui.heading(ic(ph::FOLDER_PLUS, "Create a new library"));
            ui.label("Where should it live? The folder is created if it doesn't exist.");
        } else {
            ui.heading(ic(ph::FOLDER_OPEN, "Open an existing library"));
            ui.label("Pick the library's folder: the one with library.rsn in it.");
        }
        ui.add_space(8.0);
        ui.add_enabled_ui(!d.busy, |ui| {
            let mut other = true;
            for (label, p) in &choices {
                let on = path == *p;
                other &= !on;
                if ui.radio(on, format!("{label}: {p}")).clicked() {
                    out.push(Msg::SetLibraryPath(p.clone()));
                }
            }
            ui.horizontal(|ui| {
                if !choices.is_empty() && ui.radio(other, "Somewhere else:").clicked() && !other {
                    out.push(Msg::Pick(PickKind::LibraryFolder));
                }
                // empty while a listed folder is picked; typing here picks this option
                let shown = if other { d.path.clone() } else { String::new() };
                let mut p = shown.clone();
                let r = ui.add(
                    egui::TextEdit::singleline(&mut p).desired_width(ui.available_width() - 100.0).hint_text("/path"),
                );
                if choices.is_empty() {
                    crate::widgets::focus_until_done(&r);
                }
                if p != shown {
                    out.push(Msg::SetLibraryPath(p));
                }
                if ui.button(ic(ph::FOLDER_OPEN, "Browse…")).clicked() {
                    out.push(Msg::Pick(PickKind::LibraryFolder));
                }
            });
        });
        ui.add_space(6.0);
        // what's there, and the other wizard when it fits better
        let switch = |ui: &mut egui::Ui, out: &mut Vec<Msg>, text: &str, to: LibraryWizard| {
            if ui.add_enabled(!d.busy, egui::Button::new(text).small()).clicked() {
                out.push(Msg::LibraryWizard(Some((to, path.clone()))));
            }
        };
        if !path.is_empty() {
            ui.horizontal(|ui| match (create, &state) {
                (true, FolderState::Library) => {
                    ui.colored_label(BAD, "There is a library here already.");
                    switch(ui, out, "Open it instead", LibraryWizard::Open);
                }
                (true, FolderState::Other) => {
                    ui.colored_label(BAD, "This folder isn't empty: choose another one.");
                }
                (true, _) => {}
                (false, FolderState::Library) => {
                    ui.colored_label(GOOD, ic(ph::CHECK_CIRCLE, "An IncreArt library"));
                }
                (false, FolderState::Other) => {
                    ui.colored_label(BAD, "Not an IncreArt library (no library.rsn).");
                }
                (false, FolderState::Missing | FolderState::Empty) => {
                    let what = if state == FolderState::Missing { "No such folder." } else { "This folder is empty." };
                    ui.colored_label(BAD, what);
                    switch(ui, out, "Create a library here instead", LibraryWizard::Create);
                }
            });
        }
        if let Some(e) = &d.error {
            ui.colored_label(BAD, e);
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let (text, msg, ok) = if create {
                let ok = matches!(state, FolderState::Missing | FolderState::Empty);
                (ic(ph::FOLDER_PLUS, "Create library"), Msg::CreateLibrary, ok)
            } else {
                (ic(ph::FOLDER_OPEN, "Open library"), Msg::OpenLibrary, state == FolderState::Library)
            };
            let ok = ok && !d.busy && !path.is_empty();
            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui.add_enabled(ok, egui::Button::new(RichText::new(text).strong())).clicked() || (ok && enter) {
                out.push(msg);
            }
            if ui.add_enabled(!d.busy, egui::Button::new("Cancel")).clicked() {
                out.push(Msg::LibraryWizard(None));
            }
            if d.busy {
                ui.spinner();
            }
        });
    });
    if m.should_close() && !d.busy {
        out.push(Msg::LibraryWizard(None));
    }
}

/// "Start ROCm0 on :1234" / "Connect to http://…" / "Chat: Qwen3-VL".
pub fn describe(s: &ServerConfig) -> String {
    match s {
        ServerConfig::Start { device, port, .. } => format!("Start {device} on :{port}"),
        ServerConfig::Connect { url, .. } => format!("Connect to {url}"),
        ServerConfig::Chat(c) => format!("Chat: {}", c.name),
    }
}

pub fn servers(ui: &mut egui::Ui, st: &AppState, d: &ServerDialog, out: &mut Vec<Msg>) {
    centered(ui, "servers", 560.0, |ui| {
        ui.heading(ic(ph::HARD_DRIVES, "Servers"));
        ui.label(
            "The servers for this session: sd-servers to generate with, and a chat server for prompt enhance. \
             Connected servers keep running when IncreArt exits.",
        );
        if let Some(n) = &d.notice {
            ui.add_space(4.0);
            ui.colored_label(NEUTRAL, ic(ph::WARNING, n));
        }
        ui.add_space(10.0);
        // Locked (and greyed) while opening; the progress below stays readable.
        let locked = ui.add_enabled_ui(!d.opening, |ui| presets(ui, st, d, out));
        if d.opening {
            // Disabled widgets only fade a little: dim the whole area too.
            let r = locked.response.rect.expand(6.0);
            ui.painter().rect_filled(r, 6.0, egui::Color32::from_black_alpha(120));
            ui.painter().text(
                r.center(),
                egui::Align2::CENTER_CENTER,
                ic(ph::LOCK_SIMPLE, "locked while the servers open"),
                egui::FontId::proportional(16.0),
                egui::Color32::from_gray(200),
            );
        }
        if let Some(problem) = settings_problem(st)
            && !d.opening
        {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.colored_label(NEUTRAL, ic(ph::WARNING, &problem));
                if ui.small_button("Fix…").clicked() {
                    out.push(Msg::ServerSettings(true));
                }
            });
        }
        if !d.progress.is_empty() {
            ui.separator();
            progress(ui, st, d);
        }
        ui.separator();
        if d.wizard {
            server_wizard(ui.ctx(), st, d, out);
        }
        if let Some(editing) = d.editing {
            editor_window(ui.ctx(), st, d, editing, out);
        }
        if d.settings {
            settings_window(ui.ctx(), st, d, out);
        }
        if let Some(w) = &d.without {
            without_window(ui.ctx(), st, w, out);
        }
        let modal = d.editing.is_some()
            || d.wizard
            || d.settings
            || d.without.is_some()
            || st.confirm.is_some()
            || st.pe.editor.is_some();
        // ways out on the left, the settings on the right
        let settings = egui::Sides::new().show(
            ui,
            |ui| bottom_left(ui, st, d, modal, out),
            |ui| {
                !d.opening
                    && ui
                        .button(ic(ph::GEAR, "sd-server settings…"))
                        .on_hover_text("Binary, models folder, devices: shared by every preset")
                        .clicked()
            },
        );
        if settings.1 {
            out.push(Msg::ServerSettings(true));
        }
    });
}

/// Open the last used preset, continue, close, or (while opening) continue
/// in background / cancel.
fn bottom_left(ui: &mut egui::Ui, st: &AppState, d: &ServerDialog, modal: bool, out: &mut Vec<Msg>) {
    if d.opening {
        if ui
            .button(ic(ph::ARROW_RIGHT, "Continue in background"))
            .on_hover_text("use IncreArt meanwhile; the servers join when they are up")
            .clicked()
        {
            out.push(Msg::ContinueInBackground);
        }
        if ui.button(ic(ph::X, "Cancel")).on_hover_text("stop opening (and the servers already started)").clicked() {
            out.push(Msg::CancelOpening);
        }
        return;
    }
    // the last used preset, named, as the main button (and Enter); not
    // from the main window while servers run: that is what Close keeps
    let last = st
        .config
        .last_preset
        .as_ref()
        .and_then(|n| st.config.server_presets.iter().position(|p| &p.name == n).map(|i| (i, n)));
    if let Some((i, name)) = last
        && !(d.from_main && !st.servers.is_empty())
    {
        let open = RichText::new(ic(ph::PLAY, &format!("Open {name}   ⏎"))).strong().color(ACCENT);
        let enter = !modal && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !ui.ctx().egui_wants_keyboard_input();
        if ui.button(open).on_hover_text("The preset used last time").clicked() || enter {
            out.push(Msg::OpenPreset(i));
        }
    }
    let up = d.progress.iter().filter(|p| **p == ServerProgress::Up).count();
    let failed = d.progress.iter().any(|p| matches!(p, ServerProgress::Failed(_)));
    if failed && up > 0 && ui.button(format!("Continue with {up} server(s)")).clicked() {
        out.push(Msg::ContinueWithServers);
    }
    // from the main window: back, keeping the servers in use (also Esc)
    if d.from_main {
        let keep = if st.servers.is_empty() { "Close" } else { "Close, keep the servers in use" };
        let esc = !modal && ui.input(|i| i.key_pressed(egui::Key::Escape));
        if ui.button(RichText::new(ic(ph::ARROW_LEFT, keep)).strong()).clicked() || esc {
            out.push(Msg::CloseServerDialog);
        }
    }
    let without = if d.from_main && !st.servers.is_empty() {
        "Stop the servers, continue without"
    } else {
        "Continue without servers"
    };
    // from the main window with no servers, Close does the same
    if !(d.from_main && st.servers.is_empty()) && ui.button(without).clicked() {
        out.push(Msg::AskContinueWithoutServers);
    }
}

/// What is wrong with the sd-server settings, when a preset starts servers:
/// a missing binary or models folder.
fn settings_problem(st: &AppState) -> Option<String> {
    let starts =
        st.config.server_presets.iter().flat_map(|p| &p.servers).any(|s| matches!(s, ServerConfig::Start { .. }));
    if !starts {
        return None;
    }
    st.sdserver_problem()
}

/// Per server: "starting — 12 s", up, or the error.
fn progress(ui: &mut egui::Ui, st: &AppState, d: &ServerDialog) {
    let secs = (st.now - d.started).max(0.0);
    if !st.stopping.is_empty() {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.weak(format!("stopping the servers in use first ({})", st.stopping.join(", ")));
        });
    }
    for (i, (s, p)) in d.draft.sd_servers().iter().zip(&d.progress).enumerate() {
        ui.horizontal(|ui| {
            ui.label(RichText::new(s.name()).strong());
            ui.weak(describe(s));
            match p {
                ServerProgress::Opening => {
                    ui.spinner();
                    let what = if matches!(s, ServerConfig::Start { .. }) { "starting" } else { "connecting" };
                    ui.label(format!("{what} — {secs:.0} s"));
                    if let Some(line) = st.start_log.get(i).filter(|l| !l.is_empty()) {
                        ui.add(egui::Label::new(RichText::new(line).weak().monospace()).truncate())
                            .on_hover_text(format!("from the server's log\n{line}"));
                    }
                }
                ServerProgress::Up => {
                    ui.colored_label(GOOD, ic(ph::CHECK_CIRCLE, "up"));
                }
                ServerProgress::Failed(e) => {
                    ui.colored_label(BAD, ic(ph::WARNING, e));
                }
            }
        });
    }
    // the chat server: on its own, never holding the sd-servers up
    if let Some(c) = d.draft.chat() {
        ui.horizontal(|ui| {
            ui.label(RichText::new(&c.name).strong());
            ui.weak("chat server");
            let (dot, state, color) = crate::app::pe_state(ui, st);
            let state = state.split_once(": ").map_or(state.as_str(), |(_, s)| s);
            if st.pe.status == increart_gui_core::PeStatus::Starting {
                ui.spinner();
            }
            ui.colored_label(color, format!("{dot} {state}"));
        });
    }
}

/// The presets: a click opens one, ✎ edits it, the bin deletes it (asks).
fn presets(ui: &mut egui::Ui, st: &AppState, d: &ServerDialog, out: &mut Vec<Msg>) {
    ui.label(RichText::new("Presets").strong());
    if st.config.server_presets.is_empty() {
        ui.label("No presets yet.");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.button(RichText::new(ic(ph::MAGIC_WAND, "Set up servers…")).strong()).clicked() {
                out.push(Msg::ServerWizard(true));
            }
        });
        ui.weak("Or make one by hand with New preset.");
        ui.add_space(6.0);
    }
    let last = st.config.last_preset.as_ref().and_then(|n| st.config.server_presets.iter().position(|p| &p.name == n));
    for (i, p) in st.config.server_presets.iter().enumerate() {
        let summary = p.servers.iter().map(describe).collect::<Vec<_>>().join(" · ");
        let text = format!("{}\n{}", p.name, summary);
        // the preset used last time tagged, the tag in a muted accent (as in the library list)
        let text = RichText::new(text);
        let editing = d.editing == Some(Editing::Preset(i));
        let (open, (edit, delete)) = egui::Sides::new().show(
            ui,
            |ui| {
                let open = ui.selectable_label(editing, text).on_hover_text("Open").clicked();
                if last == Some(i) {
                    ui.label(RichText::new("last used").color(ACCENT_MUTED));
                }
                open
            },
            |ui| {
                // right to left: ✎ pinned to the right edge, the bin next to it
                let edit = ui.small_button(ph::PENCIL_SIMPLE).on_hover_text("Edit").clicked();
                let delete = ui.small_button(ph::TRASH).on_hover_text("Delete preset…").clicked();
                (edit, delete)
            },
        );
        if open {
            out.push(Msg::OpenPreset(i));
        }
        if edit {
            out.push(Msg::EditPreset(i));
        }
        if delete {
            out.push(Msg::Ask(format!("Delete preset {}?", p.name), Box::new(Msg::DeletePreset(i))));
        }
    }
    ui.add_space(6.0);
    // in a row, so the column doesn't stretch it to full width
    ui.horizontal(|ui| {
        if ui.button(ic(ph::PLUS, "New preset")).clicked() {
            out.push(Msg::NewPreset);
        }
    });
    ui.add_space(6.0);
    ui.weak("Click a preset to open it");
}

/// **Continue without servers**: nothing generates meanwhile; prompt
/// enhance can go on with one of the presets' chat servers.
fn without_window(ctx: &egui::Context, st: &AppState, w: &Without, out: &mut Vec<Msg>) {
    let m = egui::Modal::new(egui::Id::unique("continue_without")).show(ctx, |ui| {
        ui.set_width(500.0);
        ui.heading(ic(ph::ARROW_RIGHT, "Continue without servers"));
        ui.label(if st.servers.is_empty() {
            "Browsing, review and the libraries work, but nothing generates until an sd-server is connected \
             (Servers… in the top bar)."
        } else {
            "The sd-servers in use stop. Nothing generates until one is connected again; queued runs wait."
        });
        if !st.active_runs.is_empty() {
            ui.label(format!(
                "{} run(s) are generating: the images being made now are dropped and made again later.",
                st.active_runs.len()
            ));
        }
        ui.add_space(8.0);
        // one per process; the one in use as it is (it may have been
        // edited in its preset since)
        let mut list: Vec<increart_core::pe::PeServer> = st.config.chat_servers().into_iter().cloned().collect();
        if let Some(c) = &st.pe.server {
            match list.iter_mut().find(|x| x.same_process(c)) {
                Some(x) => *x = c.clone(),
                None => list.insert(0, c.clone()),
            }
        }
        if list.is_empty() {
            ui.weak("Prompt enhance: no preset has a chat server.");
        } else {
            ui.label(RichText::new(ic(ph::MAGIC_WAND, "Prompt enhance meanwhile")).strong());
            let mut pick = w.chat.clone();
            ui.radio_value(&mut pick, None, "None");
            for c in &list {
                ui.radio_value(&mut pick, Some(c.clone()), &c.name).on_hover_text(crate::pe_settings::describe(c));
            }
            if pick != w.chat {
                out.push(Msg::WithoutDialog(Some(Without { chat: pick })));
            }
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button(RichText::new(ic(ph::ARROW_RIGHT, "Continue")).strong()).clicked() {
                out.push(Msg::ContinueWithoutServers);
            }
            if ui.button("Cancel").clicked() {
                out.push(Msg::WithoutDialog(None));
            }
        });
    });
    if m.should_close() {
        out.push(Msg::WithoutDialog(None));
    }
}

/// **sd-server settings…**: shared by every Start server.
fn settings_window(ctx: &egui::Context, st: &AppState, d: &ServerDialog, out: &mut Vec<Msg>) {
    let m = egui::Modal::new(egui::Id::unique("sd_server_settings")).show(ctx, |ui| {
        ui.set_width(560.0);
        ui.heading(ic(ph::GEAR, "sd-server settings"));
        ui.label("Used by every preset's Start servers.");
        ui.add_space(8.0);
        settings(ui, st, d, out);
        ui.add_space(8.0);
        if ui.button("Close").clicked() {
            out.push(Msg::ServerSettings(false));
        }
    });
    if m.should_close() {
        out.push(Msg::ServerSettings(false));
    }
}

fn settings(ui: &mut egui::Ui, st: &AppState, d: &ServerDialog, out: &mut Vec<Msg>) {
    sdserver_fields(ui, st, out);
    ui.add_space(12.0);
    ui.label(RichText::new("Devices").strong());
    ui.horizontal(|ui| {
        let label = if d.devices.is_some() { "List again" } else { "List devices (sd-server --list-devices)" };
        if ui.button(ic(ph::LIST_MAGNIFYING_GLASS, label)).clicked() {
            out.push(Msg::ListDevices);
        }
    });
    match &d.devices {
        None => {}
        Some(Ok(list)) if list.is_empty() => {
            ui.weak("none listed");
        }
        Some(Ok(list)) => {
            for l in list {
                ui.monospace(l);
            }
        }
        Some(Err(e)) => {
            ui.colored_label(BAD, e);
        }
    }
}

/// The sd-server binary, models folder and leave-running setting.
fn sdserver_fields(ui: &mut egui::Ui, st: &AppState, out: &mut Vec<Msg>) {
    let mut c = st.config.sdserver.clone();
    ui.horizontal(|ui| {
        ui.add_sized([56.0, 18.0], egui::Label::new("Binary"));
        ui.add(egui::TextEdit::singleline(&mut c.binary).desired_width(ui.available_width() - 36.0));
        if ui.button(ph::FOLDER_OPEN).on_hover_text("Browse…").clicked() {
            out.push(Msg::Pick(PickKind::SdServerBinary));
        }
    });
    ui.horizontal(|ui| {
        ui.add_sized([56.0, 18.0], egui::Label::new("Models"));
        let mut dir = c.model_dirs.first().cloned().unwrap_or_default();
        ui.add(
            egui::TextEdit::singleline(&mut dir).desired_width(ui.available_width() - 36.0).hint_text("--model-dir"),
        );
        if ui.button(ph::FOLDER_OPEN).on_hover_text("Browse…").clicked() {
            out.push(Msg::Pick(PickKind::ModelDir));
        }
        if dir != c.model_dirs.first().cloned().unwrap_or_default() {
            match c.model_dirs.first_mut() {
                Some(first) => *first = dir,
                None => c.model_dirs.push(dir),
            }
            c.model_dirs.retain(|d| !d.is_empty());
        }
    });
    ui.checkbox(&mut c.leave_running, "Leave started servers running when IncreArt exits")
        .on_hover_text("The model stays loaded, so the next start is instant; the GPU memory stays in use meanwhile");
    if c != st.config.sdserver {
        out.push(Msg::SetSdServer(c));
    }
}

/// The server setup wizard: sd-server, the GPUs to start it on (all GPUs
/// ticked), or a server already running; saved as a preset and opened.
fn server_wizard(ctx: &egui::Context, st: &AppState, d: &ServerDialog, out: &mut Vec<Msg>) {
    let m = egui::Modal::new(egui::Id::unique("server_wizard")).show(ctx, |ui| {
        ui.set_width(560.0);
        ui.heading(ic(ph::MAGIC_WAND, "Set up servers"));
        ui.label("IncreArt runs sd-server on your GPUs, one server per GPU.");
        ui.add_space(8.0);
        ui.label(RichText::new("1. sd-server").strong());
        sdserver_fields(ui, st, out);
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("2. GPUs to start it on").strong());
            if ui.small_button(ic(ph::ARROWS_CLOCKWISE, "Detect again")).clicked() {
                out.push(Msg::ListDevices);
            }
        });
        match &d.devices {
            None => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.weak("sd-server --list-devices…");
                });
            }
            Some(Err(e)) => {
                ui.colored_label(BAD, e);
                ui.weak("Check the binary above, then Detect again.");
            }
            Some(Ok(list)) if list.is_empty() => {
                ui.weak("No devices listed.");
            }
            Some(Ok(list)) => {
                for line in list {
                    let device = increart_core::launcher::device_name(line);
                    let port = d.draft.servers.iter().find_map(|s| match s {
                        ServerConfig::Start { device: x, port, .. } if x == device => Some(*port),
                        _ => None,
                    });
                    ui.horizontal(|ui| {
                        if ui.checkbox(&mut port.is_some(), line.replace('\t', "  ")).changed() {
                            out.push(Msg::DraftDevice(device.into(), port.is_none()));
                        }
                        if let Some(p) = port {
                            ui.weak(format!("on :{p}"));
                        }
                    });
                }
            }
        }
        ui.add_space(10.0);
        ui.label(RichText::new("Or connect to a server that's already running").strong());
        let connect = d.draft.servers.iter().position(|s| matches!(s, ServerConfig::Connect { .. }));
        ui.horizontal(|ui| {
            if ui.checkbox(&mut connect.is_some(), "Connect to").changed() {
                out.push(match connect {
                    Some(i) => Msg::RemoveServer(i),
                    None => Msg::AddConnectServer,
                });
            }
            if let Some(i) = connect {
                let mut s = d.draft.servers[i].clone();
                if let ServerConfig::Connect { url, .. } = &mut s {
                    ui.add(egui::TextEdit::singleline(url).desired_width(260.0));
                }
                if s != d.draft.servers[i] {
                    out.push(Msg::EditServer(i, s));
                }
            }
        });
        ui.add_space(10.0);
        ui.label(RichText::new("Prompt enhance (optional)").strong());
        ui.weak("A chat model that suggests and rewrites prompt text.");
        let chat = d.draft.servers.iter().position(ServerConfig::is_chat);
        ui.horizontal(|ui| match chat.and_then(|i| d.draft.chat().map(|c| (i, c))) {
            Some((i, c)) => {
                ui.label(RichText::new(&c.name).strong()).on_hover_text(crate::pe_settings::describe(c));
                if ui.small_button(ic(ph::GEAR, "Settings…")).clicked() {
                    out.push(Msg::PeEditorOpen(Some(i)));
                }
                if ui.small_button(ph::X).on_hover_text("Remove").clicked() {
                    out.push(Msg::RemoveServer(i));
                }
            }
            None => {
                if ui.button(ic(ph::PLUS, "Add a chat server…")).clicked() {
                    out.push(Msg::PeEditorOpen(None));
                }
            }
        });
        ui.add_space(10.0);
        if d.draft.servers.is_empty() {
            ui.weak("Tick a GPU, or a server to connect to.");
        } else {
            ui.weak(d.draft.servers.iter().map(describe).collect::<Vec<_>>().join(" · "));
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let ok = !d.draft.servers.is_empty();
            let finish = egui::Button::new(RichText::new(ic(ph::PLAY, "Save as preset and open")).strong());
            if ui.add_enabled(ok, finish).clicked() {
                out.push(Msg::FinishServerWizard);
            }
            if ui.button("Cancel").clicked() {
                out.push(Msg::ServerWizard(false));
            }
        });
    });
    if m.should_close() {
        out.push(Msg::ServerWizard(false));
    }
}

/// The editor (✎ / New preset): name, servers, Save / Save and open / Open
/// without saving / Cancel. Esc cancels; a click outside doesn't.
fn editor_window(ctx: &egui::Context, st: &AppState, d: &ServerDialog, editing: Editing, out: &mut Vec<Msg>) {
    egui::Modal::new(egui::Id::unique("preset_editor")).show(ctx, |ui| {
        ui.set_width(620.0);
        editor(ui, d, editing, out);
        if st.confirm.is_none() && st.pe.editor.is_none() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            out.push(Msg::CancelEdit);
        }
    });
}

fn editor(ui: &mut egui::Ui, d: &ServerDialog, editing: Editing, out: &mut Vec<Msg>) {
    let title = if editing == Editing::New { "New preset" } else { "Edit preset" };
    ui.heading(ic(ph::PENCIL_SIMPLE, title));
    let mut name = d.draft.name.clone();
    ui.add(egui::TextEdit::singleline(&mut name).desired_width(240.0).hint_text("preset name"));
    if name != d.draft.name {
        out.push(Msg::SetPresetName(name));
    }
    ui.add_space(6.0);
    if d.draft.servers.is_empty() {
        ui.weak("Add an sd-server to start or connect to, and a chat server for prompt enhance if you like.");
    }
    for (i, s) in d.draft.servers.iter().enumerate() {
        let mut edited = s.clone();
        // full width, × pinned to the right edge
        let remove = egui::Frame::group(ui.style())
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                egui::Sides::new().show(
                    ui,
                    |ui| match &mut edited {
                        ServerConfig::Start { name, port, device, .. } => {
                            ui.label(ic(ph::PLAY_CIRCLE, "Start"));
                            ui.add(egui::TextEdit::singleline(name).desired_width(70.0).hint_text("name"));
                            ui.label("device");
                            ui.add(egui::TextEdit::singleline(device).desired_width(80.0));
                            ui.label("port");
                            ui.add(egui::DragValue::new(port).range(1024..=65535));
                        }
                        ServerConfig::Connect { name, url, device } => {
                            ui.label(ic(ph::PLUGS_CONNECTED, "Connect"));
                            ui.add(egui::TextEdit::singleline(name).desired_width(70.0).hint_text("name"));
                            ui.add(egui::TextEdit::singleline(url).desired_width(200.0));
                            ui.add(egui::TextEdit::singleline(device).desired_width(70.0).hint_text("device"));
                        }
                        ServerConfig::Chat(c) => {
                            ui.label(ic(ph::MAGIC_WAND, "Chat"));
                            ui.label(RichText::new(&c.name).strong()).on_hover_text(crate::pe_settings::describe(c));
                            if ui.small_button(ic(ph::GEAR, "Settings…")).clicked() {
                                out.push(Msg::PeEditorOpen(Some(i)));
                            }
                        }
                    },
                    |ui| ui.small_button(ph::X).on_hover_text("Remove").clicked(),
                )
            })
            .inner
            .1;
        if remove {
            out.push(Msg::RemoveServer(i));
        }
        if &edited != s {
            out.push(Msg::EditServer(i, edited));
        }
    }
    ui.horizontal(|ui| {
        if ui.button(ic(ph::PLUS, "Start server")).clicked() {
            out.push(Msg::AddStartServer);
        }
        if ui.button(ic(ph::PLUS, "Connect")).clicked() {
            out.push(Msg::AddConnectServer);
        }
        let chat = d.draft.chat().is_some();
        if ui
            .add_enabled(!chat, egui::Button::new(ic(ph::MAGIC_WAND, "Chat server")))
            .on_hover_text("the chat model prompt enhance uses; one per preset")
            .on_disabled_hover_text("a preset has one chat server")
            .clicked()
        {
            out.push(Msg::PeEditorOpen(None));
        }
    });
    // the listed devices: a click adds a Start server on one
    ui.horizontal_wrapped(|ui| {
        ui.weak("Devices:");
        match &d.devices {
            None => {
                ui.spinner();
            }
            Some(Err(_)) => {
                ui.weak("couldn't list them (sd-server settings…)");
            }
            Some(Ok(list)) => {
                for device in list.iter().map(|l| increart_core::launcher::device_name(l)) {
                    let used = d
                        .draft
                        .servers
                        .iter()
                        .any(|s| matches!(s, ServerConfig::Start { device: x, .. } if x == device));
                    if ui
                        .add_enabled(!used, egui::Button::new(ic(ph::PLUS, device)).small())
                        .on_hover_text("Start a server on this device")
                        .clicked()
                    {
                        out.push(Msg::DraftDevice(device.into(), true));
                    }
                }
            }
        }
    });
    ui.colored_label(Color32::GRAY, "Ports default to 1234, 1235, …; device names as `--list-devices` prints them.");
    ui.add_space(8.0);
    let has_servers = !d.draft.servers.is_empty();
    ui.horizontal(|ui| {
        if ui.button(ic(ph::FLOPPY_DISK, "Save")).clicked() {
            out.push(Msg::SavePreset);
        }
        if ui.add_enabled(has_servers, egui::Button::new(ic(ph::PLAY, "Save and open"))).clicked() {
            out.push(Msg::SaveAndOpen);
        }
        if ui.add_enabled(has_servers, egui::Button::new("Open without saving")).clicked() {
            out.push(Msg::OpenDraft);
        }
        if ui.button("Cancel").clicked() {
            out.push(Msg::CancelEdit);
        }
    });
}

/// An opening library has no count to show, so the bar fills over time,
/// slowing as it goes (half in a second, never quite full).
fn loading_fraction(ui: &egui::Ui, id: egui::Id) -> f32 {
    let now = ui.input(|i| i.time);
    let since = ui.ctx().data_mut(|d| *d.get_temp_mut_or_insert_with(id, || now));
    (1.0 - 0.5f64.powf(now - since)) as f32
}
