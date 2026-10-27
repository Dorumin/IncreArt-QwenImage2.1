//! The eframe app: feeds runtime messages and UI messages through the
//! reducer, carries out effects, and draws the current state.

use std::path::PathBuf;

use eframe::egui::{self, Color32, Panel, RichText, Sense};
use increart_core::config::Config;
use increart_core::prompts::PromptPresets;
use increart_gui_core::{AppState, Msg, Phase, ProjectsTab, Runtime, Tab, update};

use crate::textures::Textures;
use crate::tour::Tour;
use crate::widgets::{BAD, GOOD, NEUTRAL, ic, ph};
use crate::{ideas, projects, review};

pub struct App {
    st: AppState,
    rt: Runtime,
    tex: Textures,
    tour: Option<Tour>,
    shown_library: Option<PathBuf>,
    /// The app's clock (`Msg::Tick`).
    started: std::time::Instant,
    /// The window manager was asked for attention (taskbar flash; on X11 the
    /// urgency hint, which stays until it is reset).
    attention: bool,
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        config: Config,
        config_path: Option<PathBuf>,
        warnings: Vec<String>,
        startup: crate::Startup,
    ) -> App {
        let ctx = cc.egui_ctx.clone();
        let rt = Runtime::new(move || ctx.request_repaint());
        let mut st = AppState::new(config, config_path);
        for w in warnings {
            st.messages_info(w);
        }
        if let Some(path) = st.config_path.as_deref().map(PromptPresets::dir_beside) {
            match PromptPresets::load_dir(&path) {
                Ok((p, bad)) => {
                    st.prompt_presets = p;
                    st.presets_path = Some(path);
                    for e in bad {
                        st.messages_error(e);
                    }
                }
                // not saved over: the file may be fixable by hand
                Err(e) => st.messages_error(format!("prompt presets not loaded ({e}); using the built-in ones")),
            }
        }
        st.startup_servers = startup.servers;
        let tour = Tour::from_env();
        let mut app = App {
            st,
            rt,
            tex: Textures::new(&cc.egui_ctx),
            tour,
            shown_library: None,
            started: std::time::Instant::now(),
            attention: false,
        };
        if let Some(asked) = startup.library {
            app.skip_library(&asked);
        }
        app
    }

    /// The command line's `--library`: opens it if it's a library, else
    /// leaves the library screen up, saying why.
    fn skip_library(&mut self, asked: &str) {
        let path = if asked == "last" { self.st.config.recent_libraries.first().cloned() } else { Some(asked.into()) };
        let problem = match &path {
            None => Some("no library opened yet".to_string()),
            Some(p) if crate::startup::folder_state(p) != crate::startup::FolderState::Library => {
                Some("not an IncreArt library (no library.rsn)".into())
            }
            Some(_) => None,
        };
        match (problem, path) {
            (None, Some(p)) => {
                self.dispatch(Msg::SetLibraryPath(p));
                self.dispatch(Msg::OpenLibrary);
            }
            (Some(e), _) => {
                if let Phase::Library(d) = &mut self.st.phase {
                    d.error = Some(format!("--library {asked}: {e}"));
                }
            }
            _ => {}
        }
    }

    /// The interface zoom: Ctrl + / − / 0 here instead of egui's own, so the
    /// config keeps it, then the zoom applied (egui does at the next pass).
    fn zoom(&mut self, ctx: &egui::Context) {
        use egui::gui_zoom::kb_shortcuts as kb;
        ctx.options_mut(|o| o.zoom_with_keyboard = false);
        let (reset, zoom_in, zoom_out) = ctx.input_mut(|i| {
            (
                i.consume_shortcut(&kb::ZOOM_RESET),
                i.consume_shortcut(&kb::ZOOM_IN) | i.consume_shortcut(&kb::ZOOM_IN_SECONDARY),
                i.consume_shortcut(&kb::ZOOM_OUT),
            )
        });
        if reset {
            self.dispatch(Msg::UiZoom { percent: 100, save: true });
        }
        if zoom_in {
            self.dispatch(Msg::UiZoomStep(true));
        }
        if zoom_out {
            self.dispatch(Msg::UiZoomStep(false));
        }
        let want = self.st.ui_zoom() as f32 / 100.0;
        if ctx.zoom_factor() != want {
            ctx.set_zoom_factor(want);
        }
    }

    fn dispatch(&mut self, msg: Msg) {
        for fx in update(&mut self.st, msg) {
            self.rt.run(fx);
        }
    }

    fn main_window(&mut self, ui: &mut egui::Ui, out: &mut Vec<Msg>) {
        let st = &self.st;
        ideas::global_keys(ui.ctx(), out);
        nav_keys(ui.ctx(), st, out);
        crate::widgets::drag_overlay(ui.ctx());
        Panel::top("tabs").show(ui, |ui| {
            ui.horizontal(|ui| {
                let back = ui
                    .add_enabled(!st.nav.back.is_empty(), egui::Button::new(ph::ARROW_LEFT))
                    .on_hover_text("Back (Alt+←, mouse back)");
                if back.clicked() {
                    out.push(Msg::NavBack);
                }
                let forward = ui
                    .add_enabled(!st.nav.forward.is_empty(), egui::Button::new(ph::ARROW_RIGHT))
                    .on_hover_text("Forward (Alt+→, mouse forward)");
                if forward.clicked() {
                    out.push(Msg::NavForward);
                }
                ui.add_space(6.0);
                ui.label(RichText::new("IncreArt").strong().size(18.0));
                ui.add_space(16.0);
                // Each tab is an icon, its name and a count; short of room
                // (a narrow window, a large zoom) the names go, leaving
                // "📥 (7)". The right side gets what it needs at its compact
                // size first.
                let projects_icon = if st.active_runs.is_empty() { ph::ROCKET_LAUNCH } else { ph::SPARKLE };
                let tabs = [
                    (Tab::IdeaInbox, ph::TRAY, "Idea Inbox", st.unreviewed_ideas().len()),
                    (Tab::InputLibrary, ph::BOOKS, "Input Library", st.library_ideas_all()),
                    (Tab::Projects, projects_icon, "Projects", st.index.projects.len()),
                    (Tab::Review, ph::THUMBS_UP, "Review", st.unreviewed_outputs()),
                    (Tab::OutputLibrary, ph::IMAGES, "Output Library", st.kept_outputs().len()),
                ];
                let text = |icon: &str, name: Option<&str>, n: usize| {
                    let t = match name {
                        Some(name) => format!("{icon} {name}  ({n})"),
                        None => format!("{icon} ({n})"),
                    };
                    RichText::new(t).size(15.0)
                };
                let width = |ui: &egui::Ui, t: RichText| {
                    egui::WidgetText::from(t)
                        .into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Button)
                        .size()
                        .x
                        + 2.0 * ui.spacing().button_padding.x
                        + ui.spacing().item_spacing.x
                };
                let full: f32 = tabs.iter().map(|(_, i, name, n)| width(ui, text(i, Some(name), *n))).sum();
                let short = full > ui.available_width() - TOP_RIGHT_MIN;
                for (tab, icon, name, n) in tabs {
                    let r = ui.selectable_label(st.tab == tab, text(icon, (!short).then_some(name), n));
                    let r = if short { r.on_hover_text(format!("{name} ({n})")) } else { r };
                    if r.clicked() {
                        out.push(Msg::Tab(tab));
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // short of room (a narrow window): shorter buttons
                    let compact = ui.available_width() < 320.0;
                    if ui.button(ph::GEAR).on_hover_text("Settings (notifications)").clicked() {
                        out.push(Msg::Settings(true));
                    }
                    library_menu(ui, st, out, compact);
                    server_statuses(ui, st, out, compact);
                });
            });
        });
        Panel::bottom("status").show(ui, |ui| {
            // the right side first, so the path and message shorten instead
            // of running into it
            let force_open = self.tour.as_ref().is_some_and(|t| t.jobs_popover());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                jobs_indicator(ui, st, out, force_open);
                crate::settings::bell(ui, st, out);
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(2.0);
                zoom_control(ui, st, out);
                if let Some((done, total)) = st.usage_check {
                    ui.add(
                        egui::ProgressBar::new(done as f32 / total.max(1) as f32)
                            .desired_width(120.0)
                            .desired_height(12.0)
                            .text(RichText::new(format!("{done} / {total}")).small()),
                    )
                    .on_hover_text("Checking each image's usage counts against the runs (after opening the library)");
                    ui.weak(RichText::new("checking images").small());
                    ui.separator();
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    if st.busy > 0 {
                        ui.spinner();
                    }
                    let latest = st.messages.iter().enumerate().next_back();
                    if let Some(p) = &st.library {
                        // the message, if any, gets the room first
                        let room = if latest.is_some() { ui.available_width() * 0.35 } else { ui.available_width() };
                        ui.scope(|ui| {
                            ui.set_max_width(room);
                            let path = RichText::new(p.display().to_string()).weak();
                            let path = egui::Label::new(path).truncate().sense(Sense::click());
                            let r = ui
                                .add(path)
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .on_hover_text(format!("{}\nopen the library in the file manager", p.display()));
                            if r.clicked() {
                                out.push(Msg::ShowInFileManager(p.clone()));
                            }
                        });
                    }
                    if let Some((i, m)) = latest {
                        ui.separator();
                        if ui.small_button(ph::X).clicked() {
                            out.push(Msg::DismissMessage(i));
                        }
                        let force_open = self.tour.as_ref().is_some_and(|t| t.messages_popover());
                        messages_line(ui, st, m, out, force_open);
                    }
                });
            });
        });
        let tex = &mut self.tex;
        match st.tab {
            Tab::IdeaInbox => ideas::inbox(ui, st, tex, out),
            Tab::InputLibrary => ideas::input_library(ui, st, tex, out),
            Tab::Projects => projects::view(ui, st, tex, out),
            Tab::Review => review::view(ui, st, tex, out),
            Tab::OutputLibrary => review::output_library(ui, st, tex, out),
        }
    }
}

/// What the top bar's right side (statuses, Servers…, library, ⚙) needs
/// at its compact size; the tabs drop their names to leave it.
const TOP_RIGHT_MIN: f32 = 340.0;

/// Top right, left of the Servers… button: each server's and the PE
/// server's status, as long as there is room: in full, then the name and
/// dot, then the dots; with no room for those, the Servers… button shows
/// the worst dot. The full status is always in the tooltip.
fn server_statuses(ui: &mut egui::Ui, st: &AppState, out: &mut Vec<Msg>, compact: bool) {
    struct Item {
        name: String,
        dot: &'static str,
        state: String,
        color: Color32,
        tip: String,
        jobs: bool,
    }
    let busy = st.progress.keys().count();
    let mut items: Vec<Item> = st
        .servers
        .iter()
        .map(|s| {
            let (dot, color) = match s.state.as_str() {
                "ready" => ("●", GOOD),
                st if st.contains("retrying") => ("●", BAD),
                _ => ("◐", NEUTRAL),
            };
            let name = if s.device.is_empty() { s.name.clone() } else { s.device.clone() };
            let model = s.model.as_ref().map(|m| format!(", {m}")).unwrap_or_default();
            let tip = format!("{}: {}{model}\n{busy} seeds generating · click for the server status", s.name, s.state);
            Item { name, dot, state: s.state.clone(), color, tip, jobs: true }
        })
        .collect();
    // stopped ones, until their processes are gone and the GPU memory free
    let stopping: Vec<&String> = st.stopping.iter().filter(|n| !items.iter().any(|i| &i.name == *n)).collect();
    items.extend(stopping.into_iter().map(|n| Item {
        name: n.clone(),
        dot: "◌",
        state: "stopping…".into(),
        color: NEUTRAL,
        tip: format!("{n}: stopping: waiting for sd-server to exit and free its GPU memory"),
        jobs: false,
    }));
    if items.is_empty() {
        let weak = ui.visuals().weak_text_color();
        let tip = "No sd-server in use: Servers… to start or connect one".to_string();
        items.push(Item { name: "no servers".into(), dot: "", state: String::new(), color: weak, tip, jobs: false });
    }
    let weak = ui.visuals().weak_text_color();
    let pe = Item {
        name: "PE".into(),
        dot: "○",
        state: "not connected".into(),
        color: weak,
        tip: "Prompt enhance (PE): not connected\nclick for the server status".into(),
        jobs: false,
    };
    items.push(pe);
    let texts = |level: usize| -> Vec<String> {
        items
            .iter()
            .filter(|i| level < 2 || !i.dot.is_empty())
            .map(|i| match level {
                0 => [i.name.as_str(), i.dot, i.state.as_str()]
                    .iter()
                    .filter(|t| !t.is_empty())
                    .copied()
                    .collect::<Vec<_>>()
                    .join(" "),
                1 => {
                    if i.dot.is_empty() {
                        i.name.clone()
                    } else {
                        format!("{} {}", i.name, i.dot)
                    }
                }
                _ => i.dot.to_string(),
            })
            .collect()
    };
    let width = |t: &str| {
        let text: egui::WidgetText = t.into();
        text.into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Body).size().x
    };
    let gap = ui.spacing().item_spacing.x;
    let title = if compact { String::new() } else { "  Servers…".to_string() };
    let button = width(&format!("{}{title} ●", ph::HARD_DRIVES)) + 2.0 * ui.spacing().button_padding.x + gap;
    let room = ui.available_width() - button - 16.0;
    let level = (0..3).find(|&l| texts(l).iter().map(|t| width(t) + gap).sum::<f32>() <= room);

    // the worst state on the button when the statuses don't fit
    let worst = items.iter().filter(|i| i.jobs).max_by_key(|i| match i.color {
        c if c == BAD => 2,
        c if c == NEUTRAL => 1,
        _ => 0,
    });
    let mut label = RichText::new(format!("{}{title}", ph::HARD_DRIVES));
    if level.is_none()
        && let Some(w) = worst
    {
        label = RichText::new(format!("{}{title} {}", ph::HARD_DRIVES, w.dot)).color(w.color);
    }
    let tips: Vec<&str> = items.iter().map(|i| i.tip.lines().next().unwrap_or("")).collect();
    let hover = if level.is_none() { format!("Server status\n{}", tips.join("\n")) } else { "Server status".into() };
    if ui.button(label).on_hover_text(hover).clicked() {
        out.push(Msg::ServerStatus(true));
    }
    let Some(level) = level else { return };
    let shown: Vec<&Item> = items.iter().filter(|i| level < 2 || !i.dot.is_empty()).collect();
    for (i, text) in shown.iter().zip(texts(level)).rev() {
        let r =
            ui.add(egui::Label::new(RichText::new(text).color(i.color)).sense(Sense::click())).on_hover_text(&i.tip);
        if r.clicked() {
            out.push(Msg::ServerStatus(true));
        }
    }
}

/// Top right: the open library, and a menu to switch to another (the
/// servers stay connected).
fn library_menu(ui: &mut egui::Ui, st: &AppState, out: &mut Vec<Msg>, compact: bool) {
    let Some(root) = &st.library else { return };
    let mut name = root.file_name().map_or(root.display().to_string(), |n| n.to_string_lossy().into_owned());
    let max = if compact { 10 } else { 24 };
    if name.chars().count() > max {
        name = format!("{}…", name.chars().take(max - 1).collect::<String>());
    }
    let r = ui.menu_button(ic(ph::BOOKS, &format!("{name} {}", ph::CARET_DOWN)), |ui| {
        ui.set_min_width(320.0);
        ui.label(RichText::new("Libraries").strong());
        let recent = &st.config.recent_libraries;
        for path in recent {
            let current = std::path::Path::new(path) == root.as_path();
            let text = if current { ic(ph::CHECK, path) } else { ic(ph::BOOKS, path) };
            let r = ui.add_enabled(!current, egui::Button::new(text).frame(false));
            let r = r.on_disabled_hover_text("open now");
            if r.on_hover_text("switch to this library").clicked() {
                out.push(Msg::SwitchLibrary(path.clone()));
                ui.close();
            }
        }
        ui.separator();
        if ui.button(ic(ph::FOLDER_OPEN, "Open an existing library…")).clicked() {
            let first = crate::startup::found_libraries(recent).into_iter().next().map(|(_, p)| p).unwrap_or_default();
            out.push(Msg::OtherLibrary(Some((increart_gui_core::LibraryWizard::Open, first))));
            ui.close();
        }
        if ui.button(ic(ph::FOLDER_PLUS, "Create a new library…")).clicked() {
            let first = crate::startup::suggested_libraries().into_iter().next().map(|(_, p)| p).unwrap_or_default();
            out.push(Msg::OtherLibrary(Some((increart_gui_core::LibraryWizard::Create, first))));
            ui.close();
        }
        if ui.button(ic(ph::FOLDER_OPEN, "Show in the file manager")).clicked() {
            out.push(Msg::ShowInFileManager(root.clone()));
            ui.close();
        }
        ui.weak("Switching keeps the servers connected.");
    });
    r.response.on_hover_text(format!("Library: {}\nclick to switch libraries", root.display()));
}

/// The interface zoom, "+ 100% −" in the status bar: the buttons step by
/// 10%, the value can be typed (click) or dragged.
fn zoom_control(ui: &mut egui::Ui, st: &AppState, out: &mut Vec<Msg>) {
    use increart_gui_core::{UI_ZOOM_MAX, UI_ZOOM_MIN};
    let z = st.ui_zoom();
    ui.scope(|ui| {
        // right to left: the minus first
        ui.spacing_mut().item_spacing.x = 2.0;
        let small = |s: &str| egui::Button::new(s).small().min_size(egui::vec2(18.0, 0.0));
        let r = ui.add_enabled(z > UI_ZOOM_MIN, small(ph::MINUS));
        if r.on_hover_text("zoom out   Ctrl −").clicked() {
            out.push(Msg::UiZoomStep(false));
        }
        let mut v = z;
        let r = ui
            .add(
                egui::DragValue::new(&mut v)
                    .range(UI_ZOOM_MIN..=UI_ZOOM_MAX)
                    .suffix("%")
                    .speed(0.5)
                    // typing "150" isn't 1%, then 15%
                    .update_while_editing(false),
            )
            .on_hover_text("Interface zoom: click to type, drag to adjust\nCtrl + / Ctrl − / Ctrl 0 (100%)");
        if r.changed() || r.drag_stopped() {
            out.push(Msg::UiZoom { percent: v, save: !r.dragged() });
        }
        let r = ui.add_enabled(z < UI_ZOOM_MAX, small(ph::PLUS));
        if r.on_hover_text("zoom in   Ctrl +").clicked() {
            out.push(Msg::UiZoomStep(true));
        }
    });
    ui.add_space(6.0);
}

/// The latest message on the status line ("(+1 more) All GPUs idle …");
/// click for a popover with all of them, newest first, and Clear all.
fn messages_line(
    ui: &mut egui::Ui,
    st: &AppState,
    latest: &increart_gui_core::Message,
    out: &mut Vec<Msg>,
    force_open: bool,
) {
    let resp = ui
        .scope(|ui| {
            ui.style_mut().interaction.selectable_labels = false;
            if st.messages.len() > 1 {
                ui.weak(format!("(+{} more)", st.messages.len() - 1));
            }
            let c = if latest.error { BAD } else { Color32::LIGHT_GRAY };
            ui.add(egui::Label::new(RichText::new(&latest.text).color(c)).truncate());
        })
        .response
        .interact(Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(format!("{}\nclick for all messages", latest.text));
    let mut popup = egui::Popup::from_toggle_button_response(&resp);
    if force_open {
        popup = popup.open_memory(egui::SetOpenCommand::Bool(true));
    }
    popup
        .align(egui::RectAlign::TOP_START)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .width(520.0)
        .show(|ui| {
            ui.set_min_width(520.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(ic(ph::CHAT_TEXT, &format!("Messages ({})", st.messages.len()))).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(ic(ph::BROOM, "Clear all")).clicked() {
                        out.push(Msg::ClearMessages);
                        egui::Popup::close_all(ui.ctx());
                    }
                });
            });
            ui.separator();
            egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                for (i, m) in st.messages.iter().enumerate().rev() {
                    ui.horizontal(|ui| {
                        if ui.small_button(ph::X).on_hover_text("dismiss").clicked() {
                            out.push(Msg::DismissMessage(i));
                        }
                        let (icon, c) = if m.error { (ph::WARNING, BAD) } else { (ph::INFO, Color32::LIGHT_GRAY) };
                        ui.add(egui::Label::new(RichText::new(format!("{icon} {}", m.text)).color(c)).wrap());
                    });
                }
            });
        });
}

/// Bottom-right: a summary of the runs in the engine with an overall
/// progress bar; click for a popover with each run and the servers.
fn jobs_indicator(ui: &mut egui::Ui, st: &AppState, out: &mut Vec<Msg>, force_open: bool) {
    let sum = st.job_summary();
    let resp = ui
        .horizontal(|ui| {
            // a click on the text opens the popover, not a selection
            ui.style_mut().interaction.selectable_labels = false;
            if sum.runs == 0 {
                let servers = st.servers.iter().filter(|s| s.state == "ready").count();
                let text = if st.servers.is_empty() {
                    "no jobs".to_string()
                } else {
                    format!("no jobs · {servers} server(s) ready")
                };
                ui.weak(ic(ph::QUEUE, &text));
            } else {
                ui.add(egui::ProgressBar::new(sum.fraction).desired_width(140.0).desired_height(12.0));
                ui.label(ic(ph::SPARKLE, &format!("{} run(s) · {} / {} seeds", sum.runs, sum.done, sum.total)));
            }
        })
        .response
        .interact(Sense::click())
        .on_hover_text("Jobs: click for details");
    let mut popup = egui::Popup::from_toggle_button_response(&resp);
    if force_open {
        popup = popup.open_memory(egui::SetOpenCommand::Bool(true));
    }
    popup
        .align(egui::RectAlign::TOP_END)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .width(440.0)
        .show(|ui| {
            ui.set_min_width(440.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(ic(ph::QUEUE, "Jobs")).strong());
                ui.add_space(12.0);
                crate::projects::order_toggle(ui, st, out);
            });
            let n = out.len();
            egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| crate::projects::run_list(ui, st, out));
            if out[n..].iter().any(|m| matches!(m, Msg::JobInfo(_))) {
                egui::Popup::close_all(ui.ctx());
            }
            ui.separator();
            ui.label(RichText::new(ic(ph::HARD_DRIVES, "Servers")).strong());
            crate::projects::server_list(ui, st);
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Open Projects → Jobs").clicked() {
                    out.push(Msg::Tab(Tab::Projects));
                    out.push(Msg::ProjectsTab(ProjectsTab::Jobs));
                    egui::Popup::close_all(ui.ctx());
                }
                if ui.button(ic(ph::GEAR, "Change servers…")).clicked() {
                    out.push(Msg::OpenServerDialog);
                    egui::Popup::close_all(ui.ctx());
                }
            });
        });
}

/// The Export… dialog: destination, metadata, file names.
fn export_window(ctx: &egui::Context, e: &increart_gui_core::ExportDialog, out: &mut Vec<Msg>) {
    let mut d = e.clone();
    let mut open = true;
    egui::Window::new(ic(ph::EXPORT, &format!("Export {}", e.what)))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.label(format!("{} image(s), copied as PNG; the library is unchanged.", d.images.len()));
            ui.horizontal(|ui| {
                ui.label("To");
                ui.add(egui::TextEdit::singleline(&mut d.dest).desired_width(320.0).hint_text("/path/to/folder"));
                if ui.button(ph::FOLDER_OPEN).on_hover_text("Browse…").clicked() {
                    out.push(Msg::Pick(increart_gui_core::PickKind::ExportDir));
                }
            });
            ui.checkbox(&mut d.keep_metadata, "keep metadata (recipe, sd-server parameters, EXIF)");
            ui.horizontal(|ui| {
                ui.label("Names:");
                ui.radio_value(&mut d.by_id, false, "run / revision / seed");
                ui.radio_value(&mut d.by_id, true, "image ID");
            });
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button(RichText::new(ic(ph::EXPORT, "Export")).strong()).clicked() {
                    out.push(Msg::RunExport);
                }
                if ui.button("Cancel").clicked() {
                    out.push(Msg::CloseExport);
                }
            });
        });
    if &d != e {
        out.push(Msg::EditExport(d));
    }
    if !open {
        out.push(Msg::CloseExport);
    }
}

/// The top bar's **Servers…**: each server's state and model, what is
/// generating, and the way to the server dialog.
fn server_status_window(ctx: &egui::Context, st: &AppState, out: &mut Vec<Msg>) {
    let m = egui::Modal::new(egui::Id::unique("server_status")).show(ctx, |ui| {
        ui.set_width(520.0);
        ui.heading(ic(ph::HARD_DRIVES, "Servers"));
        ui.add_space(6.0);
        crate::projects::server_list(ui, st);
        if st.background.is_some() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.weak("Servers are still starting; they join when they answer.");
            });
        }
        ui.add_space(6.0);
        let seeds = st.progress.keys().count();
        let runs = st.jobs().len();
        ui.weak(match (st.servers.is_empty(), runs) {
            (true, 0) => "Nothing can generate until a server is connected.".to_string(),
            (true, n) => format!("{n} run(s) waiting for a server."),
            (false, 0) => "Nothing generating.".to_string(),
            (false, n) => format!("{n} run(s) in the queue, {seeds} seed(s) generating now."),
        });
        if let Some(p) = &st.config.last_preset
            && !st.servers.is_empty()
        {
            ui.weak(format!("Preset: {p}"));
        }
        ui.add_space(8.0);
        ui.label(RichText::new(ic(ph::MAGIC_WAND, "Prompt enhance (PE)")).strong());
        ui.weak(
            "Rewriting prompts with the Qwen-Image prompt-enhancer model needs its own chat server \
             (llama-server). It isn't built into IncreArt yet, so there is nothing to connect.",
        );
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button(RichText::new(ic(ph::GEAR, "Change servers…")).strong()).clicked() {
                out.push(Msg::OpenServerDialog);
            }
            if ui.button(ic(ph::QUEUE, "Jobs")).on_hover_text("Projects → Jobs").clicked() {
                out.push(Msg::ServerStatus(false));
                out.push(Msg::Tab(Tab::Projects));
                out.push(Msg::ProjectsTab(ProjectsTab::Jobs));
            }
            if ui.button("Close").clicked() {
                out.push(Msg::ServerStatus(false));
            }
        });
    });
    if m.should_close() {
        out.push(Msg::ServerStatus(false));
    }
}

/// An output image's title: Enter or Save keeps it (empty removes it),
/// Escape or Cancel closes.
fn title_window(ctx: &egui::Context, e: &increart_gui_core::TitleEdit, out: &mut Vec<Msg>) {
    let m = egui::Modal::new(egui::Id::unique("image_title")).show(ctx, |ui| {
        ui.set_width(420.0);
        ui.heading(ic(ph::TEXT_AA, "Image title"));
        ui.weak("Shown with the image and found by the Output library's search. Empty removes it.");
        ui.add_space(4.0);
        let mut text = e.text.clone();
        let r = ui.add(egui::TextEdit::singleline(&mut text).hint_text("a title").desired_width(f32::INFINITY));
        r.request_focus();
        if r.changed() {
            out.push(Msg::EditTitle(text));
        }
        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button(RichText::new("Save   Enter").strong()).clicked() || enter {
                out.push(Msg::CommitTitle);
            }
            if ui.button("Cancel").clicked() {
                out.push(Msg::TitleDialog(None));
            }
        });
    });
    if m.should_close() {
        out.push(Msg::TitleDialog(None));
    }
}

/// After Queue with no server, once per session: the run would only wait.
fn no_server_window(ctx: &egui::Context, out: &mut Vec<Msg>) {
    let m = egui::Modal::new(egui::Id::unique("no_server")).show(ctx, |ui| {
        ui.set_width(440.0);
        ui.heading(RichText::new(ic(ph::WARNING, "No server is connected")).color(NEUTRAL));
        ui.add_space(4.0);
        ui.label(
            "The run will wait in the queue until a server joins. You won't be asked again until IncreArt restarts.",
        );
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button(RichText::new(ic(ph::GEAR, "Set up servers…")).strong()).clicked() {
                out.push(Msg::CloseNoServerWarning);
                out.push(Msg::OpenServerDialog);
            }
            if ui.button(ic(ph::QUEUE, "Queue anyway")).clicked() {
                out.push(Msg::CloseNoServerWarning);
                out.push(Msg::QueueGenerate);
            }
            if ui.button("Cancel").clicked() {
                out.push(Msg::CloseNoServerWarning);
            }
        });
    });
    if m.should_close() {
        out.push(Msg::CloseNoServerWarning);
    }
}

/// Back / forward from the keyboard and the mouse's side buttons, except
/// in a folder view (which goes through its own history with them).
fn nav_keys(ctx: &egui::Context, st: &AppState, out: &mut Vec<Msg>) {
    if st.folder_view_shown() || st.confirm.is_some() || ctx.egui_wants_keyboard_input() {
        return;
    }
    ctx.input(|i| {
        if (i.modifiers.alt && i.key_pressed(egui::Key::ArrowLeft))
            || i.pointer.button_pressed(egui::PointerButton::Extra1)
        {
            out.push(Msg::NavBack);
        }
        if (i.modifiers.alt && i.key_pressed(egui::Key::ArrowRight))
            || i.pointer.button_pressed(egui::PointerButton::Extra2)
        {
            out.push(Msg::NavForward);
        }
    });
}

/// The Yes / Cancel question of `AppState::confirm`.
fn confirm_window(ctx: &egui::Context, text: &str, out: &mut Vec<Msg>) {
    let m = egui::Modal::new(egui::Id::unique("confirm")).show(ctx, |ui| {
        ui.set_max_width(420.0);
        ui.heading("Are you sure?");
        ui.label(text);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button(RichText::new("Yes").color(BAD)).clicked() {
                out.push(Msg::ConfirmYes);
            }
            if ui.button("Cancel   Esc").clicked() {
                out.push(Msg::ConfirmNo);
            }
        });
    });
    if m.should_close() {
        out.push(Msg::ConfirmNo);
    }
}

impl Drop for App {
    /// Saves pending drafts and stops (or, if so set, leaves running) the
    /// servers the app started, before the window closes.
    fn drop(&mut self) {
        for fx in update(&mut self.st, Msg::FlushDrafts) {
            self.rt.save_now(fx);
        }
        self.rt.shutdown(self.st.config.sdserver.leave_running);
    }
}

/// "draft saved" / "unsaved changes…" next to a form.
pub fn draft_note(ui: &mut egui::Ui, st: &AppState, dirty: Option<f64>) {
    if dirty.is_some() {
        ui.weak(ic(ph::PENCIL_SIMPLE, "unsaved changes…"));
    } else if st.draft_saved.is_some_and(|t| st.now - t < 3.0) {
        ui.weak(ic(ph::CHECK, "draft saved"));
    }
}

trait Counts {
    fn library_ideas_all(&self) -> usize;
}

impl Counts for AppState {
    fn library_ideas_all(&self) -> usize {
        self.index.ideas.iter().filter(|e| matches!(e.loc, increart_core::IdeaLoc::Library(_))).count()
    }
}

impl eframe::App for App {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        if let Some(t) = &mut self.tour {
            t.raw_input(raw);
        }
    }

    /// Runs even while the window is minimized or hidden (no `ui` then), so
    /// the clock, the runtime's messages and the notification alerts go on.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // a clock of our own: egui's input time stops while hidden
        self.dispatch(Msg::Tick(self.started.elapsed().as_secs_f64()));
        for m in self.rt.poll() {
            self.dispatch(m);
        }
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        if focused != self.st.alerts.focused {
            self.dispatch(Msg::Focused(focused));
        }
        // the flash is for an unfocused window, and goes once it is focused
        // or nothing is alerting any more
        if std::mem::take(&mut self.st.alerts.attention) && !focused {
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(egui::UserAttentionType::Informational));
            self.attention = true;
        }
        let a = &self.st.alerts;
        if self.attention
            && (focused || (a.idle_text.is_none() && a.cleared_text.is_none() && a.backlog_text.is_none()))
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(egui::UserAttentionType::Reset));
            self.attention = false;
        }
        let n = &self.st.config.notifications;
        if n.gpu_idle.on || n.review_backlog.on {
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.zoom(&ctx);
        if let Some(img) = self.st.clipboard_image.take() {
            let size = [img.0.width() as usize, img.0.height() as usize];
            ctx.copy_image(egui::ColorImage::from_rgba_unmultiplied(size, img.0.as_raw()));
        }
        self.tex.poll(&ctx);
        if self.st.library != self.shown_library {
            self.tex.clear();
            self.shown_library = self.st.library.clone();
        }
        let mut out = vec![];
        if let Some(t) = &mut self.tour {
            t.step(&ctx, &self.st, &self.tex, &mut out);
            if let Some((servers, size)) = t.top_bar() {
                self.st.servers = servers;
                if ctx.input(|i| i.viewport().inner_rect.map(|r| r.size())) != Some(size.into()) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size.into()));
                }
            }
        }
        match self.st.phase.clone() {
            Phase::Library(d) => {
                let recent = &self.st.config.recent_libraries;
                let back = self.st.switching.and(self.st.library.as_deref());
                egui::CentralPanel::default().show(ui, |ui| crate::startup::library(ui, &d, recent, back, &mut out));
            }
            Phase::Servers(d) => {
                let st = &self.st;
                egui::CentralPanel::default().show(ui, |ui| crate::startup::servers(ui, st, &d, &mut out));
            }
            Phase::Main => self.main_window(ui, &mut out),
        }
        if self.st.phase == Phase::Main {
            if let Some(v) = &self.st.history_view {
                crate::history::window(&ctx, &self.st, v, &mut out);
            }
            crate::ideas::edit_window(&ctx, &self.st, &mut self.tex, &mut out);
            crate::settings::window(&ctx, &self.st, &mut out);
            crate::projects::resolutions_window(&ctx, &self.st, &mut out);
            crate::projects::scale_window(&ctx, &self.st, &mut out);
            // under the preview, which its images open
            crate::projects::job_window(&ctx, &self.st, &mut self.tex, &mut out);
            crate::refpicker::window(&ctx, &self.st, &mut self.tex, &mut out);
            if let Some(e) = &self.st.title_edit {
                title_window(&ctx, e, &mut out);
            }
            let keys_open = self.tour.as_ref().is_some_and(|t| t.preview_keys());
            crate::projects::preview_window(&ctx, &self.st, &mut self.tex, keys_open, &mut out);
            crate::projects::retag_window(&ctx, &self.st, &mut out);
            crate::projects::new_project_window(&ctx, &self.st, &mut self.tex, &mut out);
            crate::projects::duplicate_window(&ctx, &self.st, &mut out);
            crate::projects::promote_window(&ctx, &self.st, &mut out);
            crate::projects::sweep_order_window(&ctx, &self.st, &mut out);
            crate::tree::rename_window(&ctx, &self.st, &mut out);
            if self.st.tab == Tab::Projects && self.st.projects_tab == ProjectsTab::Generate {
                crate::presets::window(&ctx, &self.st, &mut out);
            }
            crate::presets::editor_window(&ctx, &self.st, &mut out);
            crate::presets::save_window(&ctx, &self.st, &mut out);
            if let Some(e) = &self.st.export {
                export_window(&ctx, e, &mut out);
            }
            crate::tiles::window(&ctx, &self.st, &mut self.tex, &mut out);
            if self.st.server_status {
                server_status_window(&ctx, &self.st, &mut out);
            }
            if self.st.no_server_warning {
                no_server_window(&ctx, &mut out);
            }
        }
        if let Some(c) = &self.st.confirm {
            confirm_window(&ctx, &c.text, &mut out);
        }
        for m in out {
            self.dispatch(m);
        }
        let opening = matches!(&self.st.phase, Phase::Servers(d) if d.opening) || self.st.background.is_some();
        if self.st.busy > 0
            || self.st.engine_running
            || opening
            || self.st.generate_dirty.is_some()
            || self.st.idea_dirty.is_some()
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
    }
}
