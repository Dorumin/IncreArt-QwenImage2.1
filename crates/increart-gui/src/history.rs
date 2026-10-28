//! The history window: entries newest first with the runs they were
//! queued as, a word diff of the selected one against the current text and
//! its settings against the current form, Restore / Copy.

use eframe::egui::{self, Color32, Key, RichText, ScrollArea, TextFormat, text::LayoutJob};
use increart_core::history::{HistoryEntry, HistoryKind};
use increart_core::{HistoryKey, Id};
use increart_gui_core::{AppState, HistoryView, Msg, OutputSource, Restore, settings_changes};

use crate::widgets::{BAD, GOOD, ic, ph, short};

fn kind_label(k: &HistoryKind) -> (String, Color32) {
    match k {
        HistoryKind::Edit => ("edit".into(), Color32::GRAY),
        HistoryKind::Queued(l) => (format!("queued {l}"), GOOD),
        HistoryKind::BeforeClear => ("before clear".into(), Color32::from_rgb(200, 170, 70)),
        HistoryKind::BeforeRestore => ("before restore".into(), Color32::from_rgb(200, 170, 70)),
        HistoryKind::BeforeRetag => ("before tag update".into(), Color32::from_rgb(200, 170, 70)),
        HistoryKind::BeforePreset => ("before preset".into(), Color32::from_rgb(200, 170, 70)),
        HistoryKind::Saved => ("saved".into(), GOOD),
        HistoryKind::BeforeRewrite => ("before rewrite".into(), Color32::from_rgb(200, 170, 70)),
    }
}

/// A row's kinds, e.g. "edit, queued r01, r02".
fn kinds_text(kinds: &[&HistoryKind]) -> String {
    let mut parts: Vec<String> = vec![];
    let mut queued: Vec<&str> = vec![];
    for k in kinds {
        match k {
            HistoryKind::Queued(l) => queued.push(l),
            k => parts.push(kind_label(k).0),
        }
    }
    if !queued.is_empty() {
        parts.push(format!("queued {}", queued.join(", ")));
    }
    parts.join(", ")
}

fn ago(e: &HistoryEntry) -> String {
    let secs = (increart_core::id::now() - e.time).num_seconds().max(0);
    match secs {
        0..60 => format!("{secs} s ago"),
        60..3600 => format!("{} min ago", secs / 60),
        3600..86_400 => format!("{} h ago", secs / 3600),
        _ => e.time.format("%Y-%m-%d %H:%M").to_string(),
    }
}

/// The current text (and negative / title) that history entries are compared with.
fn current(st: &AppState, key: &HistoryKey) -> (String, String) {
    match key {
        HistoryKey::Project(_) => (st.generate.prompt.clone(), st.generate.negative.clone()),
        HistoryKey::NewIdea => (st.draft.text.clone(), st.draft.title.clone()),
        HistoryKey::Idea(_) => st.idea_edit.as_ref().map(|e| (e.text.clone(), e.title.clone())).unwrap_or_default(),
    }
}

/// Words removed from `now` struck through in red, added ones in green.
fn diff_job(now: &str, then: &str, ui: &egui::Ui) -> LayoutJob {
    use similar::{ChangeTag, TextDiff};
    let font = egui::TextStyle::Body.resolve(ui.style());
    let normal = ui.visuals().text_color();
    let mut job = LayoutJob::default();
    for c in TextDiff::from_words(now, then).iter_all_changes() {
        let mut f = TextFormat { font_id: font.clone(), color: normal, ..Default::default() };
        match c.tag() {
            ChangeTag::Equal => {}
            ChangeTag::Insert => {
                f.color = GOOD;
                f.background = Color32::from_rgba_unmultiplied(90, 190, 100, 40);
            }
            ChangeTag::Delete => {
                f.color = BAD;
                f.strikethrough = egui::Stroke::new(1.0, BAD);
            }
        }
        job.append(c.value(), 0.0, f);
    }
    job.wrap.max_width = ui.available_width();
    job
}

/// A run's images and how many are reviewed.
fn run_counts(st: &AppState, run: &Id) -> (usize, usize) {
    let Some((p, _)) = st.index.run(run) else { return (0, 0) };
    let mine: Vec<_> = p.outputs.iter().filter(|o| &o.name.run == run).collect();
    (mine.len(), mine.iter().filter(|o| o.verdict.is_some()).count())
}

/// A run's label now, or `None` once it's deleted.
fn run_label(st: &AppState, run: &Id) -> Option<String> {
    st.index.run(run).map(|(_, r)| r.label.clone())
}

/// Chips for the runs an entry was queued as: click for the run's details,
/// the images button for its outputs. A sweep's runs are one chip, opening
/// its contact sheet.
fn run_chips(ui: &mut egui::Ui, st: &AppState, runs: &[Id], out: &mut Vec<Msg>) {
    let found: Vec<_> = runs.iter().filter_map(|id| st.index.run(id)).collect();
    let sweep = found.first().and_then(|(p, r)| r.sweep.as_ref().map(|s| (p.project.id.clone(), s.name.clone())));
    if let Some((project, name)) = sweep.filter(|_| found.len() > 1) {
        let (first, last) = (&found[0].1.label, &found[found.len() - 1].1.label);
        let r = ui
            .button(RichText::new(format!("{first} – {last} · sweep {name}")).color(GOOD))
            .on_hover_text(format!("{} runs · open the sweep's contact sheet", found.len()));
        if r.clicked() {
            out.push(Msg::ReviewSweep(project, name));
        }
    } else {
        for (p, run) in &found {
            let (n, reviewed) = run_counts(st, &run.id);
            let r = ui.button(RichText::new(ic(ph::QUEUE, &run.label)).color(GOOD)).on_hover_text(format!(
                "{} · {}\n{}\n{n} images, {reviewed} reviewed · click for details",
                p.name(),
                run.model.recipe,
                short(&run.prompt, 80)
            ));
            if r.clicked() {
                out.push(Msg::JobInfo(Some(run.id.clone())));
            }
            if ui.small_button(ph::IMAGES).on_hover_text("its images in the Output library").clicked() {
                out.push(Msg::ShowOutputs(OutputSource::Run(run.id.clone())));
            }
        }
    }
    let gone = runs.len() - found.len();
    if gone > 0 {
        let t = if gone == 1 { "deleted run".to_string() } else { format!("{gone} deleted runs") };
        ui.label(RichText::new(t).small().strikethrough().weak());
    }
}

/// "since r04": the run an entry followed.
fn since(st: &AppState, after: Option<&Id>) -> Option<String> {
    let a = after?;
    Some(format!("since {}", run_label(st, a).unwrap_or_else(|| "a deleted run".into())))
}

pub fn window(ctx: &egui::Context, st: &AppState, v: &HistoryView, out: &mut Vec<Msg>) {
    let project = matches!(v.key, HistoryKey::Project(_));
    let title = match &v.key {
        HistoryKey::Project(_) => "Generate history",
        HistoryKey::NewIdea => "New idea history",
        HistoryKey::Idea(_) => "Idea text history",
    };
    let extra_name = if project { "negative" } else { "title" };
    let mut open = true;
    egui::Window::new(ic(ph::CLOCK_COUNTER_CLOCKWISE, title))
        .open(&mut open)
        .default_size([980.0, 600.0])
        .collapsible(false)
        .show(ctx, |ui| {
            let shown = v.shown();
            // ↑/↓ browse, Enter restores
            let pos = shown.iter().position(|r| r.indexes.contains(&v.selected));
            ui.input(|i| {
                if i.key_pressed(Key::ArrowDown)
                    && let Some(r) = shown.get(pos.map_or(0, |p| p + 1))
                {
                    out.push(Msg::HistorySelect(r.index));
                }
                if i.key_pressed(Key::ArrowUp)
                    && let Some(r) = pos.and_then(|p| p.checked_sub(1)).and_then(|p| shown.get(p))
                {
                    out.push(Msg::HistorySelect(r.index));
                }
                if i.key_pressed(Key::Enter) {
                    out.push(Msg::HistoryRestore(Restore::All));
                }
            });
            ui.horizontal_wrapped(|ui| {
                let mut q = v.search.clone();
                ui.add(egui::TextEdit::singleline(&mut q).hint_text("search").desired_width(200.0));
                if q != v.search {
                    out.push(Msg::HistorySearch(q));
                }
                let mut only = v.checkpoints_only;
                if ui.checkbox(&mut only, "checkpoints only").changed() {
                    out.push(Msg::HistoryCheckpointsOnly(only));
                }
                if project {
                    let mut settings = v.settings_only;
                    if ui
                        .checkbox(&mut settings, "settings changes")
                        .on_hover_text("entries whose settings differ from the entry before")
                        .changed()
                    {
                        out.push(Msg::HistorySettingsOnly(settings));
                    }
                    run_filter(ui, st, v, out);
                }
                let mut every = v.every_entry;
                if ui
                    .checkbox(&mut every, "every entry")
                    .on_hover_text("one row per entry, instead of one per run of identical entries")
                    .changed()
                {
                    out.push(Msg::HistoryEveryEntry(every));
                }
                ui.weak(format!("{} rows, {} entries (up to 1000 kept)", shown.len(), v.entries.len()));
            });
            if let Some((run, before)) = &v.run_filter {
                ui.horizontal(|ui| {
                    let label = run_label(st, run).unwrap_or_else(|| "a deleted run".into());
                    let from = since(st, before.as_ref()).unwrap_or_else(|| "from the start".into());
                    ui.label(RichText::new(format!("What led to {label}: {from}, up to its queuing")).strong());
                    if ui.small_button(ic(ph::X, "show all")).clicked() {
                        out.push(Msg::HistoryRunFilter(None));
                    }
                });
            }
            ui.separator();
            ui.columns(2, |cols| {
                ScrollArea::vertical().id_salt("hist_list").max_height(470.0).show(&mut cols[0], |ui| {
                    if v.entries.is_empty() {
                        ui.weak("No history yet: entries are saved 2 s after you stop editing.");
                    }
                    for row in &shown {
                        let e = row.entry;
                        let mut kinds = kinds_text(&row.kinds);
                        if row.indexes.len() > 1 {
                            kinds = format!("{kinds} · {}×", row.indexes.len());
                        }
                        let mut runs: Vec<Id> = vec![];
                        for i in row.indexes.iter().rev() {
                            runs.extend(v.runs_of(*i).iter().cloned());
                        }
                        if project
                            && runs.is_empty()
                            && let Some(s) = since(st, v.after.get(row.index).and_then(Option::as_ref))
                        {
                            kinds = format!("{kinds} · {s}");
                        }
                        let mut text = format!("{}   {}\n{}", ago(e), kinds, short(&e.text, 60));
                        if project {
                            let changes = v.changes(row.index);
                            if e.form.is_none() {
                                text.push_str("\nprompt only");
                            } else if !changes.is_empty() {
                                text.push_str(&format!("\n{}", changes.join(" · ")));
                            }
                        }
                        let r = ui.selectable_label(row.indexes.contains(&v.selected), RichText::new(text));
                        if r.clicked() {
                            out.push(Msg::HistorySelect(row.index));
                        }
                        if r.double_clicked() {
                            out.push(Msg::HistorySelect(row.index));
                            out.push(Msg::HistoryRestore(Restore::All));
                        }
                        if !runs.is_empty() {
                            ui.horizontal_wrapped(|ui| {
                                ui.add_space(12.0);
                                run_chips(ui, st, &runs, out);
                            });
                        }
                    }
                });
                let ui = &mut cols[1];
                let Some(e) = v.entries.get(v.selected) else { return };
                let (now, now_extra) = current(st, &v.key);
                let (k, c) = kind_label(&e.kind);
                ui.horizontal(|ui| {
                    ui.colored_label(c, k);
                    ui.weak(e.time.format("%Y-%m-%d %H:%M:%S UTC").to_string());
                    ui.weak(format!("{} chars", e.text.chars().count()));
                });
                if project {
                    let runs = v.runs_of(v.selected);
                    ui.horizontal_wrapped(|ui| {
                        if runs.is_empty() {
                            ui.weak(since(st, v.after.get(v.selected).and_then(Option::as_ref)).unwrap_or_default());
                        } else {
                            let (n, reviewed) =
                                runs.iter().map(|r| run_counts(st, r)).fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
                            ui.label(format!("queued as · {n} images, {reviewed} reviewed:"));
                            run_chips(ui, st, runs, out);
                        }
                    });
                }
                ui.weak("Compared with the current text: added, removed");
                ScrollArea::vertical().id_salt("hist_diff").max_height(260.0).show(ui, |ui| {
                    let job = diff_job(&now, &e.text, ui);
                    ui.label(job);
                    if !e.extra.is_empty() || !now_extra.is_empty() {
                        ui.add_space(8.0);
                        ui.weak(extra_name);
                        let job = diff_job(&now_extra, &e.extra, ui);
                        ui.label(job);
                    }
                });
                if project {
                    ui.separator();
                    match &e.form {
                        Some(d) => {
                            let changes = settings_changes(&st.generate.to_draft(&st.config), d);
                            if changes.is_empty() {
                                ui.weak("Settings: the same as the current form");
                            } else {
                                ui.weak("Settings compared with the current form:");
                                ui.label(changes.join(" · "));
                            }
                            ui.weak(format!(
                                "{}×{} · {} · {} steps · cfg {} · {} seeds · {} image(s)",
                                d.width,
                                d.height,
                                d.preset,
                                d.steps,
                                d.cfg,
                                d.seeds,
                                d.images.len().max(d.refs.len())
                            ));
                        }
                        None => {
                            ui.weak("This entry has only the prompt (saved before settings were kept).");
                        }
                    }
                }
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    let main = if project && e.form.is_some() { "Restore all" } else { "Restore" };
                    let r = ui.button(RichText::new(ic(ph::ARROW_COUNTER_CLOCKWISE, main)).strong());
                    let r = if project {
                        r.on_hover_text("the prompt and every setting; the label and first seed stay the next run's")
                    } else {
                        r
                    };
                    if r.clicked() {
                        out.push(Msg::HistoryRestore(if project { Restore::All } else { Restore::Text }));
                    }
                    if project && ui.button("Restore prompt only").clicked() {
                        out.push(Msg::HistoryRestore(Restore::Text));
                    }
                    let both =
                        if project { "Restore prompt + negative".to_string() } else { "Restore with title".into() };
                    if ui.button(both).clicked() {
                        out.push(Msg::HistoryRestore(Restore::TextAndExtra));
                    }
                    if ui.button(ic(ph::COPY, "Copy")).clicked() {
                        ui.ctx().copy_text(e.text.clone());
                    }
                });
                ui.weak("The current form is saved to history first, so a restore can be undone here.");
            });
        });
    if !open {
        out.push(Msg::CloseHistory);
    }
}

/// "led to": only the entries between a run and the run before it.
fn run_filter(ui: &mut egui::Ui, st: &AppState, v: &HistoryView, out: &mut Vec<Msg>) {
    let HistoryKey::Project(p) = &v.key else { return };
    let Some(p) = st.index.project(p) else { return };
    let mut runs: Vec<_> = p.runs.iter().collect();
    runs.sort_by_key(|r| std::cmp::Reverse((r.created, r.id.clone())));
    let current = v.run_filter.as_ref().map(|(r, _)| r);
    let text = current.and_then(|r| run_label(st, r)).map_or("led to: any run".into(), |l| format!("led to: {l}"));
    egui::ComboBox::from_id_salt("hist_run").selected_text(text).height(400.0).show_ui(ui, |ui| {
        if ui.selectable_label(current.is_none(), "any run").clicked() {
            out.push(Msg::HistoryRunFilter(None));
        }
        for r in runs {
            let label = match &r.sweep {
                Some(s) => format!("{} · sweep {}", r.label, s.name),
                None => format!("{} · {}", r.label, short(&r.prompt, 40)),
            };
            if ui.selectable_label(current == Some(&r.id), label).clicked() {
                out.push(Msg::HistoryRunFilter(Some(r.id.clone())));
            }
        }
    });
}
