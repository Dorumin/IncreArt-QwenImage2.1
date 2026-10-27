//! Real-hardware smoke test: start one sd-server, load Qwen-Image 2.1 through
//! the engine and generate a few seeds into a fresh library.
//!
//!     cargo run --release --example smoke -- <empty-library-dir>
//!
//! Environment (defaults in brackets): SD_SERVER [/mnt/storage/sdcpp/build-hip/bin/sd-server],
//! MODEL_DIR [/mnt/storage/comfy-new/ComfyUI/models], DEVICE [ROCm0], PORT [1234].

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use increart_core::config::{SdServerConfig, ServerConfig};
use increart_core::engine::{Endpoint, Engine, EngineOptions, Event};
use increart_core::model::{Mode, ModelFile, ModelSelection, Sample};
use increart_core::{Library, NewIdea, NewRun, launcher};

fn env(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.into())
}

fn main() {
    let lib_dir = PathBuf::from(std::env::args().nth(1).expect("usage: smoke <empty-library-dir>"));
    let shared = SdServerConfig {
        binary: env("SD_SERVER", "/mnt/storage/sdcpp/build-hip/bin/sd-server"),
        model_dirs: vec![env("MODEL_DIR", "/mnt/storage/comfy-new/ComfyUI/models")],
        commit: "fd21ecf".into(),
        ..Default::default()
    };
    let server = ServerConfig::Start {
        name: "gpu0".into(),
        host: "127.0.0.1".into(),
        port: env("PORT", "1234").parse().unwrap(),
        device: env("DEVICE", "ROCm0"),
        args: vec![],
    };
    let t0 = Instant::now();
    let log_dir = lib_dir.with_extension("logs");
    let proc = launcher::open(&server, &shared, &log_dir, Duration::from_secs(120)).unwrap_or_else(|e| panic!("{e}"));
    println!("[{:>5.1}s] sd-server up (launched: {}, log {:?})", t0.elapsed().as_secs_f32(), proc.launched(), proc.log);

    let files = proc.client.model_files().unwrap();
    let pick = |wanted: &str| {
        let f = files
            .iter()
            .find(|f| f.id == wanted || f.relative_path.ends_with(wanted))
            .unwrap_or_else(|| panic!("no model file {wanted}"));
        ModelFile { id: f.id.clone(), sha256: "unhashed".into() }
    };
    let model = ModelSelection {
        recipe: "qwen-image-21".into(),
        files: [
            ("diffusion_model".to_string(), pick("diffusion_models/qwen_image_2.1_int8_convrot.safetensors")),
            ("llm".to_string(), pick("text_encoders/qwen3vl_8b_int8_convrot.safetensors")),
            ("vae".to_string(), pick("vae/qwen_image_2.1_vae_bf16.safetensors")),
        ]
        .into(),
    };

    let mut lib = Library::create(&lib_dir, None).unwrap();
    let idea =
        lib.new_idea(NewIdea::text("Fox", "a red fox reading a newspaper in a sunlit cafe, film photograph")).unwrap();
    let (project, _) = lib.create_project(Path::new(""), Some("smoke"), std::slice::from_ref(&idea)).unwrap();
    let sample = Sample {
        method: "euler".into(),
        scheduler: String::new(),
        steps: 20,
        cfg: 6.0,
        flow_shift: None,
        custom_sigmas: vec![],
    };
    let run = |label: &str, prompt: &str, alpha: bool, seeds: u64| NewRun {
        sweep: None,
        label: label.into(),
        prompt: prompt.into(),
        negative_prompt: String::new(),
        alpha,
        width: 1024,
        height: 1024,
        sample: sample.clone(),
        mode: Mode::Generate,
        refs: vec![],
        model: model.clone(),
        source_idea: Some(idea.clone()),
        first_seed: 0,
        seeds,
    };
    let r1 = lib
        .create_run(&project, run("r01", "a red fox reading a newspaper in a sunlit cafe, film photograph", false, 2))
        .unwrap();
    let r2 = lib.create_run(&project, run("r02 rgba", "cute robot mascot sticker, flat vector", true, 1)).unwrap();
    let lib = Arc::new(Mutex::new(lib));

    let endpoint = Endpoint { name: proc.name.clone(), device: proc.device.clone(), client: proc.client.clone() };
    let opts = EngineOptions { sdcpp: shared.commit.clone(), ..Default::default() };
    let engine = Engine::start(lib.clone(), vec![endpoint], opts, None);
    engine.queue_run(&r1);
    engine.queue_run(&r2);
    let mut done = 0;
    let mut last_step = (0, 0);
    while done < 2 {
        let e = engine.events().recv_timeout(Duration::from_secs(900)).expect("no event for 15 minutes");
        let t = t0.elapsed().as_secs_f32();
        match &e {
            Event::Progress { seed, step, total, .. } => {
                if (*seed, *step) != last_step && (*step == 1 || step % 5 == 0) {
                    println!("[{t:>5.1}s] seed {seed}: step {step}/{total}");
                }
                last_step = (*seed, *step);
            }
            Event::RunDone { .. } => {
                done += 1;
                println!("[{t:>5.1}s] {e:?}");
            }
            _ => println!("[{t:>5.1}s] {e:?}"),
        }
    }
    let lib = lib.lock().unwrap();
    let ix = lib.scan().unwrap();
    for o in &ix.project(&project).unwrap().outputs {
        let recipe = lib.recipe(&o.image.id).unwrap().unwrap();
        let img = image::open(lib.path(&o.path)).unwrap();
        println!(
            "{}  {}x{} alpha={} device={} elapsed={}s",
            o.path.display(),
            img.width(),
            img.height(),
            img.color().has_alpha(),
            recipe.device,
            recipe.elapsed_s
        );
    }
    println!("problems: {:?}", ix.problems);
    drop(engine);
}
