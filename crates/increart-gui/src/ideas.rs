//! Idea inbox (New idea · Grid · Discarded) and Input library (Grid with the
//! folder tree).

use std::path::Path;

use eframe::egui::{self, Align2, Color32, FontId, Key, Panel, RichText, ScrollArea, Sense, Stroke};
use increart_core::scan::IdeaEntry;
use increart_core::{IdeaLoc, ImageRef, Tree, Verdict};
use increart_gui_core::{
    AddIdea, AppState, IdeaAge, IdeaOrigin, InboxFilter, InboxTab, InfoPanel, LibTab, Msg, PickKind, Tab, ZoomGrid,
};

use crate::textures::Textures;
use crate::tree;
use crate::widgets::{ACCENT, NEUTRAL, card, ic, ph, short, single_click, thumb};

/// Idea cards (Idea inbox and Input library): all one size (the Input
/// library's zooms), the first image square across the full width, the
/// text cut to fit.
const CARD_W: f32 = 210.0;
/// Below the image: title, facts, two lines of text and the tags.
const CARD_TEXT_H: f32 = 92.0;

/// An idea's card. Double-click: its first image full size (the editor
/// when it has none); the pencil on the image (on hover, or selected): the
/// editor.
#[allow(clippy::too_many_arguments)]
fn idea_card(
    ui: &mut egui::Ui,
    st: &AppState,
    root: &Path,
    tex: &mut Textures,
    e: &IdeaEntry,
    w: f32,
    selected: bool,
    out: &mut Vec<Msg>,
) -> egui::Response {
    let r = idea_card_body(ui, st, root, tex, e, w, selected);
    // over the card, so it gets the click rather than the card
    let edit = egui::Rect::from_min_size(r.rect.left_top() + egui::vec2(w - 26.0, 13.0), egui::vec2(28.0, 26.0));
    if selected || ui.rect_contains_pointer(r.rect) {
        let b = ui.interact(edit, r.id.with("edit"), egui::Sense::click());
        let fill = if b.hovered() { Color32::from_black_alpha(220) } else { Color32::from_black_alpha(150) };
        ui.painter().rect_filled(edit, 4.0, fill);
        let color = if b.hovered() { Color32::WHITE } else { Color32::from_gray(210) };
        ui.painter().text(edit.center(), Align2::CENTER_CENTER, ph::PENCIL_SIMPLE, FontId::proportional(16.0), color);
        let tip = if st.tab == Tab::IdeaInbox { "Edit idea   Enter" } else { "Edit idea" };
        if b.on_hover_text(tip).clicked() {
            out.push(Msg::OpenIdea(e.idea.id.clone()));
        }
    }
    if r.double_clicked() {
        match e.idea.images.first() {
            Some(img) => out.push(Msg::Preview(Some(ImageRef::input(img.clone())))),
            None => out.push(Msg::OpenIdea(e.idea.id.clone())),
        }
    }
    r
}

fn idea_card_body(
    ui: &mut egui::Ui,
    st: &AppState,
    root: &Path,
    tex: &mut Textures,
    e: &IdeaEntry,
    w: f32,
    selected: bool,
) -> egui::Response {
    let usage = st.idea_usage(e);
    card(ui, selected, w, w + CARD_TEXT_H, |ui| {
        ui.spacing_mut().item_spacing.y = 3.0;
        match e.idea.images.first().map(|id| tex.thumb(root, &ImageRef::input(id.clone()))) {
            Some(t) => {
                thumb(ui, t.as_ref(), w, false, None);
            }
            None => {
                let (r, _) = ui.allocate_exact_size(egui::vec2(w, w), egui::Sense::hover());
                ui.painter().rect_filled(r, 2.0, Color32::from_gray(28));
                ui.painter().text(
                    r.center(),
                    Align2::CENTER_CENTER,
                    ph::TEXT_T,
                    FontId::proportional(56.0),
                    Color32::from_gray(90),
                );
            }
        }
        // what it has, on one line
        let mut facts = vec![];
        if e.idea.images.len() > 1 {
            facts.push(format!("+{} images", e.idea.images.len() - 1));
        }
        if !e.idea.variants.is_empty() {
            facts.push(format!("{} variants", e.idea.variants.len()));
        }
        if !e.idea.projects.is_empty() {
            facts.push(ic(ph::ROCKET_LAUNCH, &e.idea.projects.len().to_string()));
        }
        if usage.label().is_some() {
            facts.push(ic(ph::ARROW_BEND_DOWN_RIGHT, &format!("{} ref · {} start", usage.ref_runs, usage.start_runs)));
        }
        let title = if e.idea.title.is_empty() { "(untitled)" } else { &e.idea.title };
        ui.horizontal(|ui| {
            if e.idea.starred {
                ui.label(RichText::new(ph::STAR).color(Color32::GOLD));
            }
            ui.add(egui::Label::new(RichText::new(title).strong()).truncate());
        });
        let facts = ui.add(egui::Label::new(RichText::new(facts.join("  ")).weak().small()).truncate());
        if let Some(text) = usage.label() {
            facts.on_hover_text(format!("its images: {text}"));
        }
        // the text: two lines at most
        let color = ui.visuals().text_color();
        let mut job = egui::text::LayoutJob::simple(e.idea.text.clone(), FontId::proportional(11.0), color, w);
        job.wrap.max_rows = 2;
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        ui.add(egui::Label::new(galley).selectable(false));
        if !e.idea.tags.is_empty() {
            ui.add(egui::Label::new(RichText::new(e.idea.tags.join(", ")).weak()).truncate());
        }
    })
}

/// Files dragged from the desktop onto the Idea inbox: highlight while
/// hovering, import on drop.
fn desktop_drop(ui: &mut egui::Ui, out: &mut Vec<Msg>) {
    let ctx = ui.ctx().clone();
    let (hovering, dropped) = ctx.input(|i| (!i.raw.hovered_files.is_empty(), i.raw.dropped_files.clone()));
    if !dropped.is_empty() {
        out.push(Msg::DropFiles(dropped.iter().map(|f| f.path().to_path_buf()).collect()));
    }
    if hovering {
        let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("inbox_drop_overlay")));
        let r = ui.max_rect().shrink(12.0);
        p.rect(
            r,
            12.0,
            Color32::from_rgba_unmultiplied(40, 90, 160, 120),
            Stroke::new(3.0, ACCENT),
            egui::StrokeKind::Inside,
        );
        p.text(
            r.center(),
            Align2::CENTER_CENTER,
            ic(ph::TRAY, "Drop to import into the Idea inbox"),
            FontId::proportional(28.0),
            Color32::WHITE,
        );
    }
}

pub fn inbox(ui: &mut egui::Ui, st: &AppState, tex: &mut Textures, out: &mut Vec<Msg>) {
    desktop_drop(ui, out);
    let n = st.unreviewed_ideas().len();
    Panel::top("inbox_tabs").show(ui, |ui| {
        // groups wrap whole when the bar is short of room
        use crate::widgets::{group, right_group};
        ui.horizontal_wrapped(|ui| {
            group(ui, "tabs", |ui| {
                for (t, label) in [
                    (InboxTab::NewIdea, ic(ph::PLUS, "New idea")),
                    (InboxTab::Grid, ic(ph::SQUARES_FOUR, &format!("Grid ({n})"))),
                    (InboxTab::Folder, ic(ph::FOLDER, "Folder")),
                    (InboxTab::Import, ic(ph::FILE_ARROW_DOWN, &format!("Import ({})", st.index.inbox_drop.len()))),
                    (InboxTab::Discarded, ic(ph::TRASH, &format!("Discarded ({})", st.discarded_ideas().len()))),
                ] {
                    if ui.selectable_label(st.inbox_tab == t, label).clicked() {
                        out.push(Msg::InboxTab(t));
                    }
                }
            });
            group(ui, "import", |ui| {
                ui.separator();
                if ui
                    .button(ic(ph::FILE_ARROW_DOWN, "Import files…"))
                    .on_hover_text("images and .txt files become new ideas")
                    .clicked()
                {
                    out.push(Msg::Pick(PickKind::ImportFiles));
                }
            });
            if st.inbox_tab == InboxTab::Grid {
                right_group(ui, "right", |ui| crate::widgets::details_toggle(ui, st, InfoPanel::Inbox, out));
            }
        });
    });
    match st.inbox_tab {
        InboxTab::NewIdea => new_idea(ui, st, tex, out),
        InboxTab::Grid => inbox_grid(ui, st, tex, out),
        InboxTab::Folder => {
            egui::CentralPanel::default().show(ui, |ui| crate::folderview::view(ui, st, tex, out));
        }
        InboxTab::Import => import(ui, st, tex, out),
        InboxTab::Discarded => discarded(ui, st, tex, out),
    }
}

/// The Import tab: files waiting in `inbox_drop/`, recent imports with what
/// changed on import.
fn import(ui: &mut egui::Ui, st: &AppState, tex: &mut Textures, out: &mut Vec<Msg>) {
    if st.imports.is_none() {
        out.push(Msg::LoadImports);
    }
    egui::CentralPanel::default().show(ui, |ui| {
        let Some(root) = st.library.as_deref() else { return };
        ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading(ic(ph::TRAY_ARROW_DOWN, "Drop folder"));
                if ui.button(ic(ph::FOLDER_OPEN, "Open in file manager")).clicked() {
                    out.push(Msg::ShowInFileManager("input/inbox_drop".into()));
                }
            });
            ui.weak("Anything put in input/inbox_drop/ becomes an idea by itself, once it has finished copying: images as image ideas, .txt files as text. Originals move to imported/.");
            if !st.watch_status.is_empty() {
                ui.weak(format!("Library: {}", st.watch_status));
            }
            if st.index.inbox_drop.is_empty() {
                ui.weak("No files waiting.");
            } else {
                for f in &st.index.inbox_drop {
                    ui.monospace(f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
                }
                if ui.button(RichText::new(ic(ph::DOWNLOAD_SIMPLE, &format!("Import now ({})", st.index.inbox_drop.len()))).strong()).clicked() {
                    out.push(Msg::ImportDropNow);
                }
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.heading(ic(ph::CLOCK_COUNTER_CLOCKWISE, "Recent imports"));
                if ui.small_button(ph::ARROWS_CLOCKWISE).on_hover_text("refresh").clicked() {
                    out.push(Msg::LoadImports);
                }
            });
            match &st.imports {
                None => {
                    ui.spinner();
                }
                Some(list) if list.is_empty() => {
                    ui.weak("Nothing imported yet.");
                }
                Some(list) => {
                    egui::Grid::new("imports").num_columns(4).spacing([12.0, 6.0]).striped(true).show(ui, |ui| {
                        for (id, src) in list {
                            let t = tex.thumb(root, &ImageRef::input(id.clone()));
                            let r = thumb(ui, t.as_ref(), 48.0, false, None).on_hover_text("double-click to view");
                            if r.double_clicked() {
                                out.push(Msg::Preview(Some(ImageRef::input(id.clone()))));
                            }
                            r.context_menu(|ui| crate::widgets::image_menu_items(ui, root, &ImageRef::input(id.clone()), out));
                            ui.vertical(|ui| {
                                ui.set_width(360.0);
                                ui.add(egui::Label::new(src.original_name.clone().unwrap_or_else(|| "(clipboard)".into())).truncate());
                                ui.add(egui::Label::new(RichText::new(src.original_path.clone().unwrap_or_default()).weak().small()).truncate());
                            });
                            ui.label(format!(
                                "{} · {} × {} · {} KB",
                                src.original_format,
                                src.original_width,
                                src.original_height,
                                src.original_size / 1024
                            ));
                            ui.weak(if src.conversions.is_empty() { "unchanged".into() } else { src.conversions.join(", ") });
                            ui.end_row();
                        }
                    });
                }
            }
        });
    });
}

/// The Edit idea window: title, tags, text (with history), images, variants.
pub fn edit_window(ctx: &egui::Context, st: &AppState, tex: &mut Textures, out: &mut Vec<Msg>) {
    let Some(ed) = &st.idea_edit else { return };
    let Some(root) = st.library.as_deref() else { return };
    let idea = st.idea(&ed.id).map(|e| e.idea.clone());
    let mut open = true;
    egui::Window::new(ic(ph::PENCIL_SIMPLE, "Edit idea")).open(&mut open).default_width(620.0).collapsible(false).show(
        ctx,
        |ui| {
            let mut e = ed.clone();
            ui.label("Title");
            ui.add(egui::TextEdit::singleline(&mut e.title).desired_width(f32::INFINITY));
            ui.label("Tags (comma-separated)");
            ui.add(egui::TextEdit::singleline(&mut e.tags).desired_width(f32::INFINITY));
            ui.horizontal(|ui| {
                ui.label("Text");
                if ui.small_button(ic(ph::CLOCK_COUNTER_CLOCKWISE, "History…")).clicked() {
                    out.push(Msg::OpenHistory(increart_core::HistoryKey::Idea(e.id.clone())));
                }
            });
            ui.add(egui::TextEdit::multiline(&mut e.text).desired_rows(8).desired_width(f32::INFINITY));
            ui.label("Images");
            ui.horizontal_wrapped(|ui| {
                let mut remove = None;
                for (i, id) in e.images.iter().enumerate() {
                    ui.vertical(|ui| {
                        let t = tex.thumb(root, &ImageRef::input(id.clone()));
                        let r = thumb(ui, t.as_ref(), 96.0, false, None).on_hover_text("double-click to view");
                        r.context_menu(|ui| {
                            crate::widgets::image_menu_items(ui, root, &ImageRef::input(id.clone()), out)
                        });
                        if r.double_clicked() {
                            out.push(Msg::Preview(Some(ImageRef::input(id.clone()))));
                        }
                        if ui.small_button(ph::X).on_hover_text("remove from the idea").clicked() {
                            remove = Some(i);
                        }
                    });
                }
                if let Some(i) = remove {
                    e.images.remove(i);
                }
                if e.images.is_empty() {
                    ui.weak("none");
                }
            });
            if let Some(i) = &idea
                && !i.variants.is_empty()
            {
                ui.label(format!("Variants ({})", i.variants.len()));
                for v in &i.variants {
                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new(RichText::new(short(&v.text, 90)).small()).wrap());
                        if ui.small_button("use").on_hover_text("make this the text").clicked() {
                            e.text = v.text.clone();
                        }
                    });
                }
            }
            if &e != ed {
                out.push(Msg::EditIdea(e.clone()));
            }
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button(RichText::new(ic(ph::FLOPPY_DISK, "Save")).strong()).clicked() {
                    out.push(Msg::SaveIdea);
                }
                if ui.button("Cancel").clicked() {
                    out.push(Msg::CloseIdea);
                }
                if ui.button(ic(ph::FOLDER_OPEN, "Open folder")).clicked()
                    && let Some(entry) = st.idea(&ed.id)
                {
                    out.push(Msg::ShowInFileManager(entry.dir.clone()));
                }
                let target = increart_gui_core::PeTarget::Idea(ed.id.clone());
                crate::rewrite::button(ui, st, target, "Rewrite with PE", out);
            });
        },
    );
    if !open {
        out.push(Msg::CloseIdea);
    }
}

fn new_idea(ui: &mut egui::Ui, st: &AppState, tex: &mut Textures, out: &mut Vec<Msg>) {
    egui::CentralPanel::default().show(ui, |ui| {
        ui.set_max_width(760.0);
        ui.heading("New idea");
        let mut d = st.draft.clone();
        ui.label("Title");
        ui.add(egui::TextEdit::singleline(&mut d.title).desired_width(f32::INFINITY));
        ui.label("Text: prompt draft, notes, or both");
        ui.add(egui::TextEdit::multiline(&mut d.text).desired_rows(8).desired_width(f32::INFINITY));
        ui.label("Tags (comma-separated)");
        ui.add(egui::TextEdit::singleline(&mut d.tags).desired_width(f32::INFINITY));
        if d != st.draft {
            out.push(Msg::Draft(d));
        }
        ui.add_space(6.0);
        ui.label("Images");
        ui.horizontal_wrapped(|ui| {
            if let Some(root) = st.library.as_deref() {
                for (i, id) in st.draft.images.iter().enumerate() {
                    ui.vertical(|ui| {
                        let t = tex.thumb(root, &ImageRef::input(id.clone()));
                        let resp = thumb(ui, t.as_ref(), 96.0, false, None);
                        if i == 0 {
                            // where it is, for the screenshot tour's double-click check
                            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique("tour_first_draft_image"), resp.rect));
                        }
                        let resp = resp.on_hover_text("double-click to view");
                        if resp.double_clicked() {
                            out.push(Msg::Preview(Some(ImageRef::input(id.clone()))));
                        }
                        resp.context_menu(|ui| {
                            crate::widgets::image_menu_items(ui, root, &ImageRef::input(id.clone()), out)
                        });
                        if ui.small_button(ph::X).on_hover_text("remove").clicked() {
                            out.push(Msg::RemoveDraftImage(i));
                        }
                    });
                }
            }
            if ui.button(ic(ph::IMAGE, "Add images…")).clicked() {
                out.push(Msg::Pick(PickKind::IdeaImages));
            }
        });
        ui.weak("Files dropped on this window become ideas of their own.");
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.button(ic(ph::TRAY, "Add to inbox")).clicked() {
                out.push(Msg::AddIdea(AddIdea::Inbox));
            }
            if ui.button(ic(ph::BOOKMARK_SIMPLE, "Add & keep")).clicked() {
                out.push(Msg::AddIdea(AddIdea::Keep));
            }
            if ui.button(ic(ph::ROCKET_LAUNCH, "Add & promote into project")).clicked() {
                out.push(Msg::AddIdea(AddIdea::Promote));
            }
            crate::rewrite::button(ui, st, increart_gui_core::PeTarget::NewIdea, "Rewrite with PE", out);
            if ui.button(ic(ph::CLOCK_COUNTER_CLOCKWISE, "History…")).clicked() {
                out.push(Msg::OpenHistory(increart_core::HistoryKey::NewIdea));
            }
            crate::app::draft_note(ui, st, st.idea_dirty);
        });
    });
}

fn inbox_grid(ui: &mut egui::Ui, st: &AppState, tex: &mut Textures, out: &mut Vec<Msg>) {
    let ideas = st.inbox_queue();
    let current = ideas.get(st.inbox_cursor).map(|e| e.idea.id.clone());
    if !ui.ctx().egui_wants_keyboard_input() && st.preview.is_none() {
        ui.input(|i| {
            for (keys, v) in [
                ([Key::Num1, Key::G], Verdict::Good),
                ([Key::Num2, Key::N], Verdict::Neutral),
                ([Key::Num3, Key::B], Verdict::Bad),
            ] {
                if keys.iter().any(|k| i.key_pressed(*k)) {
                    out.push(Msg::ReviewCurrentIdea(v));
                }
            }
            if i.key_pressed(Key::ArrowRight) || i.key_pressed(Key::ArrowDown) {
                out.push(Msg::InboxCursor(st.inbox_cursor + 1));
            }
            if i.key_pressed(Key::ArrowLeft) || i.key_pressed(Key::ArrowUp) {
                out.push(Msg::InboxCursor(st.inbox_cursor.saturating_sub(1)));
            }
            if let Some(id) = &current
                && (i.key_pressed(Key::P) || i.key_pressed(Key::Space))
            {
                out.push(Msg::Promote(vec![id.clone()]));
            }
            if let Some(id) = &current
                && (i.key_pressed(Key::Enter) || i.key_pressed(Key::T))
            {
                out.push(Msg::OpenIdea(id.clone()));
            }
        });
    }
    Panel::left("inbox_side").default_size(230.0).show(ui, |ui| inbox_side(ui, st, out));
    if !st.info_hidden.contains(&InfoPanel::Inbox)
        && let Some(root) = st.library.as_deref()
    {
        Panel::right("inbox_info").default_size(300.0).show(ui, |ui| {
            ScrollArea::vertical().auto_shrink([false, true]).id_salt("inbox_info_scroll").show(ui, |ui| {
                match ideas.get(st.inbox_cursor) {
                    Some(e) => inbox_idea_panel(ui, st, root, tex, e, out),
                    None => crate::widgets::nothing_selected(
                        ui,
                        "Idea",
                        "No idea here: the inbox is empty, or the filters hide them all.",
                    ),
                }
            });
        });
    }
    egui::CentralPanel::default().show(ui, |ui| {
        let Some(root) = st.library.as_deref() else { return };
        let all = st.unreviewed_ideas().len();
        if all == 0 {
            ui.weak("Nothing to review. Add ideas with New idea, or drop files here.");
            return;
        }
        if st.inbox_filter.narrows() {
            ui.horizontal_wrapped(|ui| {
                let (shown, hidden) = (ideas.len(), all - ideas.len());
                let text = format!("Filters are on: showing {shown} of {all} ({hidden} hidden).");
                ui.colored_label(NEUTRAL, ic(ph::FUNNEL, &text));
                if ui.button(ic(ph::FUNNEL_X, "Clear filters")).clicked() {
                    let oldest_first = st.inbox_filter.oldest_first;
                    out.push(Msg::InboxFilter(InboxFilter { oldest_first, ..Default::default() }));
                }
            });
            ui.add_space(6.0);
        }
        ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (i, e) in ideas.iter().enumerate() {
                    let r = idea_card(ui, st, root, tex, e, CARD_W, i == st.inbox_cursor, out);
                    if r.clicked() {
                        out.push(Msg::InboxCursor(i));
                    }
                    r.context_menu(|ui| {
                        if let Some(img) = e.idea.images.first() {
                            crate::widgets::image_menu_items(ui, root, &ImageRef::input(img.clone()), out);
                        }
                        let id = &e.idea.id;
                        if ui.button(ic(ph::PENCIL_SIMPLE, "Edit…")).clicked() {
                            out.push(Msg::OpenIdea(id.clone()));
                            ui.close();
                        }
                        if ui.button(ic(ph::FOLDER_OPEN, "Open folder in file manager")).clicked() {
                            out.push(Msg::ShowInFileManager(e.dir.clone()));
                            ui.close();
                        }
                        for (label, v) in [
                            (ic(ph::STAR, "Keep in input library ★"), Verdict::Good),
                            (ic(ph::CHECK, "Keep in input library"), Verdict::Neutral),
                            (ic(ph::TRASH, "Discard"), Verdict::Bad),
                        ] {
                            if ui.button(label).clicked() {
                                out.push(Msg::ReviewIdea(id.clone(), v));
                                ui.close();
                            }
                        }
                        if ui.button(ic(ph::ROCKET_LAUNCH, "Promote into project")).clicked() {
                            out.push(Msg::Promote(vec![id.clone()]));
                            ui.close();
                        }
                        if ui.button(ic(ph::COPY, "Copy text")).clicked() {
                            ui.ctx().copy_text(e.idea.text.clone());
                            ui.close();
                        }
                    });
                }
            });
        });
    });
}

/// The Idea inbox Grid's sidebar: search, order, filters (each with how
/// many inbox ideas it holds) and tags; the keys at the bottom.
fn inbox_side(ui: &mut egui::Ui, st: &AppState, out: &mut Vec<Msg>) {
    Panel::bottom("inbox_keys").show(ui, |ui| {
        egui::CollapsingHeader::new(RichText::new(ic(ph::KEYBOARD, "Keys")).strong())
            .id_salt("inbox_keys")
            .default_open(true)
            .show(ui, |ui| {
                for (k, what) in [
                    ("1 / G", "keep ★ (shortlist)"),
                    ("2 / N", "keep"),
                    ("3 / B", "discard"),
                    ("P / Space", "promote"),
                    ("Enter / T", "edit"),
                    ("← →", "previous / next"),
                    ("Ctrl+N", "new idea"),
                ] {
                    ui.horizontal(|ui| {
                        ui.monospace(k);
                        ui.weak(what);
                    });
                }
                let note = "Kept ideas go to the input library; discarded ones can be restored.";
                ui.add(egui::Label::new(RichText::new(note).weak()).wrap());
            });
    });
    let mut f = st.inbox_filter.clone();
    let inbox = st.unreviewed_ideas();
    // right to left: the clear button, then the field takes what's left
    let row = egui::vec2(ui.available_width(), ui.spacing().interact_size.y);
    ui.allocate_ui_with_layout(row, egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if !f.text.is_empty() && ui.small_button(ph::X).on_hover_text("clear").clicked() {
            f.text.clear();
        }
        let r = ui.add(
            egui::TextEdit::singleline(&mut f.text)
                .hint_text(ic(ph::MAGNIFYING_GLASS, "Search ideas"))
                .desired_width(ui.available_width()),
        );
        if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Escape)) {
            f.text.clear();
        }
    });
    ui.horizontal(|ui| {
        for (oldest, label) in [(false, "newest first"), (true, "oldest first")] {
            if ui.selectable_label(f.oldest_first == oldest, label).clicked() {
                f.oldest_first = oldest;
            }
        }
    });
    ScrollArea::vertical().auto_shrink([false, true]).id_salt("inbox_side").show(ui, |ui| {
        ui.add_space(4.0);
        ui.label(RichText::new("Images").strong());
        ui.horizontal(|ui| {
            for (v, l) in [(None, "any"), (Some(true), "with images"), (Some(false), "text only")] {
                if ui.selectable_label(f.images == v, l).clicked() {
                    f.images = v;
                }
            }
        });
        ui.add_space(4.0);
        ui.label(RichText::new("Source").strong());
        if ui.selectable_label(f.origin.is_none(), format!("any ({})", inbox.len())).clicked() {
            f.origin = None;
        }
        for o in IdeaOrigin::ALL {
            let n = inbox.iter().filter(|e| IdeaOrigin::of(&e.idea.source) == o).count();
            if n == 0 && f.origin != Some(o) {
                continue;
            }
            if ui.selectable_label(f.origin == Some(o), format!("{} ({n})", o.label())).clicked() {
                f.origin = Some(o);
            }
        }
        ui.add_space(4.0);
        ui.label(RichText::new("Added").strong());
        if ui.selectable_label(f.age.is_none(), "any time").clicked() {
            f.age = None;
        }
        let now = increart_core::id::now();
        for a in IdeaAge::ALL {
            let n = inbox.iter().filter(|e| a.holds(e.idea.created, now)).count();
            if ui.selectable_label(f.age == Some(a), format!("{} ({n})", a.label())).clicked() {
                f.age = Some(a);
            }
        }
        // the inbox's tags, most used first; a click picks one, again lets go
        let mut tags: std::collections::BTreeMap<&str, usize> = Default::default();
        for t in inbox.iter().flat_map(|e| &e.idea.tags) {
            *tags.entry(t.as_str()).or_default() += 1;
        }
        let mut tags: Vec<_> = tags.into_iter().collect();
        tags.sort_by_key(|&(t, n)| (std::cmp::Reverse(n), t));
        ui.add_space(4.0);
        ui.label(RichText::new("Tags").strong());
        if tags.is_empty() {
            ui.weak("No tags in the inbox.");
        }
        ui.horizontal_wrapped(|ui| {
            for (t, n) in tags {
                let on = f.tag.as_deref() == Some(t);
                if ui.selectable_label(on, format!("{t} ({n})")).clicked() {
                    f.tag = if on { None } else { Some(t.to_string()) };
                }
            }
        });
    });
    if f != st.inbox_filter {
        out.push(Msg::InboxFilter(f));
    }
}

fn discarded(ui: &mut egui::Ui, st: &AppState, tex: &mut Textures, out: &mut Vec<Msg>) {
    egui::CentralPanel::default().show(ui, |ui| {
        let Some(root) = st.library.as_deref() else { return };
        let ideas = st.discarded_ideas();
        if ideas.is_empty() {
            ui.weak("No discarded ideas.");
            return;
        }
        ui.weak("Right-click to restore or delete.");
        ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for e in ideas {
                    let r = idea_card(ui, st, root, tex, e, CARD_W, false, out);
                    r.context_menu(|ui| {
                        if let Some(img) = e.idea.images.first() {
                            crate::widgets::image_menu_items(ui, root, &ImageRef::input(img.clone()), out);
                        }
                        let id = &e.idea.id;
                        if ui.button(ic(ph::TRAY, "Restore to the Idea inbox")).clicked() {
                            out.push(Msg::Restore(id.clone(), IdeaLoc::Unreviewed));
                            ui.close();
                        }
                        if ui.button(ic(ph::BOOKS, "Restore to the input library")).clicked() {
                            out.push(Msg::Restore(id.clone(), IdeaLoc::top_library()));
                            ui.close();
                        }
                        if ui.button(ic(ph::TRASH, "Delete permanently")).clicked() {
                            out.push(Msg::DeleteDiscarded(id.clone()));
                            ui.close();
                        }
                    });
                }
            });
        });
    });
}

pub fn input_library(ui: &mut egui::Ui, st: &AppState, tex: &mut Textures, out: &mut Vec<Msg>) {
    let Some(root) = st.library.clone() else { return };
    Panel::left("lib_tree").default_size(240.0).show(ui, |ui| {
        let mut f = st.lib_filter.clone();
        ui.add(egui::TextEdit::singleline(&mut f.text).hint_text("text or tag").desired_width(f32::INFINITY));
        ui.checkbox(&mut f.starred_only, ic(ph::STAR, "starred only"));
        ui.checkbox(&mut f.with_images_only, ic(ph::IMAGE, "with images only"));
        ui.horizontal(|ui| {
            ui.label("promoted:");
            for (v, l) in [(None, "any"), (Some(true), "yes"), (Some(false), "no")] {
                if ui.selectable_label(f.promoted == v, l).clicked() {
                    f.promoted = v;
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("order:");
            for (v, l, tip) in [
                (false, "newest", "newest first"),
                (true, "most used", "most runs using its images (as ref or start image) first"),
            ] {
                if ui.selectable_label(f.most_used == v, l).on_hover_text(tip).clicked() {
                    f.most_used = v;
                }
            }
        });
        if f != st.lib_filter {
            out.push(Msg::LibFilter(f));
        }
        ui.separator();
        ui.label(RichText::new("Folders").strong());
        if ui
            .selectable_label(st.lib_folder.is_none(), ic(ph::BOOKS, &format!("All ideas ({})", lib_count(st, None))))
            .clicked()
        {
            out.push(Msg::LibFolder(None));
        }
        tree::folders(ui, st, Tree::InputLibrary, &st.index.library_folders, st.lib_folder.as_deref(), out, |p| {
            Msg::LibFolder(Some(p.to_path_buf()))
        });
        ui.add_space(8.0);
        if ui.button(ic(ph::FOLDER_PLUS, "New folder")).clicked() {
            out.push(Msg::NewFolder(Tree::InputLibrary, st.lib_folder.clone().unwrap_or_default()));
        }
        ui.separator();
        let sel: Vec<_> = st.lib_selected.iter().cloned().collect();
        ui.label(format!("{} selected", sel.len()));
        ui.add_enabled_ui(!sel.is_empty(), |ui| {
            if ui.button(ic(ph::ROCKET_LAUNCH, "Promote selection into project")).clicked() {
                out.push(Msg::Promote(sel.clone()));
            }
            if ui
                .button(ic(ph::IMAGES, "Add images as refs"))
                .on_hover_text("to the Generate panel of the current project")
                .clicked()
            {
                let refs = sel
                    .iter()
                    .filter_map(|id| st.idea(id))
                    .flat_map(|e| e.idea.images.iter().cloned().map(ImageRef::input))
                    .collect();
                out.push(Msg::AddRefs(refs));
            }
            if ui.button(ic(ph::X, "Clear selection")).clicked() {
                out.push(Msg::LibClearSelection);
            }
        });
    });
    Panel::top("lib_tabs").show(ui, |ui| {
        // groups wrap whole when the bar is short of room
        use crate::widgets::{group, right_group};
        ui.horizontal_wrapped(|ui| {
            group(ui, "tabs", |ui| {
                if ui.selectable_label(st.lib_tab == LibTab::Grid, ic(ph::SQUARES_FOUR, "Grid")).clicked() {
                    out.push(Msg::LibTab(LibTab::Grid));
                }
                if ui.selectable_label(st.lib_tab == LibTab::Folder, ic(ph::FOLDER, "Folder")).clicked() {
                    out.push(Msg::LibTab(LibTab::Folder));
                }
                let n = st.discarded_ideas().len();
                if ui
                    .selectable_label(st.lib_tab == LibTab::Discarded, ic(ph::TRASH, &format!("Discarded ({n})")))
                    .clicked()
                {
                    out.push(Msg::LibTab(LibTab::Discarded));
                }
            });
            group(ui, "open", |ui| {
                ui.separator();
                if ui.button(ic(ph::FOLDER_OPEN, "Open in file manager")).clicked() {
                    let dir = std::path::Path::new("input/library").join(st.lib_folder.clone().unwrap_or_default());
                    out.push(Msg::ShowInFileManager(dir));
                }
            });
            if st.lib_tab == LibTab::Grid {
                group(ui, "zoom", |ui| {
                    ui.separator();
                    crate::review::grid_zoom(ui, st, ZoomGrid::Library, out);
                });
                right_group(ui, "right", |ui| crate::widgets::details_toggle(ui, st, InfoPanel::Library, out));
            }
        });
    });
    if st.lib_tab == LibTab::Discarded {
        return discarded(ui, st, tex, out);
    }
    if st.lib_tab == LibTab::Folder {
        egui::CentralPanel::default().show(ui, |ui| crate::folderview::view(ui, st, tex, out));
        return;
    }
    if !st.info_hidden.contains(&InfoPanel::Library) {
        Panel::right("lib_info").default_size(300.0).show(ui, |ui| {
            ScrollArea::vertical().auto_shrink([false, true]).id_salt("lib_info_scroll").show(ui, |ui| {
                match st.lib_info.as_ref().and_then(|id| st.idea(id)) {
                    Some(e) => idea_panel(ui, st, &root, tex, e, out),
                    None => crate::widgets::nothing_selected(
                        ui,
                        "Idea",
                        "No idea selected: click one to see it in full here.",
                    ),
                }
            });
        });
    }
    egui::CentralPanel::default().show(ui, |ui| {
        let ideas = st.library_ideas();
        if ideas.is_empty() {
            let filtered = st.lib_filter != increart_gui_core::LibFilter::default();
            ui.weak(if filtered {
                "No ideas match the filters."
            } else {
                "No kept ideas here. Keep ideas in the Idea inbox (1 / 2)."
            });
            return;
        }
        ui.weak("Click to select and see it on the right, Ctrl+click to add to the selection; double-click: full size; right-click for actions.");
        crate::review::grid_ctrl_scroll(ui, st, ZoomGrid::Library, out);
        let w = st.grid_cell_size(ZoomGrid::Library) as f32;
        ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (i, e) in ideas.into_iter().enumerate() {
                    let id = e.idea.id.clone();
                    let r = idea_card(ui, st, &root, tex, e, w, st.lib_selected.contains(&id), out);
                    if i == 0 {
                        // where it is, for the screenshot tour's click check
                        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique("tour_first_idea"), r.rect));
                    }
                    if r.clicked() {
                        out.push(Msg::LibSelect { id: id.clone(), additive: ui.input(|i| i.modifiers.command) });
                    }
                    // the side panel opens on a single click only (a double-click
                    // previews); while it's open, a click shows the idea in it at once
                    let plain = r.clicked() && !ui.input(|i| i.modifiers.any());
                    if if st.lib_info.is_some() { plain } else { single_click(ui, &r) } {
                        out.push(Msg::LibInfo(Some(id.clone())));
                    }
                    r.context_menu(|ui| {
                        if let Some(img) = e.idea.images.first() {
                            crate::widgets::image_menu_items(ui, &root, &ImageRef::input(img.clone()), out);
                        }
                        if ui.button(ic(ph::PENCIL_SIMPLE, "Edit…")).clicked() {
                            out.push(Msg::OpenIdea(id.clone()));
                            ui.close();
                        }
                        if ui.button(ic(ph::FOLDER_OPEN, "Open folder in file manager")).clicked() {
                            out.push(Msg::ShowInFileManager(e.dir.clone()));
                            ui.close();
                        }
                        let star = if e.idea.starred { ic(ph::STAR, "Unstar") } else { ic(ph::STAR, "Star") };
                        if ui.button(star).clicked() {
                            out.push(Msg::SetStar(id.clone(), !e.idea.starred));
                            ui.close();
                        }
                        if ui.button(ic(ph::ROCKET_LAUNCH, "Promote into project")).clicked() {
                            out.push(Msg::Promote(vec![id.clone()]));
                            ui.close();
                        }
                        if !e.idea.images.is_empty() && ui.button(ic(ph::IMAGES, "Add images as refs")).clicked() {
                            out.push(Msg::AddRefs(e.idea.images.iter().cloned().map(ImageRef::input).collect()));
                            ui.close();
                        }
                        if ui.button(ic(ph::TRAY, "Back to the Idea inbox")).clicked() {
                            out.push(Msg::Restore(id.clone(), IdeaLoc::Unreviewed));
                            ui.close();
                        }
                        if ui.button(ic(ph::TRASH, "Discard")).clicked() {
                            out.push(Msg::Restore(id.clone(), IdeaLoc::Discarded));
                            ui.close();
                        }
                        if ui.button(ic(ph::COPY, "Copy text")).clicked() {
                            ui.ctx().copy_text(e.idea.text.clone());
                            ui.close();
                        }
                    });
                }
            });
        });
    });
}

/// An Input library idea in the side panel: its images, text, what it's
/// been used for, and what to do with it.
fn idea_panel(ui: &mut egui::Ui, st: &AppState, root: &Path, tex: &mut Textures, e: &IdeaEntry, out: &mut Vec<Msg>) {
    let id = e.idea.id.clone();
    panel_header(ui, out, Msg::ShowInfo(InfoPanel::Library, false));
    idea_details(ui, st, root, tex, e, out);
    ui.separator();
    ui.horizontal_wrapped(|ui| {
        if ui.button(ic(ph::PENCIL_SIMPLE, "Edit…")).clicked() {
            out.push(Msg::OpenIdea(id.clone()));
        }
        if ui.button(ic(ph::ROCKET_LAUNCH, "Promote")).on_hover_text("into a project").clicked() {
            out.push(Msg::Promote(vec![id.clone()]));
        }
        if !e.idea.images.is_empty()
            && ui
                .button(ic(ph::IMAGES, "Add as refs"))
                .on_hover_text("to the current project's Generate panel")
                .clicked()
        {
            out.push(Msg::AddRefs(e.idea.images.iter().cloned().map(ImageRef::input).collect()));
        }
        if ui.button(ic(ph::COPY, "Copy text")).clicked() {
            ui.ctx().copy_text(e.idea.text.clone());
        }
        if ui.button(ic(ph::FOLDER_OPEN, "Open folder")).clicked() {
            out.push(Msg::ShowInFileManager(e.dir.clone()));
        }
    });
}

/// The Idea inbox Grid's current idea in the side panel: the same details,
/// and the triage actions (the keys, as buttons).
fn inbox_idea_panel(
    ui: &mut egui::Ui,
    st: &AppState,
    root: &Path,
    tex: &mut Textures,
    e: &IdeaEntry,
    out: &mut Vec<Msg>,
) {
    let id = e.idea.id.clone();
    panel_header(ui, out, Msg::ShowInfo(InfoPanel::Inbox, false));
    idea_details(ui, st, root, tex, e, out);
    ui.separator();
    ui.horizontal_wrapped(|ui| {
        for (label, tip, v) in [
            (ic(ph::STAR, "Keep ★"), "into the input library, starred (1 / G)", Verdict::Good),
            (ic(ph::CHECK, "Keep"), "into the input library (2 / N)", Verdict::Neutral),
            (ic(ph::TRASH, "Discard"), "can be restored from Discarded (3 / B)", Verdict::Bad),
        ] {
            if ui.button(label).on_hover_text(tip).clicked() {
                out.push(Msg::ReviewIdea(id.clone(), v));
            }
        }
    });
    ui.horizontal_wrapped(|ui| {
        if ui.button(ic(ph::ROCKET_LAUNCH, "Promote")).on_hover_text("into a project (P / Space)").clicked() {
            out.push(Msg::Promote(vec![id.clone()]));
        }
        if ui.button(ic(ph::PENCIL_SIMPLE, "Edit…")).on_hover_text("Enter / T").clicked() {
            out.push(Msg::OpenIdea(id.clone()));
        }
        if ui.button(ic(ph::COPY, "Copy text")).clicked() {
            ui.ctx().copy_text(e.idea.text.clone());
        }
        if ui.button(ic(ph::FOLDER_OPEN, "Open folder")).clicked() {
            out.push(Msg::ShowInFileManager(e.dir.clone()));
        }
    });
}

fn panel_header(ui: &mut egui::Ui, out: &mut Vec<Msg>, close: Msg) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Idea").strong());
        if ui.small_button(ph::X).on_hover_text("close").clicked() {
            out.push(close);
        }
    });
}

/// An idea's title, star, images, text and tags; then its sections: the
/// idea (when, where, size), its source, each image's file metadata, its
/// prompt variants, and (when there is any) its images' use and projects.
fn idea_details(ui: &mut egui::Ui, st: &AppState, root: &Path, tex: &mut Textures, e: &IdeaEntry, out: &mut Vec<Msg>) {
    let id = e.idea.id.clone();
    ui.horizontal(|ui| {
        let star =
            if e.idea.starred { RichText::new(ph::STAR).color(Color32::GOLD) } else { RichText::new(ph::STAR).weak() };
        if ui.add(egui::Button::new(star).frame(false)).on_hover_text("star").clicked() {
            out.push(Msg::SetStar(id.clone(), !e.idea.starred));
        }
        let title = if e.idea.title.is_empty() { "(untitled)" } else { &e.idea.title };
        ui.add(egui::Label::new(RichText::new(title).strong()).wrap());
    });
    if !e.idea.images.is_empty() {
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            let size = if e.idea.images.len() == 1 { ui.available_width().min(280.0) } else { 86.0 };
            for img in &e.idea.images {
                let image = ImageRef::input(img.clone());
                let t = tex.thumb(root, &image);
                let tip = format!("{}\ndouble-click: full size", tex.dims_text(root, &image));
                let r = thumb(ui, t.as_ref(), size, false, None).on_hover_text(tip);
                if r.double_clicked() {
                    out.push(Msg::Preview(Some(image.clone())));
                }
                r.context_menu(|ui| crate::widgets::image_menu_items(ui, root, &image, out));
            }
        });
    }
    if !e.idea.text.is_empty() {
        ui.add_space(4.0);
        ui.add(egui::Label::new(&e.idea.text).wrap());
    }
    if !e.idea.tags.is_empty() {
        ui.horizontal_wrapped(|ui| {
            ui.weak("tags");
            ui.label(e.idea.tags.join(", "));
        });
    }
    ui.add_space(4.0);
    section(ui, "idea_sec_idea", ic(ph::LIGHTBULB, "Idea"), true, |ui| {
        kv_grid(ui, "idea_kv", |ui| {
            kv(ui, "added", &crate::widgets::when(e.idea.created));
            let place = match &e.loc {
                IdeaLoc::Unreviewed => "Idea inbox".to_string(),
                IdeaLoc::Discarded => "Discarded".to_string(),
                IdeaLoc::Library(f) if f.as_os_str().is_empty() => "Input library".to_string(),
                IdeaLoc::Library(f) => format!("Input library › {}", f.display()),
            };
            kv(ui, "where", &place);
            ui.weak("folder");
            let dir =
                egui::Label::new(RichText::new(e.dir.display().to_string()).small()).truncate().sense(Sense::click());
            if ui.add(dir).on_hover_text(format!("{}\nclick: open in the file manager", e.dir.display())).clicked() {
                out.push(Msg::ShowInFileManager(e.dir.clone()));
            }
            ui.end_row();
            ui.weak("id");
            ui.horizontal(|ui| {
                ui.label(RichText::new(id.to_string()).monospace().small());
                if ui.small_button(ph::COPY).on_hover_text("copy the id").clicked() {
                    ui.ctx().copy_text(id.to_string());
                }
            });
            ui.end_row();
            let text = &e.idea.text;
            if !text.is_empty() {
                let words = text.split_whitespace().count();
                kv(ui, "text", &format!("{words} words · {} characters", text.chars().count()));
            }
            let tags = image_tags(text);
            if !tags.is_empty() {
                kv(ui, "refs", &tags.join(" "));
            }
        });
    });
    section(ui, "idea_sec_source", ic(ph::ARROW_SQUARE_IN, "Source"), true, |ui| {
        source_section(ui, st, root, tex, &e.idea.source, out)
    });
    if !e.idea.images.is_empty() {
        let missing: Vec<_> = e.idea.images.iter().filter(|i| !st.input_meta.contains_key(*i)).cloned().collect();
        if !missing.is_empty() {
            out.push(Msg::NeedInputMeta(missing));
        }
        let n = e.idea.images.len();
        let title = ic(ph::IMAGES, &format!("Images ({n})"));
        section(ui, ("idea_sec_images", n == 1), title, n == 1, |ui| {
            for (k, img) in e.idea.images.iter().enumerate() {
                if k > 0 {
                    ui.separator();
                }
                image_meta(ui, st, root, tex, img, k);
            }
        });
    }
    if !e.idea.variants.is_empty() {
        let title = ic(ph::LIST_BULLETS, &format!("Variants ({})", e.idea.variants.len()));
        section(ui, "idea_sec_variants", title, false, |ui| {
            for v in &e.idea.variants {
                ui.horizontal(|ui| {
                    let origin = match &v.origin {
                        increart_core::model::VariantOrigin::Manual => "by hand".to_string(),
                        increart_core::model::VariantOrigin::Rewritten { model, .. } => format!("rewritten by {model}"),
                    };
                    ui.weak(origin);
                    if ui.small_button(ph::COPY).on_hover_text("copy this variant").clicked() {
                        ui.ctx().copy_text(v.text.clone());
                    }
                });
                ui.add(egui::Label::new(&v.text).wrap());
                ui.add_space(4.0);
            }
        });
    }
    let usage = st.idea_usage(e).label();
    if usage.is_some() || !e.idea.projects.is_empty() {
        section(ui, "idea_sec_use", ic(ph::ROCKET_LAUNCH, "Use & projects"), true, |ui| {
            if let Some(u) = usage {
                ui.horizontal_wrapped(|ui| {
                    ui.weak("its images");
                    ui.label(u);
                });
            }
            if !e.idea.projects.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.weak("promoted into");
                    for p in &e.idea.projects {
                        let name = st.index.projects.iter().find(|x| &x.project.id == p).map(|x| x.name());
                        match name {
                            Some(name) => {
                                let r = ui.link(ic(ph::ROCKET_LAUNCH, &name)).on_hover_text("select it in Projects");
                                if r.clicked() {
                                    out.push(Msg::Tab(Tab::Projects));
                                    out.push(Msg::SelectProject(p.clone()));
                                }
                            }
                            None => {
                                ui.weak(format!("{p} (gone)"));
                            }
                        }
                    }
                });
            }
        });
    }
}

/// A side panel section, folded or not (remembered per `id`).
fn section(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    title: String,
    open: bool,
    body: impl FnOnce(&mut egui::Ui),
) {
    egui::CollapsingHeader::new(RichText::new(title).strong()).id_salt(id).default_open(open).show(ui, body);
}

fn kv_grid(ui: &mut egui::Ui, id: &str, rows: impl FnOnce(&mut egui::Ui)) {
    egui::Grid::new(id).num_columns(2).spacing([10.0, 3.0]).show(ui, rows);
}

/// A key (weak) and its value, wrapped, as a grid row.
fn kv(ui: &mut egui::Ui, key: &str, value: &str) {
    ui.weak(key);
    ui.add(egui::Label::new(value).wrap());
    ui.end_row();
}

/// The `<imageN>` tags in a prompt, each once, in order.
fn image_tags(text: &str) -> Vec<String> {
    let mut tags: Vec<String> = vec![];
    for (i, _) in text.match_indices("<image") {
        let rest = &text[i + 6..];
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if !digits.is_empty() && rest[digits.len()..].starts_with('>') {
            let tag = format!("<image{digits}>");
            if !tags.contains(&tag) {
                tags.push(tag);
            }
        }
    }
    tags
}

/// How the idea came: typed or pasted, the dropped file's path, or the
/// output(s) it was made from (a click goes to them).
fn source_section(
    ui: &mut egui::Ui,
    st: &AppState,
    root: &Path,
    tex: &mut Textures,
    source: &increart_core::model::IdeaSource,
    out: &mut Vec<Msg>,
) {
    use increart_core::model::IdeaSource;
    ui.label(IdeaOrigin::of(source).label());
    let outputs = match source {
        IdeaSource::DroppedFile { original_path } => {
            ui.horizontal(|ui| {
                ui.add(egui::Label::new(RichText::new(original_path).small()).wrap());
                if ui.small_button(ph::COPY).on_hover_text("copy the path").clicked() {
                    ui.ctx().copy_text(original_path.clone());
                }
            });
            return;
        }
        IdeaSource::FromOutput { image } => vec![image.clone()],
        IdeaSource::Tiled { images } => images.clone(),
        _ => return,
    };
    for image in outputs {
        ui.horizontal(|ui| {
            let t = tex.thumb(root, &image);
            let r = thumb(ui, t.as_ref(), 56.0, false, None).on_hover_text("double-click: full size");
            if r.double_clicked() {
                out.push(Msg::Preview(Some(image.clone())));
            }
            ui.vertical(|ui| match st.output_link(&image) {
                Some(link) => {
                    if let Some(p) = st.project_of_link(link) {
                        ui.label(ic(ph::ROCKET_LAUNCH, &p.name()));
                    }
                    let run = st.index.run(&link.name.run).map(|(_, r)| r);
                    let label = run.map(|r| r.label.clone()).filter(|l| !l.is_empty());
                    let run = label.unwrap_or_else(|| link.name.run.to_string());
                    ui.weak(format!("{run} · rev {} · seed {}", link.name.rev, link.name.seed));
                    let to = if link.verdict.is_none() { "Show in Review" } else { "Show in Output Library" };
                    if ui.link(to).clicked() {
                        out.push(Msg::GoToOutput(image.clone()));
                    }
                }
                None => {
                    ui.weak("the output is gone");
                }
            });
        });
    }
}

/// One idea image's facts: size, transparency, the file it was imported
/// from, its use and pixel hash (the file's metadata loads on first view).
fn image_meta(ui: &mut egui::Ui, st: &AppState, root: &Path, tex: &mut Textures, img: &increart_core::Id, k: usize) {
    let image = ImageRef::input(img.clone());
    kv_grid(ui, &format!("idea_img_kv{k}"), |ui| {
        let dims = tex.dims_text(root, &image);
        if !dims.is_empty() {
            kv(ui, "size", &dims);
        }
        kv(ui, "alpha", if st.index.transparent(&image) { "transparent" } else { "opaque" });
        kv(ui, "used", &st.usage(&image).label().unwrap_or_else(|| "not yet".into()));
        match st.input_meta.get(img) {
            None | Some(None) => {
                ui.weak("file");
                ui.spinner();
                ui.end_row();
            }
            Some(Some(m)) => {
                match &m.source {
                    Some(s) => {
                        kv(ui, "file", s.original_name.as_deref().unwrap_or("(clipboard)"));
                        if let Some(p) = &s.original_path {
                            kv(ui, "from", p);
                        }
                        let kb = s.original_size.div_ceil(1024);
                        let orig =
                            format!("{} · {} × {} · {kb} KB", s.original_format, s.original_width, s.original_height);
                        kv(ui, "original", &orig);
                        if let Some(t) = s.original_created {
                            kv(ui, "created", &crate::widgets::when(t));
                        }
                        if let Some(t) = s.original_mtime {
                            kv(ui, "modified", &crate::widgets::when(t));
                        }
                        if let Some(t) = s.imported {
                            kv(ui, "imported", &crate::widgets::when(t));
                        }
                        let conv =
                            if s.conversions.is_empty() { "unchanged".to_string() } else { s.conversions.join(", ") };
                        kv(ui, "on import", &conv);
                    }
                    None => kv(ui, "file", "no import record"),
                }
                if let Some(h) = &m.blake3 {
                    ui.weak("hash");
                    let short = h.get(..12).unwrap_or(h);
                    if ui
                        .add(egui::Label::new(RichText::new(short).monospace().small()).sense(Sense::click()))
                        .on_hover_text(format!("BLAKE3 of the pixels\n{h}\nclick: copy"))
                        .clicked()
                    {
                        ui.ctx().copy_text(h.clone());
                    }
                    ui.end_row();
                }
            }
        }
    });
}

fn lib_count(st: &AppState, folder: Option<&Path>) -> usize {
    st.index
        .ideas
        .iter()
        .filter(|e| match (&e.loc, folder) {
            (IdeaLoc::Library(_), None) => true,
            (IdeaLoc::Library(f), Some(sel)) => f.starts_with(sel),
            _ => false,
        })
        .count()
}

/// Ctrl+N from anywhere: the New idea form.
pub fn global_keys(ctx: &egui::Context, out: &mut Vec<Msg>) {
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::N)) {
        out.push(Msg::Tab(Tab::IdeaInbox));
        out.push(Msg::InboxTab(InboxTab::NewIdea));
    }
}
