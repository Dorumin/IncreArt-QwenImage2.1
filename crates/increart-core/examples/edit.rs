//! Real-hardware test of editing one image: the same prompt as a reference
//! only, with Transparent on, and as reference + img2img start, to see which
//! actually edits. Makes its own throwaway library; starts its own sd-server.
//!
//!     cargo run --release --example edit -- <new-library-dir> <image.png> <template run.rsn>
//!
//! The template run gives the model files (and sampling). Environment:
//! SD_SERVER, MODEL_DIR, DEVICE [ROCm1], PORT [1236], PROMPT, SIZE [832x1248],
//! CFG [2], VARIANTS: comma-separated `label:start:alpha[:refs]`, start a
//! strength or `-`, refs 1 (the image as `<image1>`, the default) or 0
//! [ref:-:0,ref-alpha:-:1,ref+start0.75:0.75:0].

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use increart_core::config::{SdServerConfig, ServerConfig};
use increart_core::engine::{Endpoint, Engine, EngineOptions, Event};
use increart_core::model::{Mode, Run};
use increart_core::{ImageRef, Library, NewRun, launcher, rsnfmt};

fn env(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.into())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [lib_dir, image, template] = &args[..] else { panic!("usage: edit <new-library-dir> <image.png> <run.rsn>") };
    let template: Run = rsnfmt::read(Path::new(template)).unwrap();
    let shared = SdServerConfig {
        binary: env("SD_SERVER", "/mnt/storage/sdcpp/build-hip/bin/sd-server"),
        model_dirs: vec![env("MODEL_DIR", "/mnt/storage/comfy-new/ComfyUI/models")],
        args: vec!["--fa".into(), "--conditioning-cache-size".into(), "32".into()],
        commit: "fd21ecf".into(),
        ..Default::default()
    };
    let server = ServerConfig::Start {
        name: "edit-test".into(),
        host: "127.0.0.1".into(),
        port: env("PORT", "1236").parse().unwrap(),
        device: env("DEVICE", "ROCm1"),
        args: vec![],
    };
    let t0 = Instant::now();
    let lib_dir = PathBuf::from(lib_dir);
    let mut lib = Library::create(&lib_dir, None).unwrap();
    let img = ImageRef::input(lib.import_file(Path::new(image)).unwrap().image);
    let (project, _) = lib.create_project(Path::new(""), Some("edit-test"), &[]).unwrap();
    let lib = Arc::new(Mutex::new(lib));
    let proc = launcher::open(&server, &shared, &lib_dir.with_extension("logs"), Duration::from_secs(180))
        .unwrap_or_else(|e| panic!("{e}"));
    println!("[{:>5.1}s] sd-server up on {}", t0.elapsed().as_secs_f32(), proc.device);
    let endpoint = Endpoint { name: proc.name.clone(), device: proc.device.clone(), client: proc.client.clone() };
    let engine = Engine::start(
        lib.clone(),
        vec![endpoint],
        EngineOptions { sdcpp: shared.commit.clone(), ..Default::default() },
        None,
    );

    let prompt = env("PROMPT", &template.prompt);
    let (w, h) =
        env("SIZE", "832x1248").split_once('x').map(|(a, b)| (a.parse().unwrap(), b.parse().unwrap())).unwrap();
    let mut sample = template.sample.clone();
    sample.cfg = env("CFG", "2").parse().unwrap();
    for variant in env("VARIANTS", "ref:-:0,ref-alpha:-:1,ref+start0.75:0.75:0").split(',') {
        let parts: Vec<&str> = variant.split(':').collect();
        let (label, start, alpha) = (parts[0], parts[1], parts[2]);
        let with_ref = parts.get(3) != Some(&"0");
        let mode = match start {
            "-" => Mode::Generate,
            s => Mode::Edit { init: Some(img.clone()), strength: s.parse().unwrap() },
        };
        let run = NewRun {
            sweep: None,
            label: label.into(),
            prompt: prompt.clone(),
            negative_prompt: String::new(),
            alpha: alpha == "1",
            width: w,
            height: h,
            sample: sample.clone(),
            mode,
            refs: if with_ref { vec![img.clone()] } else { vec![] },
            model: template.model.clone(),
            source_idea: None,
            first_seed: 0,
            seeds: 1,
        };
        let id = lib.lock().unwrap().create_run(&project, run).unwrap();
        engine.queue_run(&id);
        loop {
            let e = engine.events().recv_timeout(Duration::from_secs(900)).expect("no event for 15 minutes");
            match &e {
                Event::Progress { .. } => {}
                Event::RunDone { run } if *run == id => break,
                _ => println!("[{:>5.1}s] {e:?}", t0.elapsed().as_secs_f32()),
            }
        }
        let l = lib.lock().unwrap();
        let ix = l.scan().unwrap();
        for o in ix.project(&project).unwrap().outputs.iter().filter(|o| o.name.run == id) {
            println!("{label}: {}", lib_dir.join(o.image.rel_path()).display());
        }
    }
    println!("input: {}", lib_dir.join(img.rel_path()).display());
    drop(engine);
}
