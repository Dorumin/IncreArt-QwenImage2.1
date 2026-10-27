//! The engine against fake sd-servers and a real library: runs end to end,
//! model switching, failures, cancel, back-off and purged jobs.

mod common;

use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine as _;
use common::fake::Fake;
use increart_core::engine::{Endpoint, Engine, EngineOptions, Event, Stop};
use increart_core::model::*;
use increart_core::sdserver::Client;
use increart_core::{Id, ImageRef, Library, NewIdea, NewRun, Original, Store, png};

const FILES: &[(&str, &str)] = &[("diffusion_model", "diffusion_models/q.safetensors"), ("vae", "vae/v.safetensors")];

fn model(recipe: &str) -> ModelSelection {
    let files = FILES
        .iter()
        .map(|(slot, id)| (slot.to_string(), ModelFile { id: id.to_string(), sha256: "cd".repeat(32) }))
        .collect::<BTreeMap<_, _>>();
    ModelSelection { recipe: recipe.into(), files }
}

struct Setup {
    _tmp: tempfile::TempDir,
    lib: Arc<Mutex<Library>>,
    project: Id,
    ref_image: Id,
}

fn setup() -> Setup {
    common::init();
    let tmp = tempfile::tempdir().unwrap();
    let mut lib = Library::create(&tmp.path().join("lib"), None).unwrap();
    let img = image::RgbaImage::from_fn(16, 16, |x, y| image::Rgba([x as u8 * 16, y as u8 * 16, 9, 200]));
    let mut bytes = Vec::new();
    image::DynamicImage::ImageRgba8(img).write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
    let ref_image =
        lib.import_bytes(&bytes, &Original { name: Some("robot.png".into()), ..Default::default() }).unwrap().image;
    let idea =
        lib.new_idea(NewIdea { images: vec![ref_image.clone()], ..NewIdea::text("Robot", "robot sticker") }).unwrap();
    let (project, _) = lib.create_project(Path::new(""), Some("stickers"), &[idea]).unwrap();
    Setup { _tmp: tmp, lib: Arc::new(Mutex::new(lib)), project, ref_image }
}

fn new_run(s: &Setup, recipe: &str, seeds: u64, alpha: bool) -> Id {
    let run = NewRun {
        sweep: None,
        label: "r01".into(),
        prompt: "cute robot sticker".into(),
        negative_prompt: String::new(),
        alpha,
        width: 1024,
        height: 1024,
        sample: Sample {
            method: "euler".into(),
            scheduler: "simple".into(),
            steps: 20,
            cfg: 6.0,
            flow_shift: Some(3.0),
            custom_sigmas: vec![],
        },
        mode: Mode::Generate,
        refs: vec![ImageRef::input(s.ref_image.clone())],
        model: model(recipe),
        source_idea: None,
        first_seed: 0,
        seeds,
    };
    s.lib.lock().unwrap().create_run(&s.project, run).unwrap()
}

fn opts() -> EngineOptions {
    EngineOptions {
        poll_interval: Duration::from_millis(10),
        backoff_base: Duration::from_millis(50),
        sdcpp: "fd21ecf".into(),
        ..Default::default()
    }
}

fn start(s: &Setup, fakes: &[&Fake]) -> Engine {
    let servers = fakes
        .iter()
        .enumerate()
        .map(|(i, f)| Endpoint { name: format!("gpu{i}"), device: format!("ROCm{i}"), client: Client::new(&f.url) })
        .collect();
    Engine::start(s.lib.clone(), servers, opts(), None)
}

/// Events until `stop` returns true (panics after 20 s).
fn collect(engine: &Engine, mut stop: impl FnMut(&Event, &[Event]) -> bool) -> Vec<Event> {
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut out = vec![];
    while Instant::now() < deadline {
        if let Ok(e) = engine.events().recv_timeout(Duration::from_millis(100)) {
            let done = stop(&e, &out);
            out.push(e);
            if done {
                return out;
            }
        }
    }
    panic!("timed out; events so far: {out:#?}");
}

fn run_done(run: &Id) -> impl FnMut(&Event, &[Event]) -> bool + '_ {
    move |e, _| matches!(e, Event::RunDone { run: r } if r == run)
}

fn outputs(events: &[Event]) -> Vec<(u64, Id)> {
    let mut v: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::Output { seed, image, .. } => Some((*seed, image.clone())),
            _ => None,
        })
        .collect();
    v.sort();
    v
}

const LOADED: Option<(&str, &[(&str, &str)])> = Some(("qwen-image-21", FILES));

#[test]
fn run_end_to_end_on_two_servers() {
    let s = setup();
    let (a, b) = (Fake::start(LOADED), Fake::start(LOADED));
    let run = new_run(&s, "qwen-image-21", 6, true);
    let engine = start(&s, &[&a, &b]);
    engine.queue_run(&run);
    let events = collect(&engine, run_done(&run));
    let outs = outputs(&events);
    assert_eq!(outs.iter().map(|(seed, _)| *seed).collect::<Vec<_>>(), [0, 1, 2, 3, 4, 5]);

    // both servers did work; each warmed up once first
    let (sa, sb) = (a.submissions(), b.submissions());
    assert!(!sa.is_empty() && !sb.is_empty(), "{} / {}", sa.len(), sb.len());
    assert_eq!(sa.len() + sb.len(), 6);
    assert_eq!(a.count("/img_gen") - sa.len(), 1, "one warm-up");
    let body = &sa[0];
    assert_eq!(body["output_format"], "png");
    assert_eq!(body["sample_params"]["guidance"]["txt_cfg"], 6.0);
    assert!(
        body["prompt"].as_str().unwrap().starts_with("This is an RGBA image with transparency. cute robot sticker.")
    );
    let lib = s.lib.lock().unwrap();
    let ref_bytes = std::fs::read(lib.image_path(&ImageRef::input(s.ref_image.clone()))).unwrap();
    let sent = base64::engine::general_purpose::STANDARD.decode(body["ref_images"][0].as_str().unwrap()).unwrap();
    assert_eq!(sent, ref_bytes, "ref sent as the store PNG");

    // stored with recipe, linked as unreviewed, nothing pending
    assert!(lib.pending_seeds(&run).unwrap().is_empty());
    let ix = lib.scan().unwrap();
    let p = ix.project(&s.project).unwrap();
    assert_eq!(p.outputs.len(), 6);
    assert!(p.outputs.iter().all(|o| o.verdict.is_none()));
    let mut devices = std::collections::BTreeSet::new();
    for (seed, image) in &outs {
        let recipe = lib.recipe(image).unwrap().unwrap();
        assert_eq!(recipe.seed, *seed);
        assert!(recipe.alpha && recipe.prompt_sent.starts_with("This is an RGBA"));
        assert_eq!(recipe.prompt_original, "cute robot sticker");
        assert_eq!((recipe.sdcpp.as_str(), recipe.elapsed_s), ("fd21ecf", 33.0));
        devices.insert(recipe.device);
        let file = std::fs::read(lib.image_path(&ImageRef::output(image.clone()))).unwrap();
        assert!(png::text(&file, "parameters").unwrap().is_some(), "sd-server's chunk kept");
        assert!(image::load_from_memory(&file).unwrap().color().has_alpha());
    }
    assert_eq!(devices.into_iter().collect::<Vec<_>>(), ["ROCm0", "ROCm1"]);
    // each image's timing, in run.rsn: generating, then waiting before it
    let (_, r) = lib.find_run(&run).unwrap();
    let mut seeds: Vec<u64> = r.timings.iter().map(|t| t.seed).collect();
    seeds.sort();
    assert_eq!(seeds, [0, 1, 2, 3, 4, 5]);
    assert!(r.timings.iter().all(|t| t.elapsed_s == 33.0 && t.queued_s == 1.0 && !t.device.is_empty()));
    drop(lib);
    let report = common::verify(s.lib.lock().unwrap().root());
    assert!(report.trim_end().ends_with("problems: none"), "{report}");
}

#[test]
fn loads_the_model_a_run_needs() {
    let s = setup();
    let a = Fake::start(Some(("other-recipe", &[])));
    let run = new_run(&s, "qwen-image-21", 2, false);
    let engine = start(&s, &[&a]);
    engine.queue_run(&run);
    let events = collect(&engine, run_done(&run));
    assert_eq!(outputs(&events).len(), 2);
    let loads: Vec<_> =
        a.with(|st| st.log.iter().filter(|(_, p, _)| p.ends_with("models/load")).map(|(_, _, b)| b.clone()).collect());
    assert_eq!(loads.len(), 1);
    assert_eq!(loads[0]["recipe"], "qwen-image-21");
    assert_eq!(loads[0]["models"]["vae"], "vae/v.safetensors");
    assert!(events.iter().any(|e| matches!(e, Event::Server { state, .. } if state.starts_with("loading"))));
}

#[test]
fn failed_seeds_are_recorded_and_the_run_finishes() {
    let s = setup();
    let a = Fake::start(LOADED);
    a.with(|st| st.fail_seeds = vec![1]);
    let run = new_run(&s, "qwen-image-21", 4, false);
    let engine = start(&s, &[&a]);
    engine.queue_run(&run);
    let events = collect(&engine, run_done(&run));
    let failed: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::SeedFailed { seed, error, .. } => Some((*seed, error.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].0, 1);
    assert!(failed[0].1.contains("generation_failed"));
    assert_eq!(outputs(&events).len(), 3);
    let lib = s.lib.lock().unwrap();
    let (_, r) = lib.find_run(&run).unwrap();
    assert_eq!(r.failed.len(), 1);
    assert!(lib.pending_seeds(&run).unwrap().is_empty(), "a failed seed isn't pending");
}

#[test]
fn bad_request_fails_only_that_seed() {
    let s = setup();
    let a = Fake::start(LOADED);
    let run = new_run(&s, "qwen-image-21", 3, false);
    let mut opts = opts();
    opts.warm_up = false;
    opts.max_in_flight = 1;
    let engine = Engine::start(
        s.lib.clone(),
        vec![Endpoint { name: "gpu0".into(), device: "ROCm0".into(), client: Client::new(&a.url) }],
        opts,
        None,
    );
    a.with(|st| st.submit_errors.push_back((422, serde_json::json!({"error": "width must be a multiple of 32"}))));
    engine.queue_run(&run);
    let events = collect(&engine, run_done(&run));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::SeedFailed { seed: 0, error, .. } if error.contains("multiple of 32")))
    );
    assert_eq!(outputs(&events).iter().map(|(s, _)| *s).collect::<Vec<_>>(), [1, 2]);
}

#[test]
fn cancel_stops_a_run() {
    let s = setup();
    let a = Fake::start(LOADED);
    a.with(|st| st.steps = 1_000_000); // jobs never finish on their own
    let run = new_run(&s, "qwen-image-21", 8, false);
    let mut o = opts();
    o.warm_up = false;
    let engine = Engine::start(
        s.lib.clone(),
        vec![Endpoint { name: "gpu0".into(), device: "ROCm0".into(), client: Client::new(&a.url) }],
        o,
        None,
    );
    engine.queue_run(&run);
    collect(&engine, |e, _| matches!(e, Event::Progress { .. }));
    engine.cancel_run(&run, Stop::Now);
    let events = collect(&engine, run_done(&run));
    assert!(outputs(&events).is_empty());
    assert_eq!(a.count("/cancel"), 2, "both in-flight jobs cancelled");
    let lib = s.lib.lock().unwrap();
    let (_, r) = lib.find_run(&run).unwrap();
    assert_eq!(r.cancelled, [Seeds { first: 0, count: 8 }]);
    assert!(lib.pending_seeds(&run).unwrap().is_empty());
}

#[test]
fn finish_and_stop_keeps_the_images_being_generated() {
    let s = setup();
    let a = Fake::start(LOADED);
    a.with(|st| st.steps = 30);
    let run = new_run(&s, "qwen-image-21", 8, false);
    let mut o = opts();
    o.warm_up = false;
    let engine = Engine::start(
        s.lib.clone(),
        vec![Endpoint { name: "gpu0".into(), device: "ROCm0".into(), client: Client::new(&a.url) }],
        o,
        None,
    );
    engine.queue_run(&run);
    // the fake generates both in-flight jobs at once
    collect(&engine, |e, _| matches!(e, Event::Progress { step: 3.., .. }));
    engine.cancel_run(&run, Stop::Finish);
    let events = collect(&engine, run_done(&run));
    assert_eq!(outputs(&events).len(), 2, "both generating images finished");
    assert_eq!(a.count("/cancel"), 0);
    let lib = s.lib.lock().unwrap();
    let (_, r) = lib.find_run(&run).unwrap();
    assert_eq!(r.cancelled, [Seeds { first: 2, count: 6 }]);
    assert!(lib.pending_seeds(&run).unwrap().is_empty());
}

#[test]
fn finish_and_stop_cancels_the_waiting_job() {
    let s = setup();
    let a = Fake::start(LOADED);
    a.with(|st| {
        st.steps = 30;
        st.serial = true;
    });
    let run = new_run(&s, "qwen-image-21", 8, false);
    let mut o = opts();
    o.warm_up = false;
    let engine = Engine::start(
        s.lib.clone(),
        vec![Endpoint { name: "gpu0".into(), device: "ROCm0".into(), client: Client::new(&a.url) }],
        o,
        None,
    );
    engine.queue_run(&run);
    collect(&engine, |e, _| matches!(e, Event::Progress { step: 3.., .. }));
    engine.cancel_run(&run, Stop::Finish);
    let events = collect(&engine, run_done(&run));
    assert_eq!(outputs(&events).len(), 1, "the generating image finished");
    assert_eq!(a.count("/cancel"), 1, "the waiting one cancelled");
    let lib = s.lib.lock().unwrap();
    let (_, r) = lib.find_run(&run).unwrap();
    assert_eq!(r.cancelled, [Seeds { first: 1, count: 7 }]);
}

#[test]
fn a_failing_server_hands_its_seeds_to_the_other() {
    let s = setup();
    let (a, b) = (Fake::start(LOADED), Fake::start(LOADED));
    a.with(|st| st.steps = 20);
    let run = new_run(&s, "qwen-image-21", 6, false);
    // No warm-up: both servers are ready at once, so server 0 surely gets
    // jobs (with a warm-up, server 1 could finish everything first).
    let mut o = opts();
    o.warm_up = false;
    let servers = [&a, &b]
        .iter()
        .enumerate()
        .map(|(i, f)| Endpoint { name: format!("gpu{i}"), device: format!("ROCm{i}"), client: Client::new(&f.url) })
        .collect();
    let engine = Engine::start(s.lib.clone(), servers, o, None);
    engine.queue_run(&run);
    // once server 0 has a job running, it goes down for good
    collect(&engine, |e, _| matches!(e, Event::Progress { server: 0, .. }));
    a.with(|st| st.down = true);
    let events = collect(&engine, run_done(&run));
    assert!(events.iter().any(|e| matches!(e, Event::Server { server: 0, state, .. } if state.contains("retrying"))));
    let lib = s.lib.lock().unwrap();
    assert!(lib.pending_seeds(&run).unwrap().is_empty());
    let ix = lib.scan().unwrap();
    assert_eq!(ix.project(&s.project).unwrap().outputs.len(), 6);
}

#[test]
fn purged_job_is_generated_again() {
    let s = setup();
    let a = Fake::start(LOADED);
    a.with(|st| st.lose_seeds = vec![1]);
    let run = new_run(&s, "qwen-image-21", 2, false);
    let engine = start(&s, &[&a]);
    engine.queue_run(&run);
    let events = collect(&engine, run_done(&run));
    assert_eq!(outputs(&events).len(), 2);
    let seed1 = a.submissions().iter().filter(|b| b["seed"] == 1).count();
    assert_eq!(seed1, 2, "submitted again after the result was lost");
    assert_eq!(s.lib.lock().unwrap().store_ids(Store::Output).unwrap().len(), 2);
}

#[test]
fn requeue_after_adding_seeds() {
    let s = setup();
    let a = Fake::start(LOADED);
    let run = new_run(&s, "qwen-image-21", 2, false);
    let engine = start(&s, &[&a]);
    engine.queue_run(&run);
    collect(&engine, run_done(&run));
    s.lib.lock().unwrap().add_seeds(&run, 3, true).unwrap();
    engine.queue_run(&run);
    let events = collect(&engine, run_done(&run));
    assert_eq!(outputs(&events).iter().map(|(s, _)| *s).collect::<Vec<_>>(), [2, 3, 4]);
    let names: Vec<_> =
        s.lib.lock().unwrap().scan().unwrap().project(&s.project).unwrap().outputs.iter().map(|o| o.name.rev).collect();
    assert_eq!(names.iter().filter(|r| **r == 1).count(), 3, "new seeds in revision 1");
}

#[test]
fn renaming_and_moving_a_project_mid_run_keeps_every_output() {
    let s = setup();
    let a = Fake::start(LOADED);
    a.with(|st| st.steps = 15); // slow enough that the run outlives the moves
    let run = new_run(&s, "qwen-image-21", 6, false);
    let engine = start(&s, &[&a]);
    engine.queue_run(&run);
    // after the first image: rename the project, then move it into a folder
    let first = collect(&engine, |e, _| matches!(e, Event::Output { .. }));
    {
        let lib = s.lib.lock().unwrap();
        lib.rename_node(increart_core::Tree::Projects, Path::new("stickers"), "robots").unwrap();
        lib.create_folder(increart_core::Tree::Projects, Path::new(""), Some("clients")).unwrap();
        lib.move_node(increart_core::Tree::Projects, Path::new("robots"), Path::new("clients")).unwrap();
    }
    let mut events = first;
    events.extend(collect(&engine, run_done(&run)));
    assert_eq!(outputs(&events).len(), 6);
    let lib = s.lib.lock().unwrap();
    assert!(lib.pending_seeds(&run).unwrap().is_empty());
    let ix = lib.scan().unwrap();
    let p = ix.project(&s.project).unwrap();
    assert_eq!(p.dir, Path::new("projects/clients/robots"));
    assert_eq!(p.outputs.len(), 6, "images finished after the moves land in the new place");
    drop(lib);
    let report = common::verify(s.lib.lock().unwrap().root());
    assert!(report.trim_end().ends_with("problems: none"), "{report}");
}
