//! The rewrite / describe window: the PE server's rewrite of the Generate
//! prompt or an idea's text, shown as a word diff against the original, or
//! its description of an image. The answer can be edited, asked for again
//! with instructions, used, kept as an idea's variant, or discarded.

use eframe::egui::{self, Color32, RichText, ScrollArea, text::LayoutJob};
use increart_core::pe::{Diff, lost_tags, word_diff};
use increart_gui_core::{AppState, Msg, PeJobState, PeKey, PeStatus, PeStep, PeTarget, PeUse};

use crate::textures::Textures;
use crate::widgets::{BAD, GOOD, ic, ph, thumb};

pub fn window(ctx: &egui::Context, st: &AppState, tex: &mut Textures, out: &mut Vec<Msg>) {
    let Some(t) = &st.pe.text else { return };
    let describe = matches!(t.target, PeTarget::Describe(_));
    let title = match &t.target {
        PeTarget::Prompt => "Rewrite the prompt",
        PeTarget::NewIdea | PeTarget::Idea(_) => "Rewrite the idea",
        PeTarget::Describe(_) => "Describe the image",
    };
    let server = st.pe.server.as_ref().map_or("PE", |s| s.name.as_str());
    let mut open = true;
    let (mut how, mut text) = (t.how.clone(), t.text.clone());
    egui::Window::new(ic(ph::MAGIC_WAND, title))
        .id(egui::Id::unique("pe_text"))
        .open(&mut open)
        .default_width(620.0)
        // clear of the Generate prompt, which it's often about
        .pivot(egui::Align2::CENTER_TOP)
        .default_pos(ctx.content_rect().center_top() + egui::vec2(160.0, 90.0))
        .resizable(true)
        .collapsible(false)
        .show(ctx, |ui| {
            if let (PeTarget::Describe(img), Some(root)) = (&t.target, st.library.as_deref()) {
                let th = tex.thumb(root, img);
                thumb(ui, th.as_ref(), 128.0, false, None);
            }
            ui.horizontal(|ui| {
                ui.label("Instructions");
                let hint = if describe {
                    "optional: e.g. focus on the lighting, as a watercolour"
                } else {
                    "optional: e.g. more cinematic, shorter, keep it 1920s"
                };
                let r = ui.add(egui::TextEdit::singleline(&mut how).hint_text(hint).desired_width(360.0));
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let again = if describe { "Describe again" } else { "Rewrite again" };
                let b = ui.add_enabled(t.waiting.is_none(), egui::Button::new(ic(ph::ARROW_CLOCKWISE, again)));
                if (b.clicked() || enter) && t.waiting.is_none() {
                    // the instructions go with the ask
                    out.push(Msg::PeTextEdit { how: how.clone(), text: text.clone() });
                    out.push(Msg::PeTextRetry);
                }
            });
            let job = t.waiting.and_then(|id| st.pe.job(id));
            // the answer as it's written
            let live = match job.map(|j| &j.step) {
                Some(PeStep::Writing { text, .. }) => Some(text.as_str()),
                _ => None,
            };
            if let Some(j) = job {
                ui.horizontal(|ui| {
                    ui.spinner();
                    if ui.button(ic(ph::STOP, "Stop")).on_hover_text("stop asking").clicked() {
                        out.push(Msg::PeStop(vec![j.id]));
                    }
                    ui.label(format!("{server}: {}", progress(st, j)));
                });
            } else if let Some(e) = &t.error {
                ui.colored_label(BAD, e);
            }
            if !describe {
                ui.add_space(4.0);
                ui.label(RichText::new("Changes").strong());
                ScrollArea::vertical().id_salt("pe_diff").max_height(160.0).show(ui, |ui| match live {
                    _ if !text.is_empty() => _ = ui.label(diff_job(ui, &t.original, &text)),
                    Some(live) => _ = ui.label(diff_job(ui, &t.original, live)),
                    None => _ = ui.weak(&t.original),
                });
            }
            if let (Some(live), true) = (live, text.is_empty()) {
                ui.add_space(4.0);
                ui.label(RichText::new(if describe { "Description" } else { "Rewritten" }).strong());
                ScrollArea::vertical().id_salt("pe_answer").max_height(220.0).stick_to_bottom(true).show(ui, |ui| {
                    let mut shown = live.to_string();
                    ui.add(
                        egui::TextEdit::multiline(&mut shown)
                            .interactive(false)
                            .desired_rows(5)
                            .desired_width(f32::INFINITY),
                    );
                });
            } else if !text.is_empty() || t.waiting.is_none() {
                ui.add_space(4.0);
                let label = if describe { "Description" } else { "Rewritten" };
                ui.horizontal(|ui| {
                    ui.label(RichText::new(label).strong());
                    if text != t.answer {
                        ui.weak("(edited)");
                        if ui.small_button(ic(ph::ARROW_COUNTER_CLOCKWISE, "As answered")).clicked() {
                            text = t.answer.clone();
                        }
                    }
                });
                ScrollArea::vertical().id_salt("pe_answer").max_height(220.0).show(ui, |ui| {
                    ui.add(egui::TextEdit::multiline(&mut text).desired_rows(5).desired_width(f32::INFINITY));
                });
                let lost = if text.is_empty() { vec![] } else { lost_tags(&t.original, &text) };
                if !lost.is_empty() {
                    ui.colored_label(BAD, format!("{} {} no longer in it", ph::WARNING, lost.join(", ")));
                }
            }
            ui.separator();
            ui.horizontal(|ui| {
                let ready = !text.trim().is_empty();
                let (use_label, use_hint) = match &t.target {
                    PeTarget::Prompt => ("Use as the prompt", "the prompt before goes in its history"),
                    PeTarget::Describe(_) => ("Use as the Generate prompt", "the prompt before goes in its history"),
                    _ => ("Use as the text", "the text before goes in its history"),
                };
                if ui
                    .add_enabled(ready, egui::Button::new(RichText::new(ic(ph::CHECK, use_label)).strong()))
                    .on_hover_text(use_hint)
                    .clicked()
                {
                    out.push(Msg::PeTextEdit { how: how.clone(), text: text.clone() });
                    out.push(Msg::PeTextClose(PeUse::Use));
                }
                if matches!(t.target, PeTarget::Idea(_))
                    && ui
                        .add_enabled(ready, egui::Button::new(ic(ph::STACK, "Keep as a variant")))
                        .on_hover_text("added to the idea's variants; its text stays as it is")
                        .clicked()
                {
                    out.push(Msg::PeTextEdit { how: how.clone(), text: text.clone() });
                    out.push(Msg::PeTextClose(PeUse::Variant));
                }
                if ui.add_enabled(ready, egui::Button::new(ic(ph::COPY, "Copy"))).clicked() {
                    ui.ctx().copy_text(text.clone());
                    out.push(Msg::Info("Copied to the clipboard".into()));
                }
                if ui.button("Discard").clicked() {
                    out.push(Msg::PeTextClose(PeUse::Discard));
                }
            });
        });
    if !open {
        out.push(Msg::PeTextClose(PeUse::Discard));
    } else if (how.as_str(), text.as_str()) != (t.how.as_str(), t.text.as_str()) {
        out.push(Msg::PeTextEdit { how, text });
    }
}

/// Where an ask is, with its time: "queued, 2 ahead", "reading the prompt
/// and image… 9 s (13 s last time)", "writing… ~84 tokens, 6.2/s".
/// What an ask is for: "values for {material}", "Duotone's prompt".
pub fn ask_name(st: &AppState, j: &PeJobState) -> String {
    match &j.key {
        PeKey::Blank(b) => match &b.style {
            Some(s) => format!("values for {{{}}} of {}", b.slot, s.name),
            None => format!("values for {{{}}}", b.slot),
        },
        PeKey::Style(k) => format!("{}'s prompt", k.name),
        PeKey::Text(_) => match st.pe.text.as_ref().map(|t| &t.target) {
            Some(PeTarget::Describe(_)) => "describing an image".into(),
            _ => "a rewrite".into(),
        },
    }
}

pub fn progress(st: &AppState, j: &PeJobState) -> String {
    let now = step_text(st, j);
    match &j.on {
        // several servers: which one
        Some(on) if st.pe.pool_size() > 1 && !matches!(j.step, PeStep::Queued) => format!("{now} on {on}"),
        _ => now,
    }
}

fn step_text(st: &AppState, j: &PeJobState) -> String {
    let secs = |since: f64| (st.now - since).max(0.0);
    let last = |t: Option<f64>| t.map_or(String::new(), |t| format!(" ({t:.0} s last time)"));
    match &j.step {
        PeStep::Queued if st.pe.status == PeStatus::Starting => starting(st),
        PeStep::Queued => match st.pe.ahead(j.id) {
            0 => "about to ask".into(),
            n => format!("queued, {n} ahead"),
        },
        PeStep::Preparing => "preparing the images…".into(),
        PeStep::Reading { images } => {
            let what = match images {
                0 => "the prompt",
                1 => "the prompt and image",
                _ => "the prompt and images",
            };
            format!("reading {what}… {:.0} s{}", secs(j.since), last(st.pe.read_times.get(images).copied()))
        }
        PeStep::Writing { pieces, .. } => {
            let rate = j.rate.map_or(String::new(), |(_, r)| format!(", {r:.1}/s"));
            format!("writing… ~{pieces} tokens{rate}")
        }
    }
}

/// The PE server's state in a few words, for the top bar while it starts
/// or works ("writing 143 tok (+1)"); `None` when it does neither. The
/// rest is in `progress` and `starting`.
pub fn brief(st: &AppState) -> Option<String> {
    let secs = |since: f64| (st.now - since).max(0.0);
    if st.pe.status == PeStatus::Starting {
        return Some(format!("loading {:.0} s", secs(st.pe.starting_since)));
    }
    let j = st.pe.working().filter(|_| st.pe.status == PeStatus::Ready)?;
    let now = match &j.step {
        PeStep::Queued => "queued".into(),
        PeStep::Preparing => "preparing".into(),
        PeStep::Reading { .. } => format!("reading {:.0} s", secs(j.since)),
        PeStep::Writing { pieces, .. } => format!("writing {pieces} tok"),
    };
    let more = st.pe.jobs.len() - 1;
    Some(if more > 0 { format!("{now} (+{more})") } else { now })
}

/// "starting… 23 s (loading the model; 58 s last time)".
pub fn starting(st: &AppState) -> String {
    let last = st.pe.last_load.map_or(String::new(), |t| format!("; {t:.0} s last time"));
    format!("starting… {:.0} s (loading the model{last})", (st.now - st.pe.starting_since).max(0.0))
}

/// `a` to `b` word by word: removed words struck through in red, added
/// ones green.
fn diff_job(ui: &egui::Ui, a: &str, b: &str) -> LayoutJob {
    let font = egui::TextStyle::Body.resolve(ui.style());
    let plain = ui.visuals().text_color();
    let mut job = LayoutJob::default();
    for (kind, w) in word_diff(a, b) {
        let mut fmt = egui::TextFormat { font_id: font.clone(), color: plain, ..Default::default() };
        match kind {
            Diff::Same => {}
            Diff::Added => {
                fmt.color = GOOD;
                fmt.background = Color32::from_rgba_unmultiplied(60, 160, 80, 40);
            }
            Diff::Removed => {
                fmt.color = BAD;
                fmt.strikethrough = egui::Stroke::new(1.0, BAD);
            }
        }
        job.append(w, 0.0, fmt);
    }
    job.wrap.max_width = ui.available_width();
    job
}

/// The Rewrite button for a text: enabled with a PE server set up, and
/// showing the window's state while it's for this text.
pub fn button(ui: &mut egui::Ui, st: &AppState, target: PeTarget, label: &str, out: &mut Vec<Msg>) {
    let Some(server) = st.pe.server.as_ref() else {
        ui.add_enabled(false, egui::Button::new(ic(ph::MAGIC_WAND, label)))
            .on_disabled_hover_text("Needs a prompt-enhance server: add one under pe in config.rsn");
        return;
    };
    let busy = st.pe.text.as_ref().is_some_and(|t| t.target == target && t.waiting.is_some());
    let label = if busy { format!("{label}…") } else { label.to_string() };
    let hint = if server.vision() {
        format!("Ask {} for a better prompt (it sees the images too), to look over before using", server.name)
    } else {
        format!("Ask {} for a better prompt, to look over before using", server.name)
    };
    if ui.button(ic(ph::MAGIC_WAND, &label)).on_hover_text(hint).clicked() {
        out.push(Msg::PeRewrite(target));
    }
}
