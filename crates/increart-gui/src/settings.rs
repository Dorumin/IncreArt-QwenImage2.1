//! The Settings window (top bar ⚙) and the status bar's notification bell.

use eframe::egui::{self, RichText};
use increart_core::notify::{self, AlertRule, Notifications, Sound};
use increart_gui_core::{AppState, Msg};

use crate::widgets::{BAD, GOOD, ic, ph};

pub fn window(ctx: &egui::Context, st: &AppState, out: &mut Vec<Msg>) {
    if !st.settings_open {
        return;
    }
    let mut open = true;
    let mut n = st.config.notifications.clone();
    egui::Window::new(ic(ph::GEAR, "Settings")).open(&mut open).default_width(620.0).collapsible(false).show(
        ctx,
        |ui| {
            ui.horizontal(|ui| {
                let _ = ui.selectable_label(true, ic(ph::BELL, "Notifications"));
            });
            ui.separator();
            notifications(ui, st, &mut n, out);
        },
    );
    if n != st.config.notifications {
        out.push(Msg::SetNotifications(n));
    }
    if !open {
        out.push(Msg::Settings(false));
    }
}

fn notifications(ui: &mut egui::Ui, st: &AppState, n: &mut Notifications, out: &mut Vec<Msg>) {
    ui.label(RichText::new("GPU idle").strong());
    let gpus = if n.all_gpus { "every GPU" } else { "a GPU" };
    rule(ui, "idle", &mut n.gpu_idle, &format!("Sound when {gpus} has had no job for"), "min", out);
    ui.horizontal(|ui| {
        ui.add_space(24.0);
        ui.radio_value(&mut n.all_gpus, false, "any GPU idle");
        ui.radio_value(&mut n.all_gpus, true, "all GPUs idle");
    });
    ui.horizontal(|ui| {
        ui.add_space(24.0);
        ui.checkbox(&mut n.pe_counts_as_work, "Prompt enhance asks count as work").on_hover_text(
            "while prompt enhance is asking nothing counts as idle, and idle time counts again from its last ask",
        );
    });
    ui.horizontal(|ui| {
        ui.add_space(24.0);
        ui.weak("Counts only while servers are connected and ready (not while loading a model).");
    });
    ui.add_space(8.0);
    ui.label(RichText::new("GPU queue cleared").strong());
    ui.horizontal(|ui| {
        let what = if n.queue_cleared_all { "the last busy GPU" } else { "a GPU" };
        ui.checkbox(&mut n.queue_cleared.on, format!("Sound at once when {what} finishes its last job"));
        ui.add_enabled_ui(n.queue_cleared.on, |ui| {
            ui.label("· sound");
            sound_pick(ui, "cleared", &mut n.queue_cleared.sound);
            if ui.small_button(ph::PLAY).on_hover_text("play it").clicked() {
                out.push(Msg::TestSound(n.queue_cleared.sound.clone()));
            }
        });
    });
    ui.add_enabled_ui(n.queue_cleared.on, |ui| {
        if let Sound::File(f) = &mut n.queue_cleared.sound {
            ui.horizontal(|ui| {
                ui.add_space(24.0);
                ui.label("file");
                ui.add(egui::TextEdit::singleline(f).hint_text("/path/to/sound.wav").desired_width(380.0));
            });
        }
        ui.horizontal(|ui| {
            ui.add_space(24.0);
            ui.radio_value(&mut n.queue_cleared_all, false, "each GPU");
            ui.radio_value(&mut n.queue_cleared_all, true, "only when all GPUs are done");
        });
    });
    ui.add_space(8.0);
    ui.label(RichText::new("Review backlog").strong());
    rule(ui, "backlog", &mut n.review_backlog, "Sound when more images than", "wait for review", out);
    ui.horizontal(|ui| {
        ui.add_space(24.0);
        ui.weak(format!("now: {} (the Review tab's count)", st.unreviewed_outputs()));
    });
    ui.separator();

    ui.label(RichText::new("Sound").strong());
    ui.horizontal(|ui| {
        ui.label("Volume");
        let mut pct = (n.volume * 100.0).round() as u32;
        if ui.add(egui::Slider::new(&mut pct, 0..=100).suffix(" %")).changed() {
            n.volume = pct as f32 / 100.0;
        }
    });
    player(ui, n);
    ui.horizontal(|ui| {
        if ui.button(ic(ph::SPEAKER_HIGH, "Test")).on_hover_text("plays the GPU idle sound now").clicked() {
            out.push(Msg::TestSound(n.gpu_idle.sound.clone()));
        }
        match &st.alerts.test_result {
            Some(Ok(p)) => {
                ui.colored_label(GOOD, format!("played with {p}"));
            }
            Some(Err(e)) => {
                ui.colored_label(BAD, e);
            }
            None => {}
        }
    });
    ui.separator();
    ui.checkbox(&mut n.only_unfocused, "Only when IncreArt isn't the active window");
    ui.checkbox(&mut n.flash_taskbar, "Also ask for attention (taskbar flash)");
    ui.checkbox(&mut n.show_message, "Also say why in the status bar");
    ui.weak("Snooze from the bell in the status bar. Settings are saved in config.rsn.");
}

/// An alert's on/threshold/repeat/sound row.
fn rule(ui: &mut egui::Ui, id: &str, r: &mut AlertRule, what: &str, unit: &str, out: &mut Vec<Msg>) {
    ui.horizontal(|ui| {
        ui.checkbox(&mut r.on, what);
        ui.add_enabled(r.on, egui::DragValue::new(&mut r.threshold).range(1..=100_000));
        ui.label(unit);
    });
    ui.add_enabled_ui(r.on, |ui| {
        ui.horizontal(|ui| {
            ui.add_space(24.0);
            ui.label("repeat every");
            ui.add(egui::DragValue::new(&mut r.repeat_minutes).range(1..=1440));
            ui.label("min while it lasts · sound");
            sound_pick(ui, id, &mut r.sound);
            if ui.small_button(ph::PLAY).on_hover_text("play it").clicked() {
                out.push(Msg::TestSound(r.sound.clone()));
            }
        });
        if let Sound::File(f) = &mut r.sound {
            ui.horizontal(|ui| {
                ui.add_space(24.0);
                ui.label("file");
                ui.add(egui::TextEdit::singleline(f).hint_text("/path/to/sound.wav").desired_width(380.0));
            });
        }
    });
}

fn sound_pick(ui: &mut egui::Ui, id: &str, s: &mut Sound) {
    let label = if matches!(s, Sound::File(_)) { "file…".to_string() } else { s.name() };
    egui::ComboBox::from_id_salt(("sound", id)).selected_text(label).show_ui(ui, |ui| {
        for b in Sound::BUILT_IN {
            let name = b.name();
            ui.selectable_value(s, b, name);
        }
        if ui.selectable_label(matches!(s, Sound::File(_)), "a file of your own…").clicked()
            && !matches!(s, Sound::File(_))
        {
            *s = Sound::File(String::new());
        }
    });
}

/// The player: automatic for this OS, a ready-made command, or your own.
fn player(ui: &mut egui::Ui, n: &mut Notifications) {
    let preset = notify::PLAYERS.iter().find(|(_, c)| *c == n.player).map(|(name, _)| *name);
    let current = if n.player.is_empty() {
        "automatic".to_string()
    } else {
        preset.map_or("your own command".to_string(), String::from)
    };
    ui.horizontal(|ui| {
        ui.label("Player");
        egui::ComboBox::from_id_salt("player").selected_text(current).width(300.0).show_ui(ui, |ui| {
            if ui.selectable_label(n.player.is_empty(), "automatic").clicked() {
                n.player.clear();
            }
            for (name, cmd) in notify::PLAYERS {
                if ui.selectable_label(n.player == *cmd, *name).clicked() {
                    n.player = cmd.to_string();
                }
            }
            if ui.selectable_label(!n.player.is_empty() && preset.is_none(), "your own command…").clicked()
                && (n.player.is_empty() || preset.is_some())
            {
                n.player = format!("{} ", notify::auto_players().first().copied().unwrap_or("player {file}"));
            }
        });
    });
    if n.player.is_empty() {
        let tries: Vec<_> = notify::auto_players().iter().filter_map(|c| c.split_whitespace().next()).collect();
        ui.weak(format!("Tries {} on this system, the first one found.", tries.join(", ")));
    } else {
        ui.horizontal(|ui| {
            ui.label("Command");
            ui.add(egui::TextEdit::singleline(&mut n.player).font(egui::TextStyle::Monospace).desired_width(470.0));
        });
        if let Err(e) = notify::split_command(&n.player) {
            ui.colored_label(BAD, e);
        }
    }
    ui.weak(
        "Placeholders: {file}, {volume} (0.00–1.00), {volume_percent}, {volume_pa} (0–65536). \
         The built-in sounds are made at the chosen volume (their {volume} is 1.00).",
    );
}

/// The status bar's bell: alerts on, off, sounding or snoozed; click for
/// what holds, snooze and the settings.
pub fn bell(ui: &mut egui::Ui, st: &AppState, out: &mut Vec<Msg>) {
    let n = &st.config.notifications;
    if !n.gpu_idle.on && !n.queue_cleared.on && !n.review_backlog.on {
        if ui.small_button(ph::BELL_SLASH).on_hover_text("No notifications: click to set them up").clicked() {
            out.push(Msg::Settings(true));
        }
        return;
    }
    let a = &st.alerts;
    let holding: Vec<&String> = [&a.idle_text, &a.cleared_text, &a.backlog_text].into_iter().flatten().collect();
    let snoozed = a.snoozed(st.now);
    let (icon, color) = if a.sound_error.is_some() {
        (ph::WARNING, BAD)
    } else if snoozed {
        (ph::BELL_Z, ui.visuals().weak_text_color())
    } else if !holding.is_empty() {
        (ph::BELL_RINGING, crate::widgets::NEUTRAL)
    } else {
        (ph::BELL, ui.visuals().text_color())
    };
    let r = ui.add(egui::Button::new(RichText::new(icon).color(color)).small()).on_hover_text("Notifications");
    egui::Popup::menu(&r).show(|ui| {
        ui.set_min_width(260.0);
        if holding.is_empty() {
            ui.weak("Nothing to report.");
        }
        for h in &holding {
            ui.label(*h);
        }
        if let Some(e) = &a.sound_error {
            ui.colored_label(BAD, format!("sound: {e}"));
        }
        ui.separator();
        if snoozed {
            let until = if a.muted {
                "until IncreArt restarts".to_string()
            } else {
                format!("for {} more min", ((a.snooze_until.unwrap_or(0.0) - st.now) / 60.0).ceil())
            };
            ui.weak(format!("Snoozed {until}"));
            if ui.button(ic(ph::BELL, "Unsnooze")).clicked() {
                out.push(Msg::Unsnooze);
            }
        } else {
            for (label, secs) in [
                ("Snooze 5 min", Some(300.0)),
                ("Snooze 30 min", Some(1800.0)),
                ("Snooze 1 h", Some(3600.0)),
                ("Until restart", None),
            ] {
                if ui.button(ic(ph::BELL_Z, label)).clicked() {
                    out.push(Msg::Snooze(secs));
                }
            }
        }
        ui.separator();
        if ui.button(ic(ph::GEAR, "Notification settings…")).clicked() {
            out.push(Msg::Settings(true));
        }
    });
}
