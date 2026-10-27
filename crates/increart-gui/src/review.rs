//! Review (the output queue: Single · Grid · A/B compare) and the Output
//! library (Grid with its Kept · Good · Rejected · Unreviewed · All tabs ·
//! Folder).

use std::path::{Path, PathBuf};

use eframe::egui::{self, Key, Panel, RichText, ScrollArea, Sense, Vec2};
use increart_core::Tree;
use increart_core::Verdict;
use increart_core::scan::OutputLink;
use increart_gui_core::{
    AppState, InfoPanel, Msg, OutputPreset, OutputSource, OutputTab, Ratings, ReviewRun, ReviewView, Smart, Tab,
    UseTarget, counts,
};

use crate::textures::Textures;
use crate::widgets::{
    BAD, GOOD, NEUTRAL, copy_image_item, ic, image_fit_bg, ph, short, single_click, thumb, verdict_color,
};

pub const VERDICTS: [(&str, Verdict, egui::Color32); 3] =
    [("Good", Verdict::Good, GOOD), ("Neutral", Verdict::Neutral, NEUTRAL), ("Bad", Verdict::Bad, BAD)];

fn keys(ui: &egui::Ui, st: &AppState, out: &mut Vec<Msg>) {
    // the full-size preview is up: its keys are its own (Esc)
    if ui.ctx().egui_wants_keyboard_input() || st.preview.is_some() {
        return;
    }
    // with picks, the rating keys rate them all
    let picking = crate::tiles::keys(ui, st, out);
    if st.review_view == ReviewView::Sheet {
        return sheet_keys(ui, picking, out);
    }
    ui.input(|i| {
        for (keys, v) in [
            ([Key::Num1, Key::G], Verdict::Good),
            ([Key::Num2, Key::N], Verdict::Neutral),
            ([Key::Num3, Key::B], Verdict::Bad),
        ] {
            if !picking && keys.iter().any(|k| i.key_pressed(*k)) {
                out.push(Msg::ReviewCurrent(v));
            }
        }
        if i.key_pressed(Key::ArrowRight) {
            out.push(Msg::ReviewNext(1));
        }
        if i.key_pressed(Key::ArrowLeft) {
            out.push(Msg::ReviewNext(-1));
        }
        if i.key_pressed(Key::Space) {
            out.push(Msg::ToggleReviewGrid);
        }
        if i.key_pressed(Key::C) && !i.modifiers.command {
            out.push(Msg::Compare);
        }
        if i.modifiers.command && i.key_pressed(Key::Z) {
            out.push(Msg::UndoReview);
        }
        if !i.modifiers.command
            && !picking
            && let Some(o) = st.review_queue().get(st.review_cursor)
        {
            for (key, msg) in [
                (Key::E, Msg::UseImage(o.image.clone(), UseTarget::Edit)),
                (Key::N, Msg::UseImage(o.image.clone(), UseTarget::NewProject)),
                (Key::D, Msg::UseImage(o.image.clone(), UseTarget::DuplicateProject)),
                (Key::R, Msg::ToInputLibrary(o.image.clone())),
                (Key::I, Msg::NewIdeaFromImage(o.image.clone())),
            ] {
                if i.key_pressed(key) {
                    out.push(msg);
                }
            }
        }
    });
    let _ = st;
}

/// "fox-cafe · r02 cfg6 · rev 0 · seed 3".
fn describe(st: &AppState, o: &OutputLink) -> String {
    let what = describe_run(st, o);
    match st.image_title(&o.image) {
        Some(t) => format!("“{t}”\n{what}"),
        None => what,
    }
}

fn describe_run(st: &AppState, o: &OutputLink) -> String {
    match st.index.run(&o.name.run) {
        Some((p, run)) => {
            let what = format!("{} · {} · rev {} · seed {}", p.name(), run.label, o.name.rev, o.name.seed);
            match run.timings.iter().find(|t| t.seed == o.name.seed) {
                Some(t) => format!("{what}\n{}", increart_gui_core::timing::image_text(t, run.sample.steps)),
                None => what,
            }
        }
        None => format!("seed {}", o.name.seed),
    }
}

pub fn view(ui: &mut egui::Ui, st: &AppState, tex: &mut Textures, out: &mut Vec<Msg>) {
    let Some(root) = st.library.clone() else { return };
    keys(ui, st, out);
    let queue = st.review_queue();
    Panel::left("review_scope").default_size(230.0).show(ui, |ui| scope_panel(ui, st, out));
    Panel::top("review_tabs").show(ui, |ui| {
        ui.horizontal(|ui| {
            for (v, label) in [
                (ReviewView::Single, ic(ph::SQUARE, "Single")),
                (ReviewView::Grid, ic(ph::SQUARES_FOUR, "Grid")),
                (ReviewView::Compare, ic(ph::COLUMNS, "A/B compare")),
                (ReviewView::Sheet, ic(ph::TABLE, "Contact sheet")),
            ] {
                if v == ReviewView::Sheet && st.review_sweep.is_none() {
                    continue;
                }
                if ui.selectable_label(st.review_view == v, label).clicked() && st.review_view != v {
                    out.push(Msg::ReviewView(v));
                }
            }
            ui.separator();
            let generating = st.progress.len();
            ui.weak(format!("{} unreviewed · oldest first", queue.len()));
            if generating > 0 {
                ui.weak(format!("· {generating} generating, new images join the end"));
            }
            if ui.small_button("newest ›").on_hover_text("jump to the newest unreviewed image").clicked() {
                out.push(Msg::ReviewNewest);
            }
            // the current image is one style of a sweep: its contact sheet
            let sweep = queue
                .get(st.review_cursor)
                .filter(|_| st.review_view != ReviewView::Sheet)
                .and_then(|o| st.index.run(&o.name.run))
                .and_then(|(p, r)| Some((p.project.id.clone(), r.sweep.as_ref()?)));
            if let Some((project, tag)) = sweep {
                ui.separator();
                let label = RichText::new(ic(ph::TABLE, &format!("Sweep {} contact sheet", tag.name)))
                    .color(crate::widgets::ACCENT);
                if ui
                    .button(label)
                    .on_hover_text(format!(
                        "this image is the {} row of sweep {}: see every row side by side",
                        tag.row(),
                        tag.name
                    ))
                    .clicked()
                {
                    out.push(Msg::ReviewSweep(project, tag.name.clone()));
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mut on = st.checkerboard;
                if ui.checkbox(&mut on, "checkerboard behind alpha").changed() {
                    out.push(Msg::Checkerboard(on));
                }
            });
        });
    });
    crate::tiles::bar(ui, st, out);
    if queue.is_empty() && !matches!(st.review_view, ReviewView::Compare | ReviewView::Sheet) {
        egui::CentralPanel::default().show(ui, |ui| {
            let more = if st.active_runs.is_empty() { "" } else { " Images are still being generated." };
            ui.weak(format!("Nothing to review in this scope.{more}"));
        });
        return;
    }
    match st.review_view {
        ReviewView::Single => single(ui, st, &root, tex, out),
        ReviewView::Grid => grid(ui, st, &root, tex, out),
        ReviewView::Compare => compare(ui, st, &root, tex, out),
        ReviewView::Sheet => sheet(ui, st, &root, tex, out),
    }
}

fn scope_panel(ui: &mut egui::Ui, st: &AppState, out: &mut Vec<Msg>) {
    let sheet = st.review_view == ReviewView::Sheet;
    Panel::bottom("review_keys").show(ui, |ui| key_hints(ui, sheet));
    // right to left: the clear button, then the field takes what's left
    let row = egui::vec2(ui.available_width(), ui.spacing().interact_size.y);
    ui.allocate_ui_with_layout(row, egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let mut q = st.review_search.clone();
        if !q.is_empty() && ui.small_button(ph::X).on_hover_text("clear").clicked() {
            out.push(Msg::ReviewSearch(String::new()));
        }
        let field = egui::TextEdit::singleline(&mut q)
            .hint_text(ic(ph::MAGNIFYING_GLASS, "Search projects"))
            .desired_width(ui.available_width());
        let r = ui.add(field);
        if r.changed() {
            out.push(Msg::ReviewSearch(q.clone()));
        }
        if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Escape)) {
            out.push(Msg::ReviewSearch(String::new()));
        }
    });
    ui.label(RichText::new("Scope").strong());
    ui.weak("independent of the Projects tab");
    let mut all_runs = st.review_all_runs;
    let tip = "list every project's runs (else only the selected project's)";
    if ui.checkbox(&mut all_runs, "runs of all projects").on_hover_text(tip).changed() {
        out.push(Msg::ReviewAllRuns(all_runs));
    }
    let all = st.review_scope.is_none() && st.review_folder.is_none();
    if ui.selectable_label(all, format!("All projects ({})", st.unreviewed_outputs())).clicked() {
        out.push(Msg::ReviewScope(None));
    }
    // Unreviewed images under a folder (relative to `projects/`).
    let under = |f: &Path| -> usize {
        let base = Path::new("projects").join(f);
        st.index.projects.iter().filter(|p| p.dir.starts_with(&base)).map(|p| counts(p)[3]).sum()
    };
    /// Whether the folder's name or anything under it matches the search
    /// (lowercase).
    fn folder_matches(st: &AppState, f: &Path, q: &str) -> bool {
        let hit = |name: &str| name.to_lowercase().contains(q);
        let dir = Path::new("projects").join(f);
        hit(&f.file_name().unwrap_or_default().to_string_lossy())
            || st.index.projects.iter().any(|p| p.dir.starts_with(&dir) && hit(&p.name()))
            || st
                .index
                .project_folders
                .iter()
                .any(|g| g != f && g.starts_with(f) && hit(&g.file_name().unwrap_or_default().to_string_lossy()))
    }
    /// `q`: the search (lowercase); a matching folder shows all it holds.
    fn rec(
        ui: &mut egui::Ui,
        st: &AppState,
        parent: &Path,
        q: &str,
        out: &mut Vec<Msg>,
        under: &dyn Fn(&Path) -> usize,
    ) {
        let depth = parent.components().count() as f32 + 1.0;
        let mut folders: Vec<_> = st.index.project_folders.iter().filter(|f| f.parent() == Some(parent)).collect();
        folders.sort();
        for f in folders {
            if !q.is_empty() && !folder_matches(st, f, q) {
                continue;
            }
            let own = f.file_name().unwrap_or_default().to_string_lossy().to_lowercase().contains(q);
            let n = under(f);
            ui.horizontal(|ui| {
                ui.add_space(12.0 * depth);
                let name = f.file_name().unwrap_or_default().to_string_lossy();
                let sel = st.review_folder.as_deref() == Some(f.as_path());
                if ui.selectable_label(sel, ic(ph::FOLDER, &format!("{name} ({n})"))).clicked() {
                    out.push(Msg::ReviewScopeFolder(f.clone()));
                }
            });
            rec(ui, st, f, if own { "" } else { q }, out, under);
        }
        let dir = Path::new("projects").join(parent);
        let mut ps: Vec<_> = st
            .index
            .projects
            .iter()
            .filter(|p| p.dir.parent() == Some(dir.as_path()))
            .filter(|p| q.is_empty() || p.name().to_lowercase().contains(q))
            .collect();
        ps.sort_by_key(|p| p.name());
        for p in ps {
            let u = counts(p)[3];
            ui.horizontal(|ui| {
                ui.add_space(12.0 * depth);
                // the whole project; a run or sweep of it is selected below
                let sel = st.review_scope.as_ref() == Some(&p.project.id)
                    && st.review_sweep.is_none()
                    && st.review_run.is_none();
                let label = ic(ph::ROCKET_LAUNCH, &format!("{} ({u})", p.name()));
                let text = if u == 0 { RichText::new(label).weak() } else { RichText::new(label) };
                if ui.selectable_label(sel, text).clicked() {
                    out.push(Msg::ReviewScope(Some(p.project.id.clone())));
                }
            });
            let selected = st.review_scope.as_ref() == Some(&p.project.id);
            if !selected && !st.review_all_runs {
                continue;
            }
            for row in st.review_runs(p) {
                let (sel, label, tip, u, msg) = match row {
                    ReviewRun::Single(r) => {
                        let label = if r.label.is_empty() { r.id.to_string() } else { r.label.clone() };
                        let sel = selected && st.review_run.as_ref() == Some(&r.id);
                        let u = st.run_unreviewed(p, &r.id);
                        let msg = Msg::ReviewRun(p.project.id.clone(), r.id.clone());
                        (sel, ic(ph::IMAGES, &format!("{label} ({u})")), "this run's images", u, msg)
                    }
                    ReviewRun::Sweep(name) => {
                        let sel = selected && st.review_sweep.as_deref() == Some(name);
                        let u = st.sweep_unreviewed(p, name);
                        let msg = Msg::ReviewSweep(p.project.id.clone(), name.to_string());
                        (sel, ic(ph::TABLE, &format!("sweep {name} ({u})")), "this style sweep's contact sheet", u, msg)
                    }
                };
                ui.horizontal(|ui| {
                    ui.add_space(12.0 * depth + 18.0);
                    let text = if u == 0 { RichText::new(label).weak() } else { RichText::new(label) };
                    if ui.selectable_label(sel, text).on_hover_text(tip).clicked() {
                        out.push(msg);
                    }
                });
            }
        }
    }
    let q = st.review_search.trim().to_lowercase();
    ScrollArea::vertical().auto_shrink([false, true]).id_salt("review_scope_tree").show(ui, |ui| {
        if !q.is_empty() && !folder_matches(st, Path::new(""), &q) {
            ui.weak("No project or folder matches.");
        }
        rec(ui, st, Path::new(""), &q, out, &under);
    });
}

/// Review's keys, folded away at the bottom of the scope panel; the contact
/// sheet has fewer.
fn key_hints(ui: &mut egui::Ui, sheet: bool) {
    const SHEET: &[(&str, &str)] =
        &[("1 / G", "good"), ("2 / N", "neutral"), ("3 / B", "bad"), ("← → ↑ ↓", "move"), ("Ctrl+Z", "undo")];
    egui::CollapsingHeader::new(RichText::new(ic(ph::KEYBOARD, "Keys")).strong())
        .id_salt("review_keys")
        .default_open(true)
        .show(ui, |ui| {
            let all: &[(&str, &str)] = &[
                ("1 / G", "good"),
                ("2 / N", "neutral"),
                ("3 / B", "bad"),
                ("← →", "previous / next"),
                ("Space", "grid / single"),
                ("C", "A/B compare"),
                ("E", "edit this image"),
                ("N", "new project from it"),
                ("D", "duplicate its project"),
                ("R", "→ input library"),
                ("I", "→ new idea"),
                ("Ctrl+Z", "undo"),
            ];
            for &(k, what) in if sheet { SHEET } else { all } {
                ui.horizontal(|ui| {
                    ui.monospace(k);
                    ui.weak(what);
                });
            }
        });
}

fn verdict_buttons(ui: &mut egui::Ui, out: &mut Vec<Msg>, msg: impl Fn(Verdict) -> Msg) {
    ui.horizontal(|ui| {
        for (label, v, c) in VERDICTS {
            if ui.button(RichText::new(label).color(c)).clicked() {
                out.push(msg(v));
            }
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn info_panel(
    ui: &mut egui::Ui,
    st: &AppState,
    root: &Path,
    tex: &mut Textures,
    item: &OutputLink,
    pos: usize,
    n: usize,
    out: &mut Vec<Msg>,
) {
    ui.label(RichText::new(format!("{} / {n}", pos + 1)).strong());
    ui.label(describe(st, item));
    recipe_grid(ui, st, root, tex, &item.image, out);
}

/// The recipe of an output (loaded on demand), as a two-column grid.
pub fn recipe_grid(
    ui: &mut egui::Ui,
    st: &AppState,
    root: &Path,
    tex: &mut Textures,
    image: &increart_core::ImageRef,
    out: &mut Vec<Msg>,
) {
    let Some(loaded) = st.recipes.get(&image.id) else {
        out.push(Msg::NeedRecipe(image.id.clone()));
        ui.spinner();
        return;
    };
    let Some(r) = loaded else {
        ui.weak("no recipe in this image");
        return;
    };
    let project = st.output_link(image).and_then(|o| st.project_of_link(o)).map(|p| p.name()).unwrap_or_default();
    let run =
        st.output_link(image).and_then(|o| st.index.run(&o.name.run)).map(|(_, r)| r.label.clone()).unwrap_or_default();
    let mode = match &r.mode {
        increart_core::model::Mode::Generate => "generate".to_string(),
        increart_core::model::Mode::Edit { strength, init } => {
            let from = init.as_ref().map(|i| tex.dims_text(root, i)).filter(|d| !d.is_empty());
            let keep = ((1.0 - strength) * 100.0).round();
            let from = from.map(|d| format!(", from {d}")).unwrap_or_default();
            format!("img2img, kept {keep:.0} % (strength {strength}){from}")
        }
    };
    egui::Grid::new(ui.scope_id().with("recipe")).num_columns(2).spacing([10.0, 2.0]).show(ui, |ui| {
        let rows: Vec<(&str, String)> = vec![
            ("project", project),
            ("run", run),
            ("seed", r.seed.to_string()),
            ("recipe", r.model.recipe.clone()),
            ("size", crate::widgets::size_info(r.width, r.height)),
            ("steps / cfg", format!("{} / {}", r.sample.steps, r.sample.cfg)),
            ("mode", mode),
            ("alpha", r.alpha.to_string()),
            ("refs", {
                let sizes: Vec<String> = r.refs.iter().map(|i| tex.dims_text(root, i)).collect();
                if sizes.is_empty() { "0".into() } else { format!("{}: {}", sizes.len(), sizes.join(", ")) }
            }),
            ("parent", r.parent.as_ref().map(|p| p.id.to_string()).unwrap_or_else(|| "—".into())),
            ("GPU", r.device.clone()),
            ("elapsed", format!("{:.1} s", r.elapsed_s)),
            ("sd.cpp", r.sdcpp.clone()),
        ];
        for (k, v) in rows {
            ui.weak(k);
            ui.label(v);
            ui.end_row();
        }
    });
    if !r.inputs.is_empty() {
        ui.weak("inputs");
        egui::Grid::new(ui.scope_id().with("inputs")).num_columns(2).spacing([10.0, 2.0]).show(ui, |ui| {
            for i in &r.inputs {
                ui.weak(match i.role {
                    increart_core::model::InputRole::Ref(n) => format!("<image{n}>"),
                    increart_core::model::InputRole::Start => "start".into(),
                });
                let name = i.original_name.clone().unwrap_or_else(|| i.file.clone());
                ui.label(RichText::new(short(&name, 34)).small())
                    .on_hover_text(format!("{}\nBLAKE3 {}", i.file, i.blake3));
                ui.end_row();
            }
        });
    }
    ui.weak("prompt_sent");
    ui.add(egui::Label::new(RichText::new(&r.prompt_sent).small()).wrap());
    if !r.negative_prompt.is_empty() {
        ui.weak("negative");
        ui.add(egui::Label::new(RichText::new(&r.negative_prompt).small()).wrap());
    }
}

fn single(ui: &mut egui::Ui, st: &AppState, root: &Path, tex: &mut Textures, out: &mut Vec<Msg>) {
    let queue = st.review_queue();
    let cur = st.review_cursor.min(queue.len() - 1);
    let item = queue[cur];
    Panel::bottom("review_strip").default_size(110.0).show(ui, |ui| {
        let bg = crate::tiles::background(ui, st, out);
        let mut rects = vec![];
        ScrollArea::horizontal().auto_shrink([false, true]).show(ui, |ui| {
            ui.horizontal(|ui| {
                for (i, o) in queue.iter().enumerate().take(cur + 40).skip(cur.saturating_sub(10)) {
                    let t = tex.thumb(root, &o.image);
                    let r = thumb(ui, t.as_ref(), 90.0, crate::tiles::cursor(st, i == cur), None);
                    crate::tiles::mark(ui, st, r.rect, &o.image);
                    rects.push((o.image.clone(), r.rect));
                    if !crate::tiles::click(ui, st, &r, &o.image, out) && r.clicked() {
                        out.push(Msg::ReviewCursor(i));
                    }
                    r.context_menu(|ui| {
                        crate::tiles::menu(ui, st, &o.image, out);
                        use_menu(ui, root, &o.image, out);
                    });
                }
            });
        });
        crate::tiles::band(ui, st, &bg, &rects, out);
    });
    Panel::right("review_info").default_size(300.0).show(ui, |ui| {
        // the buttons stay at the bottom, wherever the info ends
        Panel::bottom("review_actions").show(ui, |ui| {
            ui.add_space(4.0);
            verdict_buttons(ui, out, Msg::ReviewCurrent);
            ui.horizontal(|ui| {
                if ui.button(ph::CARET_LEFT).clicked() {
                    out.push(Msg::ReviewNext(-1));
                }
                if ui.button(ph::CARET_RIGHT).clicked() {
                    out.push(Msg::ReviewNext(1));
                }
                if ui.button(ic(ph::PUSH_PIN, "Pin & compare")).clicked() {
                    out.push(Msg::Compare);
                }
                if ui
                    .add_enabled(!st.review_undo.is_empty(), egui::Button::new(ic(ph::ARROW_COUNTER_CLOCKWISE, "Undo")))
                    .clicked()
                {
                    out.push(Msg::UndoReview);
                }
            });
            ui.add_space(2.0);
        });
        ScrollArea::vertical().auto_shrink([false, false]).id_salt("review_info_scroll").show(ui, |ui| {
            info_panel(ui, st, root, tex, item, cur, queue.len(), out);
        });
    });
    egui::CentralPanel::default().show(ui, |ui| {
        let t = tex.full(root, &item.image);
        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click());
        image_fit_bg(&ui.painter_at(rect), rect.shrink(4.0), t.as_ref(), st.checkerboard);
        if resp.double_clicked() {
            out.push(Msg::Preview(Some(item.image.clone())));
        }
        resp.context_menu(|ui| {
            crate::tiles::menu(ui, st, &item.image, out);
            use_menu(ui, root, &item.image, out);
            ui.separator();
            collection_menu(ui, st, std::slice::from_ref(&item.image), out);
            if ui.button(ic(ph::FOLDER_OPEN, "Show in file manager")).clicked() {
                out.push(Msg::ShowInFileManager(item.path.clone()));
                ui.close();
            }
        });
    });
}

/// "Use this image" actions: into the current, its own, a new or a
/// duplicated project, or to the input side (DESIGN.md, Outputs as inputs).
pub fn use_menu(ui: &mut egui::Ui, root: &Path, image: &increart_core::ImageRef, out: &mut Vec<Msg>) {
    copy_image_item(ui, root, image, out);
    ui.separator();
    for (label, target) in [
        (ic(ph::PLUS, "Use in current project"), UseTarget::CurrentProject),
        (ic(ph::PENCIL_SIMPLE, "Edit this image   E"), UseTarget::Edit),
        (ic(ph::ROCKET_LAUNCH, "New project from this image   N"), UseTarget::NewProject),
        (ic(ph::COPY, "Duplicate project with this image   D"), UseTarget::DuplicateProject),
    ] {
        if ui.button(label).clicked() {
            out.push(Msg::UseImage(image.clone(), target));
            ui.close();
        }
    }
    ui.separator();
    if ui.button(ic(ph::BOOKS, "To the input library as a ref   R")).clicked() {
        out.push(Msg::ToInputLibrary(image.clone()));
        ui.close();
    }
    if ui.button(ic(ph::LIGHTBULB, "New idea from this image   I")).clicked() {
        out.push(Msg::NewIdeaFromImage(image.clone()));
        ui.close();
    }
}

fn grid(ui: &mut egui::Ui, st: &AppState, root: &Path, tex: &mut Textures, out: &mut Vec<Msg>) {
    let queue = st.review_queue();
    egui::CentralPanel::default().show(ui, |ui| {
        let bg = crate::tiles::background(ui, st, out);
        let mut rects = vec![];
        ui.weak("Click to select, double-click to open; keys rate the selected image (or the picked ones).");
        ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::splat(6.0);
                for (i, o) in queue.iter().enumerate() {
                    let t = tex.thumb(root, &o.image);
                    let r = thumb(ui, t.as_ref(), 200.0, crate::tiles::cursor(st, i == st.review_cursor), None)
                        .on_hover_text(describe(st, o));
                    crate::tiles::mark(ui, st, r.rect, &o.image);
                    rects.push((o.image.clone(), r.rect));
                    if !crate::tiles::click(ui, st, &r, &o.image, out) && r.clicked() {
                        out.push(Msg::ReviewCursor(i));
                    }
                    if r.double_clicked() {
                        out.push(Msg::ReviewCursor(i));
                        out.push(Msg::ReviewView(ReviewView::Single));
                    }
                    r.context_menu(|ui| {
                        crate::tiles::menu(ui, st, &o.image, out);
                        for (label, v, c) in VERDICTS {
                            if ui.button(RichText::new(label).color(c)).clicked() {
                                out.push(Msg::ReviewLink(o.path.clone(), Some(v)));
                                ui.close();
                            }
                        }
                        ui.separator();
                        use_menu(ui, root, &o.image, out);
                    });
                }
            });
        });
        crate::tiles::band(ui, st, &bg, &rects, out);
    });
}

fn sheet_keys(ui: &egui::Ui, picking: bool, out: &mut Vec<Msg>) {
    ui.input(|i| {
        for (keys, v) in [
            ([Key::Num1, Key::G], Verdict::Good),
            ([Key::Num2, Key::N], Verdict::Neutral),
            ([Key::Num3, Key::B], Verdict::Bad),
        ] {
            if !picking && keys.iter().any(|k| i.key_pressed(*k)) {
                out.push(Msg::SheetRate(v));
            }
        }
        for (k, dr, dc) in
            [(Key::ArrowRight, 0, 1), (Key::ArrowLeft, 0, -1), (Key::ArrowDown, 1, 0), (Key::ArrowUp, -1, 0)]
        {
            if i.key_pressed(k) {
                out.push(Msg::SheetMove(dr, dc));
            }
        }
        if i.modifiers.command && i.key_pressed(Key::Z) {
            out.push(Msg::UndoReview);
        }
    });
}

/// A style sweep as a contact sheet: a row per style, a column per seed.
fn sheet(ui: &mut egui::Ui, st: &AppState, root: &Path, tex: &mut Textures, out: &mut Vec<Msg>) {
    use increart_gui_core::SheetCell;
    let Some(sh) = st.sheet() else { return };
    let size = st.sheet_cell_size() as f32;
    egui::CentralPanel::default().show(ui, |ui| {
        let bg = crate::tiles::background(ui, st, out);
        let mut rects = vec![];
        ui.horizontal(|ui| {
            let mut f = st.sheet_filter.clone();
            let r = ui.add(
                egui::TextEdit::singleline(&mut f)
                    .hint_text(format!("{}  filter styles by name", ph::MAGNIFYING_GLASS))
                    .desired_width(220.0),
            );
            if r.has_focus() && ui.input(|i| i.key_pressed(Key::Escape)) {
                f.clear();
            }
            if !st.sheet_filter.is_empty() {
                if ui.small_button(ph::X).on_hover_text("clear the filter").clicked() {
                    f.clear();
                }
                let shown = sh.rows.len();
                let all = shown + sh.hidden;
                ui.weak(format!("{shown} of {all} styles"));
            }
            if f != st.sheet_filter {
                out.push(Msg::SheetFilter(f));
            }
            ui.separator();
            zoom(ui, st, out);
            ui.separator();
            ui.weak(ph::INFO).on_hover_text(
                "Click selects, double-click opens; keys rate, arrows move. Row buttons rate unreviewed images only. \
                 Ctrl+click, Shift+click or drag a box to pick images: to rate them together, or Tile… them.",
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let kept = sh
                    .rows
                    .iter()
                    .filter(|r| {
                        r.cells.iter().any(|c| matches!(c, SheetCell::Image(o) if o.verdict == Some(Verdict::Good)))
                    })
                    .count();
                let n = sh.seeds.len().max(1) as u64;
                let styles = if kept == 1 { "style" } else { "styles" };
                let label = ic(ph::PLUS, &format!("{n} more seeds for {kept} kept {styles}"));
                if ui
                    .add_enabled(kept > 0, egui::Button::new(label))
                    .on_hover_text("styles with a Good image get more seeds")
                    .clicked()
                {
                    out.push(Msg::SheetMoreSeeds(n));
                }
            });
        });
        if sh.rows.is_empty() && sh.hidden > 0 {
            ui.add_space(12.0);
            ui.weak(format!("No style's name has “{}”.", st.sheet_filter.trim()));
            return;
        }
        // Ctrl+scroll over the sheet zooms it
        let over = ui.rect_contains_pointer(ui.available_rect_before_wrap());
        let zoom = ui.input(|i| i.zoom_delta());
        if over && zoom != 1.0 {
            out.push(Msg::SheetZoom { size: (size * zoom).round() as u32, save: true });
        }
        ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("contact_sheet").spacing([6.0, 6.0]).show(ui, |ui| {
                ui.label("");
                for s in &sh.seeds {
                    ui.weak(format!("seed {s}"));
                }
                ui.end_row();
                for (r, row) in sh.rows.iter().enumerate() {
                    ui.vertical(|ui| {
                        ui.set_width(170.0);
                        let style = row.run.sweep.as_ref().map_or(row.run.label.clone(), |s| s.row());
                        ui.label(RichText::new(style).strong()).on_hover_text(&row.run.label);
                        let param = row.run.sweep.as_ref().and_then(|s| s.param.as_ref());
                        if let Some(p) =
                            param.filter(|p| p.name == "cfg" && p.value.parse().is_ok_and(|c: f32| c <= 1.0))
                        {
                            ui.colored_label(crate::widgets::NEUTRAL, "no negative prompt")
                                .on_hover_text(format!("at {} the negative prompt isn't used", p.label()));
                        }
                        let [mut g, mut b, mut u] = [0, 0, 0];
                        for c in &row.cells {
                            if let SheetCell::Image(o) = c {
                                match o.verdict {
                                    Some(Verdict::Good) => g += 1,
                                    Some(Verdict::Bad) => b += 1,
                                    None => u += 1,
                                    _ => {}
                                }
                            }
                        }
                        ui.weak(format!("{g} good · {b} bad · {u} new"));
                        ui.horizontal(|ui| {
                            for (label, v, c) in VERDICTS {
                                let b = egui::Button::new(RichText::new(&label[..1]).color(c));
                                if ui
                                    .add_enabled(u > 0, b)
                                    .on_hover_text(format!("{label} for this row's unreviewed images"))
                                    .clicked()
                                {
                                    out.push(Msg::SheetRow(r, v));
                                }
                            }
                            if ui
                                .small_button(ph::ARROW_SQUARE_OUT)
                                .on_hover_text(if param.is_some() {
                                    "Use these settings: open its run in Generate"
                                } else {
                                    "Use this style: open its run in Generate"
                                })
                                .clicked()
                            {
                                out.push(Msg::RunInGenerate(row.run.id.clone()));
                            }
                        });
                    });
                    for (c, cell) in row.cells.iter().enumerate() {
                        let sel = crate::tiles::cursor(st, st.sheet_cell == (r, c));
                        match cell {
                            SheetCell::Image(o) => {
                                let t = tex.thumb(root, &o.image);
                                let resp = thumb(ui, t.as_ref(), size, sel, o.verdict.map(|v| verdict_color(Some(v))))
                                    .on_hover_text(describe(st, o));
                                crate::tiles::mark(ui, st, resp.rect, &o.image);
                                rects.push((o.image.clone(), resp.rect));
                                if !crate::tiles::click(ui, st, &resp, &o.image, out) && resp.clicked() {
                                    out.push(Msg::SheetCell(r, c));
                                }
                                if resp.double_clicked() {
                                    out.push(Msg::Preview(Some(o.image.clone())));
                                }
                                resp.context_menu(|ui| {
                                    crate::tiles::menu(ui, st, &o.image, out);
                                    for (label, v, col) in VERDICTS {
                                        if ui.button(RichText::new(label).color(col)).clicked() {
                                            out.push(Msg::ReviewLink(o.path.clone(), Some(v)));
                                            ui.close();
                                        }
                                    }
                                    ui.separator();
                                    use_menu(ui, root, &o.image, out);
                                });
                            }
                            SheetCell::Pending | SheetCell::Empty => {
                                let (text, tip) = match cell {
                                    SheetCell::Pending => ("…", "still to be generated"),
                                    _ => ("–", "not generated (cancelled or failed)"),
                                };
                                let resp = empty_cell(ui, size, sel, text).on_hover_text(tip);
                                if resp.clicked() {
                                    out.push(Msg::SheetCell(r, c));
                                }
                            }
                        }
                    }
                    ui.end_row();
                }
            });
        });
        crate::tiles::band(ui, st, &bg, &rects, out);
    });
}

/// The contact sheet's cell size: a slider; also Ctrl+scroll over the sheet.
fn zoom(ui: &mut egui::Ui, st: &AppState, out: &mut Vec<Msg>) {
    use increart_gui_core::{SHEET_CELL_DEFAULT, SHEET_CELL_MAX, SHEET_CELL_MIN};
    let now = st.sheet_cell_size();
    let step = |ui: &mut egui::Ui, icon: &str, tip: &str, to: u32| {
        ui.add_enabled(to != now, egui::Button::new(icon).small()).on_hover_text(tip).clicked().then_some(to)
    };
    let mut to = step(ui, ph::MAGNIFYING_GLASS_MINUS, "smaller", now.saturating_sub(32).max(SHEET_CELL_MIN));
    let mut size = now;
    let r = ui
        .add(egui::Slider::new(&mut size, SHEET_CELL_MIN..=SHEET_CELL_MAX).show_value(false))
        .on_hover_text(format!("cell size {now} (Ctrl+scroll over the sheet; double-click: {SHEET_CELL_DEFAULT})"));
    if r.double_clicked() {
        size = SHEET_CELL_DEFAULT;
    }
    if size != now || r.drag_stopped() {
        out.push(Msg::SheetZoom { size, save: !r.dragged() });
    }
    to = to.or(step(ui, ph::MAGNIFYING_GLASS_PLUS, "larger", (now + 32).min(SHEET_CELL_MAX)));
    if let Some(size) = to {
        out.push(Msg::SheetZoom { size, save: true });
    }
}

/// A contact sheet cell without an image: `…` pending, `–` none.
fn empty_cell(ui: &mut egui::Ui, size: f32, selected: bool, text: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 2.0, egui::Color32::from_gray(35));
    p.text(rect.center(), egui::Align2::CENTER_CENTER, text, egui::FontId::proportional(24.0), egui::Color32::GRAY);
    let stroke = if selected {
        egui::Stroke::new(3.0, crate::widgets::ACCENT)
    } else if resp.hovered() {
        egui::Stroke::new(1.0, egui::Color32::GRAY)
    } else {
        egui::Stroke::NONE
    };
    p.rect_stroke(rect, 2.0, stroke, egui::StrokeKind::Inside);
    resp
}

fn compare(ui: &mut egui::Ui, st: &AppState, root: &Path, tex: &mut Textures, out: &mut Vec<Msg>) {
    let queue = st.review_queue();
    let a = st.review_pinned.as_ref().and_then(|p| st.output_link(p));
    let b = queue.get(st.review_cursor.min(queue.len().saturating_sub(1))).copied();
    Panel::bottom("compare_bar").show(ui, |ui| {
        ui.horizontal(|ui| {
            if ui.button(ic(ph::ARROWS_LEFT_RIGHT, "Swap A / B")).clicked() {
                out.push(Msg::SwapCompare);
            }
            if ui.button(ph::CARET_LEFT).on_hover_text("previous B").clicked() {
                out.push(Msg::ReviewNext(-1));
            }
            if ui.button(ph::CARET_RIGHT).on_hover_text("next B").clicked() {
                out.push(Msg::ReviewNext(1));
            }
            if ui.button(ic(ph::X, "Leave compare")).clicked() {
                out.push(Msg::Compare);
            }
            ui.weak("Keys rate B; A has its own buttons.");
        });
    });
    egui::CentralPanel::default().show(ui, |ui| {
        ui.columns(2, |cols| {
            for (ui, (item, is_a)) in cols.iter_mut().zip([(a, true), (b, false)]) {
                let title = if is_a { "A (pinned)" } else { "B" };
                ui.horizontal(|ui| {
                    ui.label(RichText::new(title).strong());
                    if let Some(o) = item {
                        ui.weak(describe(st, o));
                        if is_a && o.verdict.is_some() {
                            ui.colored_label(verdict_color(o.verdict), "rated");
                        }
                    }
                });
                match item {
                    Some(o) => {
                        let msg: fn(Verdict) -> Msg = if is_a { Msg::ReviewPinned } else { Msg::ReviewCurrent };
                        // bottom up: the buttons at the bottom of the side, the image above
                        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                            verdict_buttons(ui, out, msg);
                            let t = tex.full(root, &o.image);
                            let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click());
                            image_fit_bg(&ui.painter_at(rect), rect.shrink(4.0), t.as_ref(), st.checkerboard);
                            resp.context_menu(|ui| copy_image_item(ui, root, &o.image, out));
                        });
                    }
                    None => {
                        ui.weak(if is_a { "Nothing pinned." } else { "No more unreviewed images." });
                    }
                }
            }
        });
    });
}

// ---------------------------------------------------------------- output library

pub fn output_library(ui: &mut egui::Ui, st: &AppState, tex: &mut Textures, out: &mut Vec<Msg>) {
    let Some(root) = st.library.clone() else { return };
    Panel::left("out_sources").default_size(250.0).show(ui, |ui| {
        ScrollArea::vertical()
            .auto_shrink([false, true])
            .id_salt("out_sources_scroll")
            .show(ui, |ui| sources_panel(ui, st, out));
        if st.output_reveal {
            reveal_done(ui, out);
        }
    });
    let images = st.output_source_images();
    // F2: the selected image's title
    if st.output_tab == OutputTab::Grid
        && st.preview.is_none()
        && st.title_edit.is_none()
        && !ui.ctx().egui_wants_keyboard_input()
        && ui.input(|i| i.key_pressed(Key::F2))
        && let Some(img) = &st.output_selected
    {
        out.push(Msg::TitleDialog(Some(img.clone())));
    }
    Panel::top("out_tabs").show(ui, |ui| {
        ui.horizontal(|ui| {
            if ui.selectable_label(st.output_tab == OutputTab::Grid, ic(ph::SQUARES_FOUR, "Grid")).clicked() {
                out.push(Msg::OutputTab(OutputTab::Grid));
            }
            if ui.selectable_label(st.output_tab == OutputTab::Folder, ic(ph::FOLDER, "Folder")).clicked() {
                out.push(Msg::OutputTab(OutputTab::Folder));
            }
            ui.separator();
            let shown = &images;
            if ui
                .add_enabled(
                    !shown.is_empty(),
                    egui::Button::new(ic(ph::EXPORT, &format!("Export these {}…", shown.len()))),
                )
                .clicked()
            {
                out.push(Msg::OpenExport(shown.iter().map(|o| o.image.clone()).collect(), source_name(st)));
            }
            if st.output_tab == OutputTab::Grid {
                ui.weak(format!("· {}", source_name(st)));
            }
            if let OutputSource::Collection(f) = &st.output_source
                && ui.button(ic(ph::FOLDER_OPEN, "Open in file manager")).clicked()
            {
                out.push(Msg::ShowInFileManager(Path::new("collections").join(f)));
            }
            crate::widgets::details_toggle(ui, st, InfoPanel::Output, out);
        });
    });
    if st.output_tab == OutputTab::Grid {
        Panel::top("out_presets").show(ui, |ui| preset_tabs(ui, st, out));
        crate::tiles::bar(ui, st, out);
        crate::tiles::keys(ui, st, out);
    }
    if !st.info_hidden.contains(&InfoPanel::Output) {
        Panel::right("out_lineage").default_size(300.0).show(ui, |ui| {
            ScrollArea::vertical().auto_shrink([false, true]).id_salt("lineage_scroll").show(ui, |ui| {
                match st.output_selected.clone() {
                    Some(sel) => lineage_panel(ui, st, &root, tex, &sel, out),
                    None => crate::widgets::nothing_selected(
                        ui,
                        "Lineage",
                        "No image selected: click one to see where it came from, its recipe and what was made from it.",
                    ),
                }
            });
        });
    }
    egui::CentralPanel::default().show(ui, |ui| match st.output_tab {
        OutputTab::Folder => crate::folderview::view(ui, st, tex, out),
        OutputTab::Grid => {
            let noticed = st.output_notice && output_notice(ui, st, images.len(), out);
            let r = st.output_filter.ratings;
            let rejected = r.bad && !r.good && !r.neutral && !r.unreviewed;
            if rejected && !images.is_empty() {
                ui.horizontal(|ui| {
                    ui.weak("Images rated bad. Right-click to re-rate or delete.");
                    let n = images.len();
                    if ui.button(ic(ph::TRASH, &format!("Delete these {n} rejected…"))).clicked() {
                        let text = format!(
                            "Delete these {n} rejected images ({})? Their links go to the trash; the images stay in \
                             the store until Clean up.",
                            source_name(st)
                        );
                        out.push(Msg::Ask(text, Box::new(Msg::DeleteShownRejected)));
                    }
                });
            }
            if images.is_empty() {
                if !noticed {
                    ui.weak(match st.output_preset {
                        OutputPreset::Rejected if !st.output_preset_modified() => "No rejected images here.",
                        OutputPreset::Unreviewed if !st.output_preset_modified() => "Nothing waiting for review here.",
                        _ => "Nothing here: rate images in Review, pick another tab, or loosen the filters.",
                    });
                }
                return;
            }
            image_grid(ui, st, &root, tex, out, &images, rejected);
        }
    });
}

/// Kept · Good · Rejected · Unreviewed · All, with the current source's
/// counts. A click replaces the filters; changed filters mark the tab "•".
fn preset_tabs(ui: &mut egui::Ui, st: &AppState, out: &mut Vec<Msg>) {
    ui.horizontal(|ui| {
        for p in OutputPreset::EVERY {
            let n = st.source_images_in(&st.output_source, p.ratings()).len();
            let on = st.output_preset == p;
            let dot = if on && st.output_preset_modified() { " •" } else { "" };
            let tip = if dot.is_empty() { "Show these" } else { "Filters changed: click to reset them" };
            if ui.selectable_label(on, format!("{} ({n}){dot}", p.label())).on_hover_text(tip).clicked() {
                out.push(Msg::OutputPreset(p));
            }
        }
        if st.output_preset == OutputPreset::Unreviewed && st.source_unreviewed(&st.output_source) > 0 {
            ui.separator();
            review_them(ui, st, out);
        }
    });
}

/// Jumps to Review for the source's unreviewed images.
fn review_them(ui: &mut egui::Ui, st: &AppState, out: &mut Vec<Msg>) {
    if ui.button(ic(ph::EYE, "Review them")).clicked() {
        match &st.output_source {
            OutputSource::Project(id) => out.push(Msg::ReviewScope(Some(id.clone()))),
            OutputSource::ProjectFolder(f) => out.push(Msg::ReviewScopeFolder(f.clone())),
            _ => out.push(Msg::ReviewScope(None)),
        }
        out.push(Msg::Tab(Tab::Review));
    }
}

/// "Show outputs (12 kept)": a project's or project folder's kept images in
/// the Output library Grid.
pub fn show_outputs_button(ui: &mut egui::Ui, st: &AppState, source: OutputSource, out: &mut Vec<Msg>) -> bool {
    let n = st.source_images(&source).len();
    let r = ui.button(ic(ph::IMAGES, &format!("Show outputs ({n} kept)")));
    let clicked = r.on_hover_text("its good and neutral images, in Output library → Grid").clicked();
    if clicked {
        out.push(Msg::ShowOutputs(source));
    }
    clicked
}

/// After Show outputs: the source has no kept images (offering the All
/// tab and Review), or filters besides the ratings hide some. Returns
/// whether it showed.
fn output_notice(ui: &mut egui::Ui, st: &AppState, shown: usize, out: &mut Vec<Msg>) -> bool {
    let f = &st.output_filter;
    let all = st.source_images_in(&st.output_source, f.ratings).len();
    if all > 0 && !f.narrows() {
        return false;
    }
    egui::Frame::group(ui.style()).stroke(egui::Stroke::new(1.0, NEUTRAL)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            if all == 0 {
                ui.colored_label(NEUTRAL, ic(ph::WARNING, &format!("No kept images in {} yet.", source_name(st))));
                let any = st.source_images_in(&st.output_source, Ratings::ALL).len();
                if f.ratings != Ratings::ALL
                    && any > 0
                    && ui.button(ic(ph::IMAGES, &format!("Show all {any}, bad included"))).clicked()
                {
                    out.push(Msg::OutputPreset(OutputPreset::All));
                }
                let waiting = st.source_unreviewed(&st.output_source);
                if waiting > 0 {
                    ui.label(format!("{waiting} waiting in Review."));
                    review_them(ui, st, out);
                }
            } else {
                let hidden = all - shown;
                ui.colored_label(
                    NEUTRAL,
                    ic(ph::FUNNEL, &format!("Filters are on: showing {shown} of {all} ({hidden} hidden).")),
                );
                if ui.button(ic(ph::FUNNEL_X, "Reset filters")).clicked() {
                    out.push(Msg::ResetOutputFilters);
                }
            }
            if ui.small_button(ph::X).on_hover_text("dismiss").clicked() {
                out.push(Msg::DismissOutputNotice);
            }
        });
    });
    ui.add_space(6.0);
    true
}

/// "collection animals", "smart: 2026.10.24", "all kept images".
fn source_name(st: &AppState) -> String {
    match &st.output_source {
        OutputSource::AllProjects => "all projects".into(),
        OutputSource::Project(id) => st.index.project(id).map(|p| p.name()).unwrap_or_default(),
        OutputSource::ProjectFolder(f) if f.as_os_str().is_empty() => "all projects".into(),
        OutputSource::ProjectFolder(f) => format!("project folder {}", f.display()),
        OutputSource::Collection(f) if f.as_os_str().is_empty() => "all collections".into(),
        OutputSource::Collection(f) => format!("collection {}", f.display()),
        OutputSource::Smart(s) => format!("smart folder {s:?}"),
        OutputSource::Run(id) => match st.index.run(id) {
            Some((p, r)) => format!("run {} of {}", r.label, p.name()),
            None => format!("run {id}"),
        },
    }
}

fn pick(ui: &mut egui::Ui, st: &AppState, out: &mut Vec<Msg>, src: OutputSource, label: String) {
    let selected = st.output_source == src;
    let r = ui.selectable_label(selected, label);
    // after Show outputs: scroll it into view
    if selected {
        if st.output_reveal {
            r.scroll_to_me(Some(egui::Align::Center));
        }
        // for the tour's reveal check: where it is, what's visible, which frame
        let seen = (r.rect, ui.clip_rect(), ui.ctx().cumulative_pass_nr());
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique("tour_selected_source"), seen));
    }
    if r.clicked() {
        out.push(Msg::OutputSource(src));
    }
}

/// The "by project" smart folder: the project tree (what has kept images),
/// each folder a source of its own. A folder holding the selected source
/// starts open; Show outputs starts the tree over, only the way to its
/// source open. `q` (lower case) is the project-name search: only matching
/// projects and folders (a matching folder with all it holds), every folder
/// open.
fn by_project(ui: &mut egui::Ui, st: &AppState, folder: &Path, q: &str, out: &mut Vec<Msg>) {
    let (folders, projects) = st.kept_project_children(folder);
    let selected_dir = match &st.output_source {
        OutputSource::ProjectFolder(f) => Some(Path::new("projects").join(f)),
        OutputSource::Project(id) => st.index.projects.iter().find(|p| &p.project.id == id).map(|p| p.dir.clone()),
        OutputSource::Run(id) => st.index.run(id).map(|(p, _)| p.dir.clone()),
        _ => None,
    };
    let searching = !q.is_empty();
    for f in folders {
        let name = f.file_name().unwrap_or_default().to_string_lossy();
        let own = searching && name.to_lowercase().contains(q);
        if searching && !own && !kept_match(st, f, q) {
            continue;
        }
        let dir = Path::new("projects").join(f);
        let inside = searching || selected_dir.as_ref().is_some_and(|d| d.starts_with(&dir) && *d != dir);
        let src = OutputSource::ProjectFolder(f.clone());
        let n = st.source_images(&src).len();
        // while searching, open state of its own (so the search leaves the tree as it was)
        let id = ui.make_persistent_id(("by project", f, searching, st.output_reveals));
        // a folder whose name matches shows all it holds
        let q = if own { "" } else { q };
        let (toggle, ..) = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, inside)
            .show_header(ui, |ui| pick(ui, st, out, src, ic(ph::FOLDER, &format!("{name} ({n})"))))
            .body(|ui| by_project(ui, st, f, q, out));
        // for the tour's reveal check: the open/close arrow
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique(("tour_folder_toggle", &**f)), toggle.rect));
    }
    for p in projects {
        if searching && !p.name().to_lowercase().contains(q) {
            continue;
        }
        let src = OutputSource::Project(p.project.id.clone());
        let n = st.source_images(&src).len();
        let label = ic(ph::ROCKET_LAUNCH, &format!("{} ({n})", p.name()));
        // its runs with kept images, newest first; open while one is shown
        let runs = st.kept_runs(p);
        let run_shown = matches!(&st.output_source, OutputSource::Run(id) if runs.iter().any(|(r, _)| &r.id == id));
        let id = ui.make_persistent_id(("by project runs", &p.project.id, st.output_reveals));
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, run_shown)
            .show_header(ui, |ui| pick(ui, st, out, src, label))
            .body(|ui| {
                for (r, n) in runs {
                    let name = if r.label.is_empty() { r.id.to_string() } else { r.label.clone() };
                    let icon = if r.sweep.is_some() { ph::TABLE } else { ph::IMAGES };
                    pick(ui, st, out, OutputSource::Run(r.id.clone()), ic(icon, &format!("{name} ({n})")));
                }
            });
        // for the tour's reveal check: drawn this frame
        let pass = ui.ctx().cumulative_pass_nr();
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique(("tour_project_row", p.name())), pass));
    }
}

/// Whether anything in the kept project tree under `folder` has `q`
/// (lower case) in its name.
fn kept_match(st: &AppState, folder: &Path, q: &str) -> bool {
    let (folders, projects) = st.kept_project_children(folder);
    projects.iter().any(|p| p.name().to_lowercase().contains(q))
        || folders.iter().any(|f| {
            f.file_name().unwrap_or_default().to_string_lossy().to_lowercase().contains(q) || kept_match(st, f, q)
        })
}

/// The search box (images' titles and prompts, or project names) and the
/// filters, at the top of the sources panel.
fn search_panel(ui: &mut egui::Ui, st: &AppState, out: &mut Vec<Msg>) {
    let mut f = st.output_filter.clone();
    let projects = st.output_search_projects;
    // right to left: the clear button, then the field takes what's left
    let row = egui::vec2(ui.available_width(), ui.spacing().interact_size.y);
    ui.allocate_ui_with_layout(row, egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let mut q = if projects { st.output_project_search.clone() } else { f.text.clone() };
        let mut clear = !q.is_empty() && ui.small_button(ph::X).on_hover_text("clear").clicked();
        let hint = if projects { "Search project names" } else { "Search titles and prompts" };
        let r = ui.add(
            egui::TextEdit::singleline(&mut q)
                .hint_text(ic(ph::MAGNIFYING_GLASS, hint))
                .desired_width(ui.available_width()),
        );
        clear |= r.lost_focus() && ui.input(|i| i.key_pressed(Key::Escape));
        if clear {
            q.clear();
        }
        if projects && q != st.output_project_search {
            out.push(Msg::OutputProjectSearch(q));
        } else if !projects {
            f.text = q;
        }
    });
    ui.horizontal(|ui| {
        ui.weak("in");
        let images =
            ui.selectable_label(!projects, "Images").on_hover_text("the images' titles and their runs' prompts");
        if images.clicked() {
            out.push(Msg::OutputSearchProjects(false));
        }
        let names = ui.selectable_label(projects, "Project names").on_hover_text("narrow the by-project tree below");
        if names.clicked() {
            out.push(Msg::OutputSearchProjects(true));
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut f.has_refs, "has references");
        ui.checkbox(&mut f.transparent, "transparent");
    });
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut f.ratings.good, "good");
        ui.checkbox(&mut f.ratings.neutral, "neutral");
        ui.checkbox(&mut f.ratings.bad, "bad");
        ui.checkbox(&mut f.ratings.unreviewed, "unreviewed");
    });
    if f != st.output_filter {
        out.push(Msg::OutputFilter(f));
    }
}

/// Search and filters, then the sources: collections (All projects first,
/// as a virtual one), then the smart folders.
/// Show outputs' reveal lasts while the folders it opens animate open (so
/// the scroll lands on the row where it ends up), then ends.
fn reveal_done(ui: &egui::Ui, out: &mut Vec<Msg>) {
    let (ctx, id) = (ui.ctx(), egui::Id::unique("output reveal start"));
    let now = ctx.input(|i| i.time);
    let start = ctx.data_mut(|d| *d.get_temp_mut_or(id, now));
    if now - start > f64::from(ui.style().animation_time) * 2.0 + 0.1 {
        ctx.data_mut(|d| d.remove::<f64>(id));
        out.push(Msg::OutputRevealed);
    } else {
        ctx.request_repaint();
    }
}

fn sources_panel(ui: &mut egui::Ui, st: &AppState, out: &mut Vec<Msg>) {
    let kept = st.kept_outputs();
    search_panel(ui, st, out);
    ui.separator();

    ui.horizontal(|ui| {
        ui.label(RichText::new("Collections").strong());
        if ui.small_button(ph::FOLDER_PLUS).on_hover_text("New collection").clicked() {
            out.push(Msg::NewCollection(Default::default(), vec![]));
        }
    });
    let n = st.source_images_in(&OutputSource::AllProjects, st.output_filter.ratings).len();
    let all = ui.selectable_label(
        st.output_source == OutputSource::AllProjects,
        ic(ph::IMAGES, &format!("All projects ({n})")),
    );
    if all.on_hover_text("every project's images (a collection of everything)").clicked() {
        out.push(Msg::OutputSource(OutputSource::AllProjects));
    }
    let n_all = st.index.collection_links.len();
    pick(
        ui,
        st,
        out,
        OutputSource::Collection(Default::default()),
        ic(ph::FOLDERS, &format!("All collections ({n_all})")),
    );
    let mut folders = st.index.collection_folders.clone();
    folders.sort();
    for f in folders {
        let n = st.index.collection_links.iter().filter(|l| l.folder.starts_with(&f)).count();
        ui.horizontal(|ui| {
            ui.add_space(12.0 * f.components().count() as f32);
            let name = f.file_name().unwrap_or_default().to_string_lossy();
            let src = OutputSource::Collection(f.clone());
            let r = ui.selectable_label(st.output_source == src, ic(ph::FOLDER, &format!("{name} ({n})")));
            if r.clicked() {
                out.push(Msg::OutputSource(src));
            }
            r.context_menu(|ui| {
                if ui.button(ic(ph::FOLDER_PLUS, "New collection inside")).clicked() {
                    out.push(Msg::NewCollection(f.clone(), vec![]));
                    ui.close();
                }
                if ui.button(ic(ph::PENCIL_SIMPLE, "Rename")).clicked() {
                    out.push(Msg::StartRename(Tree::Collections, f.clone()));
                    ui.close();
                }
                if ui.button(ic(ph::FOLDER_OPEN, "Open in file manager")).clicked() {
                    out.push(Msg::ShowInFileManager(Path::new("collections").join(&f)));
                    ui.close();
                }
                if ui.button(ic(ph::TRASH, "Delete…")).clicked() {
                    let text = format!("Delete collection {name}? {}.", st.delete_summary(Tree::Collections, &f));
                    out.push(Msg::Ask(text, Box::new(Msg::DeleteNode(Tree::Collections, f.clone()))));
                    ui.close();
                }
            });
        });
    }

    ui.add_space(6.0);
    ui.label(RichText::new("Smart folders").strong());
    ui.collapsing("by date", |ui| {
        for (day, n) in st.kept_days() {
            pick(ui, st, out, OutputSource::Smart(Smart::Day(day.clone())), format!("{day} ({n})"));
        }
    });
    ui.collapsing("by source idea", |ui| {
        for (id, n) in st.kept_source_ideas() {
            let title =
                st.idea(&id).map(|e| e.idea.title.clone()).filter(|t| !t.is_empty()).unwrap_or_else(|| id.to_string());
            pick(ui, st, out, OutputSource::Smart(Smart::Idea(id)), format!("{} ({n})", short(&title, 26)));
        }
    });
    let q = if st.output_search_projects { st.output_project_search.trim().to_lowercase() } else { String::new() };
    let by_projects =
        matches!(st.output_source, OutputSource::Project(_) | OutputSource::ProjectFolder(_) | OutputSource::Run(_));
    let mut header = egui::CollapsingHeader::new("by project").default_open(by_projects);
    if !q.is_empty() || (st.output_reveal && by_projects) {
        header = header.open(Some(true));
    }
    let shown = header.show(ui, |ui| {
        let top = OutputSource::ProjectFolder(PathBuf::new());
        if st.output_source == top {
            let n = st.source_images(&top).len();
            pick(ui, st, out, top, ic(ph::FOLDER, &format!("All projects ({n})")));
        }
        if !q.is_empty() && !kept_match(st, Path::new(""), &q) {
            ui.weak(format!("No project name has “{q}”."));
        }
        by_project(ui, st, Path::new(""), &q, out);
    });
    let at = shown.header_response.rect;
    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique("tour_by_project_header"), at));
    let n_alpha = kept.iter().filter(|o| st.index.transparent(&o.image)).count();
    pick(
        ui,
        st,
        out,
        OutputSource::Smart(Smart::Transparent),
        ic(ph::CHECKERBOARD, &format!("transparent ({n_alpha})")),
    );
}

fn lineage_row(
    ui: &mut egui::Ui,
    st: &AppState,
    root: &Path,
    tex: &mut Textures,
    out: &mut Vec<Msg>,
    label: &str,
    imgs: Vec<increart_core::ImageRef>,
) {
    if imgs.is_empty() {
        return;
    }
    ui.weak(label);
    ui.horizontal_wrapped(|ui| {
        for i in imgs {
            let t = tex.thumb(root, &i);
            let r = thumb(ui, t.as_ref(), 64.0, false, None).on_hover_text(format!(
                "{}{}\ndouble-click: full size",
                tex.dims_text(root, &i),
                st.usage(&i).label().map(|u| format!("\nused: {u}")).unwrap_or_default()
            ));
            r.context_menu(|ui| copy_image_item(ui, root, &i, out));
            if r.double_clicked() {
                out.push(Msg::Preview(Some(i.clone())));
            }
            if r.clicked() && i.store == increart_core::Store::Output {
                out.push(Msg::OutputSelect(Some(i)));
            }
        }
    });
}

/// Source idea → parent ← image → children, and the recipe.
fn lineage_panel(
    ui: &mut egui::Ui,
    st: &AppState,
    root: &Path,
    tex: &mut Textures,
    image: &increart_core::ImageRef,
    out: &mut Vec<Msg>,
) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Lineage").strong());
        if ui.small_button(ph::X).on_hover_text("close").clicked() {
            out.push(Msg::ShowInfo(InfoPanel::Output, false));
        }
    });
    let link = st.output_link(image);
    if link.is_some() {
        ui.horizontal(|ui| {
            ui.weak("title");
            match st.image_title(image) {
                Some(t) => {
                    ui.label(RichText::new(t).strong());
                }
                None => {
                    ui.weak("none");
                }
            }
            if ui.small_button(ph::PENCIL_SIMPLE).on_hover_text("give it a title   F2").clicked() {
                out.push(Msg::TitleDialog(Some(image.clone())));
            }
        });
    }
    if let Some(p) = link.and_then(|o| st.project_of_link(o)) {
        ui.horizontal(|ui| {
            ui.weak("project");
            if ui.link(ic(ph::ROCKET_LAUNCH, &p.name())).on_hover_text("open it in Projects").clicked() {
                out.push(Msg::SelectProject(p.project.id.clone()));
                out.push(Msg::Tab(Tab::Projects));
            }
        });
    }
    let run = link.and_then(|o| st.index.run(&o.name.run)).map(|(_, r)| r.clone());
    if let Some(r) = &run {
        ui.horizontal(|ui| {
            ui.weak("run");
            if ui.link(&r.label).on_hover_text("the run's details").clicked() {
                out.push(Msg::JobInfo(Some(r.id.clone())));
            }
            if ui
                .small_button(ic(ph::CLOCK_COUNTER_CLOCKWISE, "history"))
                .on_hover_text("the Generate history that led to this run")
                .clicked()
            {
                out.push(Msg::RunHistory(r.id.clone()));
            }
        });
        if let Some(t) = link.and_then(|o| r.timings.iter().find(|t| t.seed == o.name.seed)) {
            ui.horizontal_wrapped(|ui| {
                ui.weak(ph::TIMER);
                ui.weak(increart_gui_core::timing::image_text(t, r.sample.steps));
            });
        }
    }
    if let Some(idea) = run.as_ref().and_then(|r| r.source_idea.as_ref()).and_then(|id| st.idea(id)) {
        ui.weak("source idea");
        ui.label(if idea.idea.title.is_empty() { "(untitled)".to_string() } else { idea.idea.title.clone() });
    }
    let (parent, children) = st.lineage(image);
    if let Some(r) = &run {
        lineage_row(ui, st, root, tex, out, "references", r.refs.clone());
    }
    lineage_row(ui, st, root, tex, out, "edited from (parent)", parent.into_iter().collect());
    ui.weak("this image");
    let t = tex.thumb(root, image);
    if thumb(ui, t.as_ref(), 160.0, true, None).on_hover_text("double-click: full size").double_clicked() {
        out.push(Msg::Preview(Some(image.clone())));
    }
    if let Some(u) = st.usage(image).label() {
        ui.weak(format!("used: {u}"));
    }
    lineage_row(ui, st, root, tex, out, "used by (children)", children);
    ui.separator();
    ui.label(RichText::new("Recipe").strong());
    recipe_grid(ui, st, root, tex, image, out);
}

/// "Add to collection ▸": existing collections and a new one.
pub fn collection_menu(ui: &mut egui::Ui, st: &AppState, images: &[increart_core::ImageRef], out: &mut Vec<Msg>) {
    ui.menu_button(ic(ph::FOLDER_PLUS, "Add to collection"), |ui| {
        let mut folders = st.index.collection_folders.clone();
        folders.sort();
        for f in folders {
            if ui.button(f.display().to_string()).clicked() {
                out.push(Msg::AddToCollection(f.clone(), images.to_vec()));
                ui.close();
            }
        }
        ui.separator();
        if ui.button(ic(ph::PLUS, "New collection…")).clicked() {
            out.push(Msg::NewCollection(Default::default(), images.to_vec()));
            ui.close();
        }
    });
}

fn image_grid(
    ui: &mut egui::Ui,
    st: &AppState,
    root: &Path,
    tex: &mut Textures,
    out: &mut Vec<Msg>,
    images: &[&OutputLink],
    rejected: bool,
) {
    let bg = crate::tiles::background(ui, st, out);
    let mut rects = vec![];
    ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::splat(6.0);
            for o in images {
                let t = tex.thumb(root, &o.image);
                let tip = st
                    .index
                    .run(&o.name.run)
                    .map(|(_, run)| format!("{}\n{}\ndouble-click: full size", describe(st, o), short(&run.prompt, 80)))
                    .unwrap_or_else(|| "double-click: full size".into());
                let selected = crate::tiles::cursor(st, st.output_selected.as_ref() == Some(&o.image));
                let r = thumb(ui, t.as_ref(), 180.0, selected, Some(verdict_color(o.verdict))).on_hover_text(tip);
                if rects.is_empty() {
                    // where it is, for the screenshot tour's double-click check
                    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique("tour_first_output"), r.rect));
                }
                crate::tiles::mark(ui, st, r.rect, &o.image);
                rects.push((o.image.clone(), r.rect));
                // the lineage panel opens on a single click only (a double-click
                // previews); while it's open, a click shows the image in it at once
                let picked = crate::tiles::click(ui, st, &r, &o.image, out);
                let open = if st.output_selected.is_some() { r.clicked() } else { single_click(ui, &r) };
                if !picked && open {
                    out.push(Msg::OutputSelect(Some(o.image.clone())));
                }
                if r.double_clicked() {
                    out.push(Msg::Preview(Some(o.image.clone())));
                }
                r.context_menu(|ui| {
                    crate::tiles::menu(ui, st, &o.image, out);
                    if ui.button(ic(ph::ARROW_COUNTER_CLOCKWISE, "Back to Review")).clicked() {
                        out.push(Msg::ReviewLink(o.path.clone(), None));
                        ui.close();
                    }
                    for (label, v, c) in VERDICTS {
                        if o.verdict != Some(v)
                            && ui.button(RichText::new(format!("Rate {}", label.to_lowercase())).color(c)).clicked()
                        {
                            out.push(Msg::ReviewLink(o.path.clone(), Some(v)));
                            ui.close();
                        }
                    }
                    ui.separator();
                    collection_menu(ui, st, std::slice::from_ref(&o.image), out);
                    if let OutputSource::Collection(f) = &st.output_source
                        && let Some(link) = st.index.collection_links.iter().find(|l| l.image == o.image && l.folder.starts_with(f))
                        && ui.button(ic(ph::MINUS_CIRCLE, "Remove from this collection")).clicked()
                    {
                        let rel = link.path.strip_prefix("collections").map(Path::to_path_buf).unwrap_or_default();
                        out.push(Msg::DeleteNode(Tree::Collections, rel));
                        ui.close();
                    }
                    let title = if st.image_title(&o.image).is_some() { "Edit title…" } else { "Set title…" };
                    if ui.button(ic(ph::TEXT_AA, title)).clicked() {
                        out.push(Msg::TitleDialog(Some(o.image.clone())));
                        ui.close();
                    }
                    if let Some(p) = st.project_of_link(o)
                        && ui.button(ic(ph::IMAGE_SQUARE, "Use as project cover")).clicked()
                    {
                        out.push(Msg::SetCover(p.project.id.clone(), Some(o.image.clone())));
                        ui.close();
                    }
                    if ui.button(ic(ph::EXPORT, "Export…")).clicked() {
                        out.push(Msg::OpenExport(vec![o.image.clone()], "1 image".into()));
                        ui.close();
                    }
                    if ui.button(ic(ph::FOLDER_OPEN, "Show in file manager")).clicked() {
                        out.push(Msg::ShowInFileManager(o.path.clone()));
                        ui.close();
                    }
                    if ui.button(ic(ph::COPY, "Copy path")).clicked() {
                        let p = st.library.as_ref().map(|r| r.join(&o.path)).unwrap_or_else(|| o.path.clone());
                        ui.ctx().copy_text(p.display().to_string());
                        ui.close();
                    }
                    if rejected && ui.button(ic(ph::TRASH, "Delete permanently…")).clicked() {
                        let text = "Delete this image? Its link goes to the trash; the image stays in the store until Clean up.";
                        out.push(Msg::Ask(text.into(), Box::new(Msg::DeleteOutput(o.path.clone()))));
                        ui.close();
                    }
                    ui.separator();
                    use_menu(ui, root, &o.image, out);
                });
            }
        });
    });
    crate::tiles::band(ui, st, &bg, &rects, out);
}
