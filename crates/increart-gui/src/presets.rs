//! Prompt presets in Generate: the Presets window (pick one), Edit
//! presets… and Save as preset… (DESIGN.md, Prompt presets).

use eframe::egui::{self, ComboBox, RichText, ScrollArea};
use increart_core::prompts::{PresetKey, Sweep};
use increart_core::prompts::{PromptPreset, PromptPresets, PromptTask};
use increart_gui_core::{AppState, GenerateForm, Msg, PresetsEditor, PresetsView, SavePresetDialog};

use crate::widgets::{BAD, ICON_FILL, ic, ph, short};

/// The Generate prompt's text field.
pub fn prompt_id() -> egui::Id {
    egui::Id::unique("gen_prompt")
}

/// How many reference images a prompt needs: its highest `<imageN>`.
fn needs_refs(prompt: &str) -> usize {
    increart_gui_core::image_tags(prompt).into_iter().map(|(_, n)| n).max().unwrap_or(0)
}

fn matches(p: &PromptPreset, q: &str) -> bool {
    [&p.name, &p.description, &p.prompt, &p.group].iter().any(|s| s.to_lowercase().contains(q))
}

/// The Presets window, beside Generate: tasks on the left, their presets on
/// the right. Click replaces the prompt, Shift+click inserts at its cursor.
pub fn window(ctx: &egui::Context, st: &AppState, out: &mut Vec<Msg>) {
    let Some(panel) = &st.presets_panel else { return };
    let list = &st.prompt_presets;
    let refs = st.generate.refs().len();
    // in sweep mode a click ticks the preset instead of using it
    let style_sweep = st.generate.sweep.on && st.generate.sweep.vary == increart_core::prompts::SweepVary::Style;
    let sweep = style_sweep.then_some(st.generate.sweep.selected.as_slice());
    // Ticked only in a style sweep
    let view = match &panel.view {
        PresetsView::Ticked(_) if sweep.is_none() => &PresetsView::Favourites,
        v => v,
    };
    let mut open = true;
    egui::Window::new(ic(ph::SWATCHES, "Prompt presets"))
        .id(egui::Id::unique("prompt_presets_window"))
        .open(&mut open)
        // over the images pane, so the prompt stays in view
        .pivot(egui::Align2::RIGHT_TOP)
        .default_pos(ctx.content_rect().right_top() + egui::vec2(-16.0, 56.0))
        .default_width(720.0)
        .default_height((ctx.content_rect().height() - 130.0).clamp(380.0, 690.0))
        .min_height(300.0)
        .resizable(true)
        .collapsible(false)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                let mut q = panel.search.clone();
                ui.label(ph::MAGNIFYING_GLASS);
                let r = ui.add(
                    egui::TextEdit::singleline(&mut q)
                        .hint_text("Search names, descriptions and prompts")
                        .desired_width(320.0),
                );
                if r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    q.clear();
                }
                if !q.is_empty() && ui.small_button(ph::X).on_hover_text("clear the search").clicked() {
                    q.clear();
                }
                if q != panel.search {
                    out.push(Msg::PresetsSearch(q));
                }
                ui.weak(format!("{} presets in {} tasks", list.count(), list.tasks.len()));
            });
            if let Some(ticked) = sweep {
                ui.horizontal(|ui| {
                    ui.weak("Style sweep: click to tick or untick a preset  ·  ★: favourite");
                    let q = panel.search.to_lowercase();
                    let all = shown(list, view, ticked, &q);
                    let usable: Vec<_> = all
                        .iter()
                        .copied()
                        .filter(|&(t, p)| needs_refs(&list.tasks[t].presets[p].prompt) <= refs)
                        .collect();
                    let skipped = all.len() - usable.len();
                    // the Ticked view only unticks: Clear all ticks does it
                    let ticked_view = matches!(view, PresetsView::Ticked(_)) && q.is_empty();
                    let tip = match skipped {
                        0 => "tick every preset listed here".to_string(),
                        n => format!("tick every preset listed here ({n} need more reference images and are left out)"),
                    };
                    if !ticked_view
                        && ui
                            .add_enabled(
                                !usable.is_empty(),
                                egui::Button::new(ic(ph::CHECK_SQUARE, "Tick all")).small(),
                            )
                            .on_hover_text(tip)
                            .on_disabled_hover_text(format!(
                                "every preset listed here needs more reference images than Generate has ({refs})"
                            ))
                            .clicked()
                    {
                        out.push(Msg::SweepTicks(usable, true));
                    }
                    if !ticked_view
                        && ui
                            .add_enabled(!all.is_empty(), egui::Button::new(ic(ph::SQUARE, "Untick all")).small())
                            .on_hover_text("untick every preset listed here")
                            .clicked()
                    {
                        out.push(Msg::SweepTicks(all, false));
                    }
                    let clear = egui::Button::new(ic(ph::X, &format!("Clear all ticks ({})", ticked.len()))).small();
                    if ui
                        .add_enabled(!ticked.is_empty(), clear)
                        .on_hover_text("untick every preset, in every task")
                        .clicked()
                    {
                        out.push(Msg::SweepClearTicks);
                    }
                });
            } else {
                ui.weak("Click: replace the prompt  ·  Shift+click: insert at the prompt's cursor  ·  ★: favourite");
            }
            ui.separator();
            // the lists take what the (resizable) window leaves above the buttons
            let height = (ui.available_height() - 40.0).max(120.0);
            let row = egui::Layout::left_to_right(egui::Align::Min);
            // bounded: a vertical separator takes all the height it is given
            ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), height), row, |ui| {
                let down = egui::Layout::top_down(egui::Align::Min);
                ui.allocate_ui_with_layout(egui::vec2(200.0, height), down, |ui| {
                    ScrollArea::vertical().id_salt("preset_tasks").max_height(height).auto_shrink([false, false]).show(
                        ui,
                        |ui| {
                            ui.set_width(190.0);
                            task_list(ui, list, view, sweep, panel.search.is_empty(), out)
                        },
                    );
                });
                ui.separator();
                ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), height), down, |ui| {
                    ScrollArea::vertical()
                        .id_salt("preset_list")
                        .max_height(height)
                        .auto_shrink([false, false])
                        .show(ui, |ui| preset_list(ui, list, view, &panel.search.to_lowercase(), refs, sweep, out));
                });
            });
            ui.separator();
            ui.horizontal(|ui| {
                let at = view.task().unwrap_or(0);
                if ui.button(ic(ph::PENCIL_SIMPLE, "Edit presets…")).clicked() {
                    out.push(Msg::OpenPresetEditor(at, None));
                }
                if ui
                    .button(ic(ph::FLOPPY_DISK, "Save prompt as preset…"))
                    .on_hover_text("the Generate prompt (and negative) as a new preset")
                    .clicked()
                {
                    out.push(Msg::OpenSavePreset);
                }
            });
        });
    if !open {
        out.push(Msg::PresetsPanel(false));
    }
}

/// Ticked presets of task `t` (by name).
fn ticked_in(t: &PromptTask, ticks: &[PresetKey]) -> usize {
    t.presets.iter().filter(|p| ticks.iter().any(|k| k.task == t.name && k.name == p.name)).count()
}

fn task_list(
    ui: &mut egui::Ui,
    list: &PromptPresets,
    view: &PresetsView,
    ticks: Option<&[PresetKey]>,
    enabled: bool,
    out: &mut Vec<Msg>,
) {
    ui.add_enabled_ui(enabled, |ui| {
        let favs = list.tasks.iter().flat_map(|t| &t.presets).filter(|p| p.favourite).count();
        let fav = RichText::new(format!("{}  Favourites  ({favs})", ph::STAR));
        if ui.selectable_label(*view == PresetsView::Favourites, fav).clicked() {
            out.push(Msg::PresetsShow(PresetsView::Favourites));
        }
        if let Some(ticks) = ticks {
            let label = format!("{}  Ticked  ({})", ph::CHECK_SQUARE, ticks.len());
            let on = matches!(view, PresetsView::Ticked(_));
            let r = ui.selectable_label(on, label).on_hover_text("every preset ticked for the sweep, in every task");
            if r.clicked() && !on {
                out.push(Msg::PresetsShow(PresetsView::Ticked(vec![])));
            }
        }
        ui.add_space(4.0);
        for (i, t) in list.tasks.iter().enumerate() {
            let n = t.presets.len();
            let label = match ticks.map(|k| ticked_in(t, k)) {
                Some(k) if k > 0 => format!("{}  ({k} / {n})", t.name),
                _ => format!("{}  ({n})", t.name),
            };
            let r = ui.selectable_label(*view == PresetsView::Task(i), label);
            let r = if t.description.is_empty() { r } else { r.on_hover_text(&t.description) };
            if r.clicked() {
                out.push(Msg::PresetsShow(PresetsView::Task(i)));
            }
        }
    });
    if !enabled {
        ui.add_space(6.0);
        ui.weak("Searching every task.");
    }
}

/// The Ticked view's presets: those listed on choosing it, then any ticked
/// since.
fn ticked_keys(view: &PresetsView, ticks: &[PresetKey]) -> Vec<PresetKey> {
    let PresetsView::Ticked(listed) = view else { return vec![] };
    let mut v = listed.clone();
    v.extend(ticks.iter().filter(|k| !listed.contains(k)).cloned());
    v
}

/// Where a preset is in the list, if it still is.
fn find(list: &PromptPresets, k: &PresetKey) -> Option<(usize, usize)> {
    let ti = list.tasks.iter().position(|t| t.name == k.task)?;
    Some((ti, list.tasks[ti].presets.iter().position(|p| p.name == k.name)?))
}

fn preset_list(
    ui: &mut egui::Ui,
    list: &PromptPresets,
    view: &PresetsView,
    q: &str,
    refs: usize,
    ticks: Option<&[PresetKey]>,
    out: &mut Vec<Msg>,
) {
    if let (PresetsView::Ticked(_), true, Some(ticks)) = (view, q.is_empty(), ticks) {
        return ticked_list(ui, list, view, refs, ticks, out);
    }
    let task = view.task();
    // (task, preset) shown, under (task or group) headings
    let mut count = 0;
    let mut heading = String::new();
    let list_of = shown(list, view, ticks.unwrap_or_default(), q);
    for (ti, t) in list.tasks.iter().enumerate() {
        let in_view = if !q.is_empty() { true } else { task.is_none_or(|x| x == ti) };
        if !in_view {
            continue;
        }
        if q.is_empty() && task.is_some() && !t.description.is_empty() {
            ui.weak(&t.description);
            ui.add_space(4.0);
        }
        for (pi, p) in t.presets.iter().enumerate() {
            if !list_of.contains(&(ti, pi)) {
                continue;
            }
            let h = match (task, q.is_empty()) {
                (Some(_), true) => p.group.clone(),
                _ => t.name.clone(),
            };
            if h != heading {
                ui.add_space(6.0);
                // a group's ticks (in a task's view)
                let k = match (ticks, task.is_some() && q.is_empty()) {
                    (Some(k), true) => t
                        .presets
                        .iter()
                        .filter(|x| x.group == h && k.iter().any(|k| k.task == t.name && k.name == x.name))
                        .count(),
                    _ => 0,
                };
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&h).strong().size(15.0));
                    if k > 0 {
                        ui.weak(format!("·  {k} ticked"));
                    }
                });
                heading = h;
            }
            let ticked = ticks.map(|k| k.iter().any(|k| k.task == t.name && k.name == p.name));
            preset_row(ui, ti, pi, p, refs, ticked, out);
            count += 1;
        }
    }
    if count == 0 {
        ui.add_space(8.0);
        ui.weak(match (q.is_empty(), task) {
            (false, _) => "No preset matches.",
            (true, None) => "No favourites yet: click ☆ next to a preset to add it here.",
            (true, Some(_)) => "No presets in this task: add some with Edit presets….",
        });
    }
}

/// The presets (task, preset) the window lists: those matching the search
/// `q` (lower case), else the view's (Ticked: those still there).
fn shown(list: &PromptPresets, view: &PresetsView, ticks: &[PresetKey], q: &str) -> Vec<(usize, usize)> {
    if let (PresetsView::Ticked(_), true) = (view, q.is_empty()) {
        let mut v: Vec<_> = ticked_keys(view, ticks).iter().filter_map(|k| find(list, k)).collect();
        v.sort();
        return v;
    }
    let task = view.task();
    let mut v = vec![];
    for (ti, t) in list.tasks.iter().enumerate() {
        if q.is_empty() && task.is_some_and(|x| x != ti) {
            continue;
        }
        for (pi, p) in t.presets.iter().enumerate() {
            let keep = if !q.is_empty() { matches(p, q) } else { task.is_some() || p.favourite };
            if keep {
                v.push((ti, pi));
            }
        }
    }
    v
}

/// The Ticked view: its presets under their tasks' headings, in the list's
/// order; then those no longer there (renamed or deleted), which the sweep
/// skips, to untick.
fn ticked_list(
    ui: &mut egui::Ui,
    list: &PromptPresets,
    view: &PresetsView,
    refs: usize,
    ticks: &[PresetKey],
    out: &mut Vec<Msg>,
) {
    let keys = ticked_keys(view, ticks);
    let is_ticked = |k: &PresetKey| ticks.contains(k);
    let mut heading = "";
    for (ti, pi) in shown(list, view, ticks, "") {
        let t = &list.tasks[ti];
        if t.name != heading {
            ui.add_space(6.0);
            ui.label(RichText::new(&t.name).strong().size(15.0));
            heading = &t.name;
        }
        let k = PresetKey { task: t.name.clone(), name: t.presets[pi].name.clone() };
        preset_row(ui, ti, pi, &t.presets[pi], refs, Some(is_ticked(&k)), out);
    }
    let missing: Vec<_> = keys.iter().filter(|k| find(list, k).is_none()).collect();
    if !missing.is_empty() {
        ui.add_space(6.0);
        ui.label(RichText::new("No longer there").strong().size(15.0));
        ui.weak("Renamed or deleted since they were ticked: the sweep skips them.");
        for k in missing {
            ui.horizontal(|ui| {
                let mut on = is_ticked(k);
                if ui.checkbox(&mut on, "").on_hover_text("in the sweep (skipped)").clicked() {
                    out.push(Msg::SweepToggleKey(k.clone()));
                }
                ui.label(RichText::new(&k.name).weak().strikethrough());
                ui.weak(format!("({})", k.task));
            });
        }
    }
    if keys.is_empty() {
        ui.add_space(8.0);
        ui.weak("Nothing ticked: click presets in the tasks to add them to the sweep.");
    }
}

/// `ticked` is set in sweep mode: whether the sweep has this preset.
fn preset_row(
    ui: &mut egui::Ui,
    task: usize,
    preset: usize,
    p: &PromptPreset,
    refs: usize,
    ticked: Option<bool>,
    out: &mut Vec<Msg>,
) {
    let need = needs_refs(&p.prompt);
    let ok = need <= refs;
    ui.horizontal_top(|ui| {
        let star = if p.favourite {
            RichText::new(ph::STAR).family(egui::FontFamily::Name(ICON_FILL.into())).color(crate::widgets::ACCENT)
        } else {
            RichText::new(ph::STAR).weak()
        };
        let tip = if p.favourite { "remove from the favourites" } else { "add to the favourites" };
        if ui.add(egui::Button::new(star).frame(false)).on_hover_text(tip).clicked() {
            out.push(Msg::PresetFavourite(task, preset));
        }
        if let Some(mut on) = ticked
            && ui.checkbox(&mut on, "").on_hover_text("in the sweep").clicked()
        {
            out.push(Msg::SweepToggle(task, preset));
        }
        ui.vertical(|ui| {
            let r = ui.add_enabled(ok, egui::Button::selectable(ticked == Some(true), RichText::new(&p.name).strong()));
            let mut tip = p.prompt.clone();
            if !p.negative.is_empty() {
                tip.push_str(&format!("\n\nNegative: {}", p.negative));
            }
            let r = r
                .on_hover_text(&tip)
                .on_disabled_hover_text(format!("Needs {need} reference image(s); Generate has {refs}.\n\n{tip}"));
            if r.clicked() && ticked.is_some() {
                out.push(Msg::SweepToggle(task, preset));
            } else if r.clicked() {
                let shift = ui.input(|i| i.modifiers.shift);
                let insert_at = shift.then(|| cursor(ui.ctx())).flatten();
                out.push(Msg::ApplyPreset { task, preset, insert_at: insert_at.or(shift.then_some(usize::MAX)) });
            }
            if !p.description.is_empty() {
                ui.add(egui::Label::new(RichText::new(&p.description).weak()).wrap());
            }
        });
    });
}

/// Where the cursor is in the Generate prompt (characters), if it was ever
/// focused.
fn cursor(ctx: &egui::Context) -> Option<usize> {
    let s = egui::TextEdit::load_state(ctx, prompt_id())?;
    s.cursor.char_range().map(|r| r.primary.index.0)
}

/// The Generate prompt's selection (characters, start ≤ end; empty at the
/// cursor), if it was ever focused.
pub fn selection(ctx: &egui::Context) -> Option<std::ops::Range<usize>> {
    let s = egui::TextEdit::load_state(ctx, prompt_id())?;
    s.cursor.char_range().map(|r| {
        let (a, b) = (r.primary.index.0, r.secondary.index.0);
        a.min(b)..a.max(b)
    })
}

/// Puts the Generate prompt's cursor at character `at` and focuses it.
pub fn focus_prompt_at(ctx: &egui::Context, at: usize) {
    let id = prompt_id();
    let mut s = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
    s.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(at))));
    s.store(ctx, id);
    ctx.memory_mut(|m| m.request_focus(id));
}

/// Selects `st.prompt_select` in the prompt field (once).
pub fn select_in_prompt(ui: &egui::Ui, st: &AppState, resp: &egui::Response, out: &mut Vec<Msg>) {
    let Some(r) = &st.prompt_select else { return };
    let id = prompt_id();
    let mut s = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
    s.cursor.set_char_range(Some(egui::text::CCursorRange::two(
        egui::text::CCursor::new(r.start),
        egui::text::CCursor::new(r.end),
    )));
    s.store(ui.ctx(), id);
    resp.request_focus();
    out.push(Msg::PromptSelected);
}

/// **Edit presets…**: tasks, their presets, and the selected preset's fields,
/// all on a working copy saved or dropped as a whole.
pub fn editor_window(ctx: &egui::Context, st: &AppState, out: &mut Vec<Msg>) {
    let Some(e0) = &st.presets_editor else { return };
    let mut e = e0.clone();
    let m = egui::Modal::new(egui::Id::unique("presets_editor")).show(ctx, |ui| {
        ui.set_width(980.0);
        ui.heading(ic(ph::PENCIL_SIMPLE, "Edit prompt presets"));
        ui.weak(
            "Shared by every library; yours are files in prompt-presets next to config.rsn. Write <image1>, <image2>… for reference images and {name} for blanks to fill in.",
        );
        ui.add_space(6.0);
        let height = 440.0;
        let down = egui::Layout::top_down(egui::Align::Min);
        let row = egui::Layout::left_to_right(egui::Align::Min);
        // bounded: a vertical separator takes all the height it is given
        ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), height), row, |ui| {
            ui.allocate_ui_with_layout(egui::vec2(220.0, height), down, |ui| {
                ui.set_width(220.0);
                tasks_column(ui, &mut e);
            });
            ui.separator();
            ui.allocate_ui_with_layout(egui::vec2(260.0, height), down, |ui| {
                ui.set_width(260.0);
                presets_column(ui, &mut e);
            });
            ui.separator();
            ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), height), down, |ui| fields_column(ui, &mut e));
        });
        ui.separator();
        if let Some(err) = &e.error {
            ui.colored_label(BAD, err);
        }
        let (save, restore) = egui::Sides::new().show(
            ui,
            |ui| {
                let save = ui.button(RichText::new(ic(ph::CHECK, "Save")).strong()).clicked();
                if ui.button("Cancel   Esc").clicked() {
                    out.push(Msg::ClosePresetEditor);
                }
                save
            },
            |ui| {
                ui.button(ic(ph::ARROW_COUNTER_CLOCKWISE, "Restore defaults"))
                    .on_hover_text("the built-in presets in place of all of these (Cancel still undoes it)")
                    .clicked()
            },
        );
        if save {
            out.push(Msg::SavePresetEditor);
        }
        if restore {
            out.push(Msg::RestorePresetDefaults);
        }
    });
    if &e != e0 {
        e.error = None;
        // the edit lands before a Save sent in the same frame
        out.insert(0, Msg::EditPresetEditor(e));
    }
    if m.should_close() {
        out.push(Msg::ClosePresetEditor);
    }
}

/// A delete button for item `i`, if any is selected; true when clicked.
/// (No moving: the built-in order is kept, the user's presets sorted in.)
fn delete_button(ui: &mut egui::Ui, i: Option<usize>, what: &str) -> bool {
    i.is_some() && ui.button(ph::TRASH).on_hover_text(format!("delete this {what}")).clicked()
}

fn tasks_column(ui: &mut egui::Ui, e: &mut PresetsEditor) {
    ui.label(RichText::new("Tasks").strong());
    ScrollArea::vertical().id_salt("edit_tasks").max_height(250.0).auto_shrink([false, true]).show(ui, |ui| {
        for (i, t) in e.list.tasks.iter().enumerate() {
            if ui.selectable_label(i == e.task, format!("{}  ({})", t.name, t.presets.len())).clicked() {
                e.task = i;
                e.preset = None;
            }
        }
    });
    ui.horizontal(|ui| {
        if ui.button(ph::PLUS).on_hover_text("new task").clicked() {
            let name = unique_name(e.list.tasks.iter().map(|t| t.name.as_str()), "New task");
            e.list.tasks.push(PromptTask { name, ..Default::default() });
            e.task = e.list.tasks.len() - 1;
            e.preset = None;
        }
        let n = e.list.tasks.len();
        if delete_button(ui, (e.task < n).then_some(e.task), "task and its presets") {
            e.list.tasks.remove(e.task);
            e.task = e.task.min(e.list.tasks.len().saturating_sub(1));
            e.preset = None;
        }
    });
    if let Some(t) = e.list.tasks.get_mut(e.task) {
        ui.add_space(6.0);
        ui.label("Task name");
        ui.add(egui::TextEdit::singleline(&mut t.name).desired_width(f32::INFINITY));
        ui.label("Description");
        ui.add(egui::TextEdit::multiline(&mut t.description).desired_rows(2).desired_width(f32::INFINITY));
    }
}

fn presets_column(ui: &mut egui::Ui, e: &mut PresetsEditor) {
    let Some(t) = e.list.tasks.get_mut(e.task) else {
        ui.weak("No tasks: add one with +.");
        return;
    };
    ui.label(RichText::new(format!("Presets in {}", t.name)).strong());
    ScrollArea::vertical().id_salt("edit_presets").max_height(390.0).auto_shrink([false, false]).show(ui, |ui| {
        let mut group = String::new();
        for (i, p) in t.presets.iter().enumerate() {
            if p.group != group {
                ui.add_space(4.0);
                ui.weak(&p.group);
                group = p.group.clone();
            }
            if ui.selectable_label(e.preset == Some(i), &p.name).clicked() {
                e.preset = Some(i);
            }
        }
    });
    ui.horizontal(|ui| {
        if ui.button(ph::PLUS).on_hover_text("new preset").clicked() {
            let name = unique_name(t.presets.iter().map(|p| p.name.as_str()), "New preset");
            // in the selected one's group, after it
            let at = e.preset.map_or(t.presets.len(), |i| i + 1);
            let group = e.preset.and_then(|i| t.presets.get(i)).map(|p| p.group.clone()).unwrap_or_default();
            t.presets.insert(at, PromptPreset { name, group, prompt: "<image1>".into(), ..Default::default() });
            e.preset = Some(at);
        }
        if let Some(i) = e.preset.filter(|i| *i < t.presets.len())
            && ui.button(ph::COPY).on_hover_text("duplicate this preset").clicked()
        {
            let mut p = t.presets[i].clone();
            p.name = unique_name(t.presets.iter().map(|p| p.name.as_str()), &format!("{} copy", p.name));
            p.favourite = false;
            t.presets.insert(i + 1, p);
            e.preset = Some(i + 1);
        }
        let n = t.presets.len();
        if let Some(i) = e.preset.filter(|i| *i < n)
            && delete_button(ui, Some(i), "preset")
        {
            t.presets.remove(i);
            e.preset = (!t.presets.is_empty()).then(|| i.min(t.presets.len() - 1));
        }
    });
}

fn fields_column(ui: &mut egui::Ui, e: &mut PresetsEditor) {
    let names: Vec<String> = e.list.tasks.iter().map(|t| t.name.clone()).collect();
    let Some(t) = e.list.tasks.get_mut(e.task) else { return };
    let groups: Vec<String> = {
        let mut g: Vec<String> = t.presets.iter().map(|p| p.group.clone()).filter(|g| !g.is_empty()).collect();
        g.dedup();
        g
    };
    let Some(i) = e.preset.filter(|i| *i < t.presets.len()) else {
        ui.weak("Select a preset to edit it.");
        return;
    };
    let mut move_to = None;
    let p = &mut t.presets[i];
    ui.set_width(ui.available_width());
    egui::Grid::new("preset_fields").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        ui.label("Name");
        ui.add(egui::TextEdit::singleline(&mut p.name).desired_width(f32::INFINITY));
        ui.end_row();
        ui.label("Group");
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut p.group).hint_text("none").desired_width(200.0));
            if !groups.is_empty() {
                ComboBox::from_id_salt("preset_group").selected_text("existing…").show_ui(ui, |ui| {
                    for g in &groups {
                        if ui.selectable_label(*g == p.group, g).clicked() {
                            p.group = g.clone();
                        }
                    }
                });
            }
        });
        ui.end_row();
        ui.label("Task");
        ComboBox::from_id_salt("preset_task").selected_text(&names[e.task]).show_ui(ui, |ui| {
            for (j, n) in names.iter().enumerate() {
                if ui.selectable_label(j == e.task, n).clicked() && j != e.task {
                    move_to = Some(j);
                }
            }
        });
        ui.end_row();
    });
    ui.label("Description");
    ui.add(
        egui::TextEdit::multiline(&mut p.description)
            .desired_rows(2)
            .desired_width(f32::INFINITY)
            .hint_text("what the name means, loosely, and what you get"),
    );
    ui.label("Prompt");
    ui.add(egui::TextEdit::multiline(&mut p.prompt).desired_rows(6).desired_width(f32::INFINITY));
    let slots = increart_core::prompts::slots(&p.prompt);
    let need = needs_refs(&p.prompt);
    ui.weak(format!(
        "{} reference image(s){}",
        need,
        if slots.is_empty() {
            String::new()
        } else {
            let names: Vec<&str> = slots.iter().map(|r| &p.prompt[r.clone()]).collect();
            format!(" · to fill in: {}", names.join(", "))
        }
    ));
    ui.label("Negative prompt");
    ui.add(egui::TextEdit::singleline(&mut p.negative).desired_width(f32::INFINITY).hint_text("optional"));
    if let Some(j) = move_to {
        let p = t.presets.remove(i);
        e.list.tasks[j].presets.push(p);
        e.task = j;
        e.preset = Some(e.list.tasks[j].presets.len() - 1);
    }
}

/// `base`, or `base 2`, `base 3`… if taken.
fn unique_name<'a>(taken: impl Iterator<Item = &'a str> + Clone, base: &str) -> String {
    let free = |n: &str| !taken.clone().any(|t| t.eq_ignore_ascii_case(n));
    if free(base) {
        return base.to_string();
    }
    (2..).map(|i| format!("{base} {i}")).find(|n| free(n)).unwrap()
}

/// **Save as preset…**: name the Generate prompt and pick its task.
pub fn save_window(ctx: &egui::Context, st: &AppState, out: &mut Vec<Msg>) {
    let Some(d0) = &st.save_preset else { return };
    let mut d: SavePresetDialog = d0.clone();
    let tasks = &st.prompt_presets.tasks;
    let m = egui::Modal::new(egui::Id::unique("save_preset")).show(ctx, |ui| {
        ui.set_width(520.0);
        ui.heading(ic(ph::FLOPPY_DISK, "Save as preset"));
        ui.weak(short(&d.preset.prompt, 160));
        ui.add_space(6.0);
        let mut enter = false;
        egui::Grid::new("save_preset_fields").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            ui.label("Task");
            let current = tasks.get(d.task).map_or("?", |t| t.name.as_str());
            ComboBox::from_id_salt("save_preset_task").selected_text(current).show_ui(ui, |ui| {
                for (i, t) in tasks.iter().enumerate() {
                    ui.selectable_value(&mut d.task, i, &t.name);
                }
            });
            ui.end_row();
            ui.label("Name");
            let r = ui.add(egui::TextEdit::singleline(&mut d.preset.name).desired_width(f32::INFINITY));
            crate::widgets::focus_until_done(&r);
            enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            ui.end_row();
            ui.label("Group");
            ui.add(egui::TextEdit::singleline(&mut d.preset.group).hint_text("none").desired_width(f32::INFINITY));
            ui.end_row();
        });
        ui.label("Description");
        ui.add(
            egui::TextEdit::multiline(&mut d.preset.description)
                .desired_rows(2)
                .desired_width(f32::INFINITY)
                .hint_text("what the name means, loosely, and what you get"),
        );
        if let Some(e) = &d.error {
            ui.colored_label(BAD, e);
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.button(RichText::new(ic(ph::CHECK, "Save   Enter")).strong()).clicked() || enter {
                out.push(Msg::ConfirmSavePreset);
            }
            if ui.button("Cancel   Esc").clicked() {
                out.push(Msg::CancelSavePreset);
            }
        });
    });
    if &d != d0 {
        d.error = None;
        // the edit lands before a Save sent in the same frame
        out.insert(0, Msg::EditSavePreset(d));
    }
    if m.should_close() {
        out.push(Msg::CancelSavePreset);
    }
}

/// Sweep mode's settings under the template: where the styles come from,
/// all or a random few, the `{slot}` values, and what Queue will make.
/// The Generate form's sweep: what it varies, then the styles (a prompt
/// preset per run) or the setting's values (DESIGN.md, Sweeps).
pub fn sweep_panel(ui: &mut egui::Ui, st: &AppState, f: &mut GenerateForm, out: &mut Vec<Msg>) {
    use increart_core::prompts::SweepVary;
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(ic(ph::SQUARES_FOUR, "Sweep")).strong());
            ui.separator();
            ui.label("Vary");
            for v in SweepVary::EVERY {
                let tip = match v {
                    SweepVary::Style => "a run per chosen prompt preset",
                    SweepVary::Strength if f.init().is_none() => "needs a start image",
                    SweepVary::Model => "a run per model preset, each with its own steps and CFG",
                    _ => "a run per value, the rest as the form has it",
                };
                let ok = v != SweepVary::Strength || f.init().is_some();
                let r = ui.add_enabled(ok, egui::Button::selectable(f.sweep.vary == v, v.name()));
                if r.on_hover_text(tip).on_disabled_hover_text(tip).clicked() && f.sweep.vary != v {
                    f.sweep.vary = v;
                    f.sweep.values = increart_gui_core::update::sweep_defaults(v, f);
                    if v == SweepVary::Model && f.sweep.models.is_empty() {
                        f.sweep.models = st.config.recipes.get(f.preset).map(|p| p.name.clone()).into_iter().collect();
                    }
                }
            }
        });
        if f.sweep.vary == SweepVary::Style {
            style_sweep(ui, st, f, out);
        } else {
            setting_sweep(ui, st, f);
        }
    });
}

/// A setting sweep's values (a model sweep's model presets), the runs
/// they make and how long they take.
fn setting_sweep(ui: &mut egui::Ui, st: &AppState, f: &mut GenerateForm) {
    use increart_core::prompts::SweepVary;
    use increart_gui_core::update::{param_runs, range_values};
    if f.sweep.vary == SweepVary::Model {
        ui.horizontal_wrapped(|ui| {
            ui.label("Models");
            for p in &st.config.recipes {
                let mut on = f.sweep.models.contains(&p.name);
                let tip = format!("{} steps · cfg {}", p.steps, p.cfg);
                if ui.checkbox(&mut on, &p.name).on_hover_text(tip).changed() {
                    if on {
                        f.sweep.models.push(p.name.clone());
                    } else {
                        f.sweep.models.retain(|m| *m != p.name);
                    }
                }
            }
        });
        ui.weak("Each model runs with its own preset's steps, CFG and flow shift.");
    } else {
        ui.horizontal(|ui| {
            ui.label("Values");
            ui.add(
                egui::TextEdit::singleline(&mut f.sweep.values).hint_text("e.g. 8, 12, 20, 30").desired_width(260.0),
            );
            // from … to … by …, kept for the session
            let id = egui::Id::unique(("sweep_range", f.sweep.vary.key()));
            let mut r: [f32; 3] = ui.data(|d| d.get_temp(id)).unwrap_or(match f.sweep.vary {
                SweepVary::Steps => [10.0, 40.0, 10.0],
                SweepVary::Cfg => [1.0, 5.0, 1.0],
                SweepVary::FlowShift => [1.0, 6.0, 1.0],
                _ => [0.3, 0.9, 0.15],
            });
            ui.separator();
            ui.label("from");
            ui.add(egui::DragValue::new(&mut r[0]).speed(0.1).max_decimals(2));
            ui.label("to");
            ui.add(egui::DragValue::new(&mut r[1]).speed(0.1).max_decimals(2));
            ui.label("by");
            ui.add(egui::DragValue::new(&mut r[2]).speed(0.05).range(0.01..=1000.0).max_decimals(2));
            if ui.button("Fill").on_hover_text("replace the values with this range").clicked() {
                f.sweep.values = range_values(r[0], r[1], r[2]);
            }
            ui.data_mut(|d| d.insert_temp(id, r));
        });
    }
    match param_runs(f, &st.config.recipes) {
        Err(e) => {
            ui.colored_label(BAD, e);
        }
        Ok(runs) => {
            let secs: f32 = runs.iter().map(|(r, _)| crate::projects::estimate(r) * r.seeds as f32).sum();
            let images = runs.len() as u64 * f.seeds;
            let line = format!(
                "{} {} × {} seeds = {images} images ≈ {} on one R9700 · same seeds for every value",
                runs.len(),
                if runs.len() == 1 { "value" } else { "values" },
                f.seeds,
                crate::projects::fmt_secs(secs)
            );
            if images > increart_gui_core::update::BIG_SWEEP {
                ui.colored_label(crate::widgets::NEUTRAL, format!("{line} (Queue asks first)"));
            } else {
                ui.label(line);
            }
            let rows: Vec<String> = runs.iter().filter_map(|(_, t)| t.param.as_ref().map(|p| p.label())).collect();
            ui.add(egui::Label::new(RichText::new(rows.join(" · ")).weak()).wrap());
            if f.sweep.vary == SweepVary::Cfg && runs.iter().any(|(r, _)| r.cfg <= 1.0) {
                ui.colored_label(
                    crate::widgets::NEUTRAL,
                    "At CFG 1 or below the negative prompt isn't used (and each step is about twice as fast).",
                );
            }
        }
    }
}

fn style_sweep(ui: &mut egui::Ui, st: &AppState, f: &mut GenerateForm, out: &mut Vec<Msg>) {
    use increart_core::prompts::{Removed, SweepSource, needing_slot, slot_names, sweep_presets};
    let list = &st.prompt_presets;
    let per = crate::projects::estimate(f);
    let seeds = f.seeds;
    let refs = f.images.iter().filter(|i| i.reference).count();
    let prompt = f.prompt.clone();
    let sw = &mut f.sweep;
    let base = sw.clone();
    {
        let count = |src: SweepSource| {
            sweep_presets(list, &Sweep { source: src, random: 0, removed: vec![], ..base.clone() }).len()
        };
        let name = |src: &SweepSource| match src {
            SweepSource::Selected => format!("Ticked in Presets ({})", base.selected.len()),
            SweepSource::Favourites => format!("★ Favourites ({})", count(SweepSource::Favourites)),
            SweepSource::Task(t) => format!("{t} ({})", count(src.clone())),
            SweepSource::Group(t, g) => format!("{t} → {g} ({})", count(src.clone())),
        };
        ui.horizontal(|ui| {
            ui.label("Styles from");
            let mut src = sw.source.clone();
            ComboBox::from_id_salt("sweep_source").selected_text(name(&src)).width(300.0).show_ui(ui, |ui| {
                for s in [SweepSource::Selected, SweepSource::Favourites] {
                    let n = name(&s);
                    ui.selectable_value(&mut src, s, n);
                }
                for t in &list.tasks {
                    let s = SweepSource::Task(t.name.clone());
                    let n = name(&s);
                    ui.selectable_value(&mut src, s, n);
                    let mut groups: Vec<&str> =
                        t.presets.iter().map(|p| p.group.as_str()).filter(|g| !g.is_empty()).collect();
                    groups.dedup();
                    for g in groups {
                        let s = SweepSource::Group(t.name.clone(), g.to_string());
                        let n = format!("      {g} ({})", count(s.clone()));
                        ui.selectable_value(&mut src, s, n);
                    }
                }
            });
            if src != sw.source {
                sw.removed.clear();
            }
            sw.source = src;
            if sw.source == SweepSource::Selected
                && ui
                    .button(ic(ph::SWATCHES, "Tick in Presets…"))
                    .on_hover_text("tick presets in the Presets window")
                    .clicked()
            {
                out.push(Msg::PresetsPanel(true));
            }
        });
        ui.horizontal(|ui| {
            ui.label("Use");
            let mut random = sw.random > 0;
            ui.radio_value(&mut random, false, "all of them");
            ui.radio_value(&mut random, true, "a random");
            let mut k = sw.random.max(1);
            ui.add_enabled(random, egui::DragValue::new(&mut k).range(1..=999));
            ui.label("of them");
            sw.random = if random { k } else { 0 };
            if random && ui.button(ic(ph::SHUFFLE, "Reroll")).on_hover_text("pick another random set").clicked() {
                sw.random_seed = (ui.input(|i| i.time) * 1e6) as u64 ^ sw.random_seed.rotate_left(17);
                sw.removed.clear();
            }
        });
        let chosen = sweep_presets(list, sw);
        // a preset not in the pick for a replaced one to take its place
        let spare = sw.random > 0 && sweep_presets(list, &Sweep { random: 0, ..sw.clone() }).len() > chosen.len();
        let mut remove: Option<Removed> = None;
        // the slots of the template and the chosen presets, asked once each,
        // with what needs them
        let names = slot_names(std::iter::once(prompt.as_str()).chain(chosen.iter().map(|(_, p)| p.prompt.as_str())));
        if !names.is_empty() {
            egui::Grid::new("sweep_slots").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                for n in &names {
                    ui.label(format!("{{{n}}}"));
                    let v = sw.slots.entry(n.clone()).or_default();
                    ui.add(egui::TextEdit::singleline(v).desired_width(320.0).hint_text("fill in for every run"));
                    ui.end_row();
                    ui.label("");
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        if slot_names([prompt.as_str()]).contains(n) {
                            ui.label(RichText::new("needed by the template;").weak());
                        }
                        let need = needing_slot(&chosen, n);
                        if !need.is_empty() {
                            ui.label(RichText::new("needed by").weak());
                        }
                        for (t, p) in need {
                            ui.label(RichText::new(&p.name).weak().italics()).on_hover_text(&p.prompt);
                            let key = PresetKey { task: t.name.clone(), name: p.name.clone() };
                            if spare
                                && ui
                                    .small_button(ic(ph::SHUFFLE, "Replace"))
                                    .on_hover_text(format!("replace {} with another random style", p.name))
                                    .clicked()
                            {
                                remove = Some(Removed { preset: key.clone(), replace: true });
                            }
                            if ui
                                .small_button(ic(ph::X, "Omit"))
                                .on_hover_text(format!("leave {} out of this sweep", p.name))
                                .clicked()
                            {
                                remove = Some(Removed { preset: key, replace: false });
                            }
                        }
                    });
                    ui.end_row();
                }
            });
        }
        if !sw.removed.is_empty() {
            ui.horizontal_wrapped(|ui| {
                let (rep, om): (Vec<_>, Vec<_>) = sw.removed.iter().partition(|r| r.replace);
                let list = |v: &[&Removed]| v.iter().map(|r| r.preset.name.as_str()).collect::<Vec<_>>().join(", ");
                let mut parts = vec![];
                if !rep.is_empty() {
                    parts.push(format!("replaced: {}", list(&rep)));
                }
                if !om.is_empty() {
                    parts.push(format!("left out: {}", list(&om)));
                }
                ui.label(RichText::new(parts.join(" · ")).weak());
                if ui.small_button(ic(ph::ARROW_COUNTER_CLOCKWISE, "Put back")).clicked() {
                    sw.removed.clear();
                }
            });
        }
        if let Some(r) = remove {
            sw.removed.retain(|x| x.preset != r.preset);
            sw.removed.push(r);
            ui.ctx().request_repaint();
        }
        sw.slots.retain(|k, v| names.contains(k) || !v.is_empty());
        let (fit, unfit): (Vec<_>, Vec<_>) = chosen.iter().partition(|(_, p)| needs_refs(&p.prompt) <= refs);
        let names: Vec<&str> = fit.iter().map(|(_, p)| p.name.as_str()).collect();
        if names.is_empty() {
            ui.colored_label(BAD, "No styles chosen.");
        } else {
            let images = names.len() as u64 * seeds;
            let secs = per * images as f32;
            let line = format!(
                "{} styles × {} seeds = {images} images ≈ {} on one R9700 · same seeds in every style",
                names.len(),
                seeds,
                crate::projects::fmt_secs(secs)
            );
            if images > increart_gui_core::update::BIG_SWEEP {
                ui.colored_label(crate::widgets::NEUTRAL, format!("{line} (Queue asks first)"));
            } else {
                ui.label(line);
            }
            ui.add(egui::Label::new(RichText::new(names.join(" · ")).weak()).wrap());
        }
        if !unfit.is_empty() {
            ui.colored_label(
                crate::widgets::NEUTRAL,
                format!("{} chosen preset(s) left out: they need more than {refs} reference image(s).", unfit.len()),
            );
        }
    }
}
