//! IncreArt: Qwen-Image 2.1 batch generation and review (docs/DESIGN.md).
//!
//! Settings: `$XDG_CONFIG_HOME/increart/config.rsn`, or the file named by
//! `INCREART_CONFIG`. The command line can skip the start-up screens (see
//! `USAGE`); a screen is only skipped when its argument is valid.

mod app;
mod folderview;
mod history;
mod ideas;
mod presets;
mod projects;
mod refpicker;
mod review;
mod settings;
mod startup;
mod textures;
mod tiles;
mod tour;
mod tree;
mod widgets;

use std::path::PathBuf;

use eframe::egui;
use increart_core::config::Config;
use increart_gui_core::StartupServers;

const USAGE: &str = "\
usage: increart [--library <folder>|last] [--servers <preset>|last | --no-servers]

  --library <folder>   open this library (skips the library screen)
  --library last       the most recently opened library
  --servers <preset>   open this server preset (skips the servers screen;
                       the main window opens while the servers start)
  --servers last       the preset opened last
  --no-servers         continue without servers

A screen is still shown when its argument doesn't work (not a library, no
such preset, sd-server missing), saying why.";

/// The command line's answers for the start-up screens.
#[derive(Default)]
pub struct Startup {
    pub library: Option<String>,
    pub servers: Option<StartupServers>,
}

fn parse_args() -> Result<Startup, String> {
    let mut s = Startup::default();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut value = |flag: &str| args.next().ok_or_else(|| format!("{flag} needs a value"));
        match a.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            "--library" => s.library = Some(value("--library")?),
            "--servers" => s.servers = Some(StartupServers::Preset(value("--servers")?)),
            "--no-servers" => s.servers = Some(StartupServers::Without),
            _ => return Err(format!("unknown argument: {a}")),
        }
    }
    Ok(s)
}

/// The smallest window: the top bar's tabs and buttons still fit (the
/// server statuses shrink, then hide).
pub const MIN_WINDOW: [f32; 2] = [1000.0, 640.0];

fn main() -> eframe::Result {
    let startup = parse_args().unwrap_or_else(|e| {
        eprintln!("increart: {e}\n\n{USAGE}");
        std::process::exit(2);
    });
    let config_path = std::env::var_os("INCREART_CONFIG").map(PathBuf::from).or_else(Config::default_path);
    let (config, warnings) = match &config_path {
        Some(p) => Config::load(p).unwrap_or_else(|e| {
            let mut c = Config::default();
            let w = c.normalize();
            (c, [vec![format!("settings not loaded ({e}); using defaults")], w].concat())
        }),
        None => {
            let mut c = Config::default();
            let w = c.normalize();
            (c, w)
        }
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size(MIN_WINDOW)
            .with_title("IncreArt"),
        ..Default::default()
    };
    eframe::run_native(
        "IncreArt",
        options,
        Box::new(move |cc| {
            add_icon_font(&cc.egui_ctx);
            Ok(Box::new(app::App::new(cc, config, config_path, warnings, startup)))
        }),
    )
}

/// Registers Phosphor as a fallback after the text font. egui-phosphor's own
/// `add_to_fonts` targets egui 0.36, so only its font bytes and constants are used.
fn add_icon_font(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert("phosphor".into(), egui::FontData::from_static(egui_phosphor::Variant::Regular.font_bytes()).into());
    if let Some(keys) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        keys.insert(1.min(keys.len()), "phosphor".into());
    }
    // The filled glyphs (same code points), for two-tone icons.
    fonts
        .font_data
        .insert("phosphor-fill".into(), egui::FontData::from_static(egui_phosphor::Variant::Fill.font_bytes()).into());
    fonts.families.insert(egui::FontFamily::Name(widgets::ICON_FILL.into()), vec!["phosphor-fill".into()]);
    ctx.set_fonts(fonts);
}
