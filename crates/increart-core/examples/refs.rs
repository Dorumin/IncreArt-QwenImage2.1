//! Real-hardware test of several reference images at once: in a library made
//! by `examples/smoke.rs`, generate a third subject, then a run with three
//! refs (fox, robot, teapot) placed side by side on a given background.
//!
//!     cargo run --release --example refs -- tmp/smoke-library
//!
//! Environment as for `smoke`: SD_SERVER, MODEL_DIR, DEVICE [ROCm0], PORT [1234].
//! Variations: CFG [6], PROMPT, LABEL [r04 three refs], SEEDS [2], ALPHA [0],
//! REF_FILES (comma-separated image files imported as the refs, instead of
//! fox/robot/teapot), SIZE [1344x896].

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use increart_core::config::{SdServerConfig, ServerConfig};
use increart_core::engine::{Endpoint, Engine, EngineOptions, Event};
use increart_core::model::{Mode, Run};
use increart_core::{Id, ImageRef, Library, NewRun, launcher};

fn env(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.into())
}

/// Queues a run and prints events until it is done.
fn generate(engine: &Engine, run: &Id, t0: Instant) {
    engine.queue_run(run);
    loop {
        let e = engine.events().recv_timeout(Duration::from_secs(900)).expect("no event for 15 minutes");
        let t = t0.elapsed().as_secs_f32();
        match &e {
            Event::Progress { .. } => {}
            Event::RunDone { run: r } if r == run => {
                println!("[{t:>5.1}s] {e:?}");
                return;
            }
            _ => println!("[{t:>5.1}s] {e:?}"),
        }
    }
}

/// The first output image of the run with this label.
fn output_of(lib: &Library, label: &str) -> Option<ImageRef> {
    let ix = lib.scan().unwrap();
    let p = ix.projects.iter().find(|p| p.name() == "smoke")?;
    let run = p.runs.iter().find(|r| r.label == label)?;
    let mut outs: Vec<_> = p.outputs.iter().filter(|o| o.name.run == run.id).collect();
    outs.sort_by_key(|o| o.name.seed);
    outs.first().map(|o| o.image.clone())
}

fn main() {
    let lib_dir = PathBuf::from(std::env::args().nth(1).expect("usage: refs <smoke-library-dir>"));
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
    let lib = Library::open(&lib_dir).unwrap();
    let ix = lib.scan().unwrap();
    let project = ix.projects.iter().find(|p| p.name() == "smoke").expect("run examples/smoke.rs first");
    let template: Run = project.runs.iter().find(|r| r.label == "r01").unwrap().clone();
    let project_id = project.project.id.clone();
    let fox = output_of(&lib, "r01").unwrap();
    let robot = output_of(&lib, "r02 rgba").unwrap();
    let mut lib = lib;
    let ref_files: Vec<ImageRef> = env("REF_FILES", "")
        .split(',')
        .filter(|f| !f.is_empty())
        .map(|f| ImageRef::input(lib.import_file(std::path::Path::new(f)).unwrap().image))
        .collect();
    let lib = Arc::new(Mutex::new(lib));

    let proc = launcher::open(&server, &shared, &lib_dir.with_extension("logs"), Duration::from_secs(120))
        .unwrap_or_else(|e| panic!("{e}"));
    println!("[{:>5.1}s] sd-server up", t0.elapsed().as_secs_f32());
    let endpoint = Endpoint { name: proc.name.clone(), device: proc.device.clone(), client: proc.client.clone() };
    let engine = Engine::start(
        lib.clone(),
        vec![endpoint],
        EngineOptions { sdcpp: shared.commit.clone(), ..Default::default() },
        None,
    );

    let mut sample = template.sample.clone();
    sample.cfg = env("CFG", "6").parse().unwrap();
    let new_run = |label: &str, prompt: &str, refs: Vec<ImageRef>, width, height, seeds| NewRun {
        sweep: None,
        label: label.into(),
        prompt: prompt.into(),
        negative_prompt: String::new(),
        alpha: env("ALPHA", "0") == "1",
        width,
        height,
        sample: sample.clone(),
        mode: Mode::Generate,
        refs,
        model: template.model.clone(),
        source_idea: None,
        first_seed: 0,
        seeds,
    };

    let existing = output_of(&lib.lock().unwrap(), "r03 teapot");
    let teapot = match existing.filter(|_| ref_files.is_empty()) {
        Some(t) => t,
        None if !ref_files.is_empty() => fox.clone(),
        None => {
            let prompt =
                "a round ceramic teapot with a blue-green crackle glaze, studio product photo, plain white background";
            let run = lib
                .lock()
                .unwrap()
                .create_run(&project_id, new_run("r03 teapot", prompt, vec![], 1024, 1024, 1))
                .unwrap();
            generate(&engine, &run, t0);
            output_of(&lib.lock().unwrap(), "r03 teapot").unwrap()
        }
    };

    let prompt = env(
        "PROMPT",
        "Place the three subjects side by side in one row, left to right: the red fox from image 1, \
         the white robot from image 2, and the ceramic teapot from image 3. All three fully visible, \
         similar size, standing on the same surface. Plain pastel yellow seamless studio background, \
         soft even lighting.",
    );
    let label = env("LABEL", "r04 three refs");
    let seeds = env("SEEDS", "2").parse().unwrap();
    let refs = if ref_files.is_empty() { vec![fox.clone(), robot.clone(), teapot.clone()] } else { ref_files.clone() };
    let size = env("SIZE", "1344x896");
    let (w, h) = size.split_once('x').unwrap();
    let (w, h): (u32, u32) = (w.parse().unwrap(), h.parse().unwrap());
    let run = lib.lock().unwrap().create_run(&project_id, new_run(&label, &prompt, refs, w, h, seeds)).unwrap();
    generate(&engine, &run, t0);

    let lib = lib.lock().unwrap();
    if ref_files.is_empty() {
        println!("refs: 1 fox {}, 2 robot {}, 3 teapot {}", fox.rel_path(), robot.rel_path(), teapot.rel_path());
    }
    let ix = lib.scan().unwrap();
    for o in ix.project(&project_id).unwrap().outputs.iter().filter(|o| o.name.run == run) {
        let r = lib.recipe(&o.image.id).unwrap().unwrap();
        println!("{}  refs={} cfg={} elapsed={}s", o.path.display(), r.refs.len(), r.sample.cfg, r.elapsed_s);
    }
    println!("problems: {:?}", ix.problems);
    drop(engine);
}
