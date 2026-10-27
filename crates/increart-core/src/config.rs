//! The user's `config.rsn` (DESIGN.md, Config file).

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
    /// How the job queue shares the servers (Jobs: spread evenly / one at
    /// a time).
    #[serde(skip_serializing_if = "is_default")]
    pub job_order: crate::scheduler::Order,
    /// Sounds when a GPU is idle or the review queue is long.
    #[serde(skip_serializing_if = "is_default")]
    pub notifications: crate::notify::Notifications,
    /// The contact sheet's cell size in points (0: the default).
    #[serde(skip_serializing_if = "is_default")]
    pub sheet_cell: u32,
    /// The interface zoom in percent (0: 100%).
    #[serde(skip_serializing_if = "is_default")]
    pub ui_zoom: u32,
    /// The Tile dialog's settings, as last used.
    #[serde(skip_serializing_if = "is_default")]
    pub tile: TileSettings,
    /// The project tree doesn't list the selected project's runs.
    #[serde(skip_serializing_if = "is_default")]
    pub hide_project_runs: bool,
    /// The project tree and grid list projects by name (A–Z), instead of
    /// newest first.
    #[serde(skip_serializing_if = "is_default")]
    pub projects_by_name: bool,
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

/// A named set of servers (e.g. "Both GPUs", "Remote box").
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename = "ServerPreset")]
pub struct ServerPreset {
    pub name: String,
    pub servers: Vec<ServerConfig>,
}

/// One sd-server of a preset: started by the app, or a running one to
/// connect to.
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
}

impl ServerConfig {
    pub fn name(&self) -> &str {
        match self {
            ServerConfig::Start { name, .. } | ServerConfig::Connect { name, .. } => name,
        }
    }

    pub fn device(&self) -> &str {
        match self {
            ServerConfig::Start { device, .. } | ServerConfig::Connect { device, .. } => device,
        }
    }

    pub fn base_url(&self) -> String {
        match self {
            ServerConfig::Start { host, port, .. } => format!("http://{host}:{port}"),
            ServerConfig::Connect { url, .. } => url.trim_end_matches('/').to_string(),
        }
    }
}

impl Config {
    pub fn preset(&self, name: &str) -> Option<&ServerPreset> {
        self.server_presets.iter().find(|p| p.name == name)
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
    /// the default config. Returns the warnings to log.
    pub fn load(path: &Path) -> Result<(Config, Vec<String>)> {
        let mut config = if path.exists() { rsnfmt::read(path)? } else { Config::default() };
        let warnings = config.normalize();
        Ok((config, warnings))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        rsnfmt::write(path, self)
    }

    /// Applies the load rules: `library` folded into the recent libraries,
    /// built-in list if empty, sizes snapped to multiples of 32, exactly one
    /// default (the first marked, else the first).
    pub fn normalize(&mut self) -> Vec<String> {
        let mut warnings = Vec::new();
        if let Some(l) = self.library.take().filter(|l| !l.is_empty()) {
            self.add_recent_library(l);
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
