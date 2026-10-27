//! Building libraries entirely through the core API, then checking them with
//! the independent verifier (scripts/verify-sample-library.py) and a rescan.

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use chrono::TimeZone;
use image::{DynamicImage, ImageEncoder, Rgb, RgbImage, Rgba, RgbaImage};
use increart_core::model::*;
use increart_core::{
    Disposal, Error, IdeaLoc, ImageRef, Library, NewIdea, NewRun, Original, Store, Tree, Verdict, fsutil, link, png,
};

fn t0() -> increart_core::id::Timestamp {
    chrono::Utc.with_ymd_and_hms(2026, 10, 24, 12, 0, 0).unwrap()
}

fn picture(w: u32, h: u32, seed: u8) -> RgbImage {
    RgbImage::from_fn(w, h, |x, y| Rgb([(x * 4) as u8 ^ seed, (y * 4) as u8, seed]))
}

fn png_bytes(img: DynamicImage) -> Vec<u8> {
    let mut out = Vec::new();
    img.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
    out
}

fn jpeg_bytes(img: RgbImage) -> Vec<u8> {
    let mut out = Vec::new();
    DynamicImage::ImageRgb8(img).write_to(&mut Cursor::new(&mut out), image::ImageFormat::Jpeg).unwrap();
    out
}

/// A PNG with an `eXIf` block saying "rotate 90" (orientation 6).
fn rotated_png() -> Vec<u8> {
    let mut exif = b"MM\0\x2a\0\0\0\x08\0\x01".to_vec();
    exif.extend([0x01, 0x12, 0, 3, 0, 0, 0, 1, 0, 6, 0, 0]);
    exif.extend([0, 0, 0, 0]);
    let plain = png_bytes(DynamicImage::ImageRgb8(picture(32, 16, 1)));
    png::set_chunks(&plain, &[png::Chunk::exif(exif)]).unwrap()
}

/// A PNG tagged with a Display P3 ICC profile.
fn p3_png() -> Vec<u8> {
    let mut out = Vec::new();
    let mut enc = image::codecs::png::PngEncoder::new(&mut out);
    enc.set_icc_profile(moxcms::ColorProfile::new_display_p3().encode().unwrap()).unwrap();
    let img = picture(16, 16, 2);
    enc.write_image(img.as_raw(), 16, 16, image::ExtendedColorType::Rgb8).unwrap();
    out
}

/// What sd-server returns: a PNG with its own `parameters` chunk.
fn sdserver_png(seed: u8) -> Vec<u8> {
    let plain = png_bytes(DynamicImage::ImageRgb8(picture(64, 64, 100 + seed)));
    png::set_chunks(&plain, &[png::Chunk::text("parameters", &format!("a fox\nSeed: {seed}"))]).unwrap()
}

fn sample() -> Sample {
    Sample {
        method: "euler".into(),
        scheduler: "simple".into(),
        steps: 20,
        cfg: 6.0,
        flow_shift: Some(3.0),
        custom_sigmas: vec![],
    }
}

fn model() -> ModelSelection {
    let files = BTreeMap::from([(
        "diffusion_model".to_string(),
        ModelFile { id: "diffusion_models/q.safetensors".into(), sha256: "ab".repeat(32) },
    )]);
    ModelSelection { recipe: "qwen-image-21".into(), files }
}

fn new_run(refs: Vec<ImageRef>, mode: Mode, seeds: u64) -> NewRun {
    NewRun {
        sweep: None,
        label: "r01".into(),
        prompt: "a fox in a café".into(),
        negative_prompt: String::new(),
        alpha: false,
        width: 64,
        height: 64,
        sample: sample(),
        mode,
        refs,
        model: model(),
        source_idea: None,
        first_seed: 0,
        seeds,
    }
}

fn recipe(run: &Run, seed: u64, parent: Option<ImageRef>) -> Recipe {
    Recipe {
        prompt_original: run.prompt.clone(),
        prompt_sent: run.prompt.clone(),
        negative_prompt: String::new(),
        width: run.width,
        height: run.height,
        seed,
        alpha: false,
        sample: run.sample.clone(),
        mode: run.mode.clone(),
        refs: run.refs.clone(),
        parent,
        source_idea: run.source_idea.clone(),
        rewrite: None,
        model: run.model.clone(),
        sdcpp: "fd21ecf".into(),
        increart: env!("CARGO_PKG_VERSION").into(),
        device: "ROCm0".into(),
        elapsed_s: 33.4,
        queued_s: 0.0,
        step_s: 0.0,
        inputs: vec![],
    }
}

fn original(name: &str) -> Original {
    Original {
        name: Some(name.into()),
        path: Some(format!("/home/user/Downloads/{name}")),
        mtime: Some(t0()),
        ..Default::default()
    }
}

fn setup(mode: Option<LinkMode>) -> (tempfile::TempDir, Library) {
    common::init();
    let tmp = tempfile::tempdir().unwrap();
    let mut lib = Library::create(&tmp.path().join("lib"), mode).unwrap();
    lib.set_clock(t0);
    lib.set_disposal(Disposal::Directory(tmp.path().join("trash")));
    (tmp, lib)
}

/// The image a link resolves to, checking it really reaches the store file.
fn resolves(lib: &Library, rel: &Path, store: Store) -> ImageRef {
    let abs = lib.path(rel);
    let r = link::read_ref(&abs, store).unwrap().unwrap_or_else(|| panic!("{} is not a link", rel.display()));
    assert_eq!(fs::read(&abs).unwrap(), fs::read(lib.image_path(&r)).unwrap(), "{}", rel.display());
    r
}

#[test]
fn whole_library_through_the_api() {
    let (tmp, mut lib) = setup(None);
    let root = lib.root().to_path_buf();
    assert_eq!(lib.link_mode(), LinkMode::Symlink);

    // ---- import
    let jpeg = jpeg_bytes(picture(40, 30, 7));
    let photo = lib.import_bytes(&jpeg, &original("photo.jpg")).unwrap();
    assert!(!photo.reused);
    let again = lib.import_bytes(&jpeg, &original("copy-of-photo.jpg")).unwrap();
    assert_eq!((again.image.clone(), again.reused), (photo.image.clone(), true));
    let photo = photo.image;
    let src = lib.source_info(&photo).unwrap().unwrap();
    assert_eq!((src.original_format.as_str(), src.original_name.as_deref()), ("jpeg", Some("photo.jpg")));
    assert_eq!(src.conversions, ["jpeg -> png"]);
    assert_eq!(src.original_sha256, increart_core::pixel::sha256_hex(&jpeg));

    let rotated = lib.import_bytes(&rotated_png(), &original("hut.png")).unwrap().image;
    let rot_file = fs::read(lib.image_path(&ImageRef::input(rotated.clone()))).unwrap();
    let rot_img = image::load_from_memory(&rot_file).unwrap();
    assert_eq!((rot_img.width(), rot_img.height()), (16, 32));
    assert!(png::read_chunks(&rot_file).unwrap().iter().any(|c| &c.kind == b"eXIf"), "EXIF kept");
    assert_eq!(lib.source_info(&rotated).unwrap().unwrap().conversions, ["exif-orientation: rotate 90"]);

    let p3 = lib.import_bytes(&p3_png(), &original("fish.png")).unwrap().image;
    let notes = lib.source_info(&p3).unwrap().unwrap().conversions;
    assert!(notes.len() == 1 && notes[0].starts_with("icc: ") && notes[0].ends_with(" -> sRGB"), "{notes:?}");

    let rgba = RgbaImage::from_fn(8, 8, |x, _| Rgba([200, 10, 10, (x * 30) as u8]));
    let sticker = lib.import_bytes(&png_bytes(DynamicImage::ImageRgba8(rgba)), &original("sticker.png")).unwrap().image;
    let st = image::load_from_memory(&fs::read(lib.image_path(&ImageRef::input(sticker.clone()))).unwrap()).unwrap();
    assert!(st.color().has_alpha());

    // ---- ideas and input review
    let fox = lib.new_idea(NewIdea { images: vec![photo.clone()], ..NewIdea::text("Fox", "a fox in a café") }).unwrap();
    let hut = lib
        .new_idea(NewIdea {
            images: vec![rotated.clone(), p3.clone()],
            source: IdeaSource::DroppedFile { original_path: "/home/user/Pictures/hut.png".into() },
            ..NewIdea::text("Hut", "")
        })
        .unwrap();
    let junk = lib.new_idea(NewIdea { images: vec![sticker.clone()], ..NewIdea::text("Junk", "meh") }).unwrap();
    assert_eq!(fox.as_str(), "2026.10.24-12.00-0000");
    assert_eq!(hut.as_str(), "2026.10.24-12.00-0001");
    let hut_dir = lib.find_idea(&hut).unwrap().1;
    assert!(hut_dir.join("1-hut.png").exists() && hut_dir.join("2-fish.png").exists());

    lib.review_idea(&fox, Verdict::Good).unwrap();
    lib.review_idea(&hut, Verdict::Neutral).unwrap();
    lib.review_idea(&junk, Verdict::Bad).unwrap();
    assert!(lib.load_idea(&fox).unwrap().starred);
    assert!(!lib.load_idea(&hut).unwrap().starred);
    assert_eq!(lib.find_idea(&junk).unwrap().0, IdeaLoc::Discarded);

    // ---- input library folders
    let places = lib.create_folder(Tree::InputLibrary, Path::new(""), Some("places")).unwrap();
    assert!(matches!(
        lib.create_folder(Tree::InputLibrary, Path::new(""), Some("Places")),
        Err(Error::InvalidName { .. })
    ));
    let deep = lib.create_folder(Tree::InputLibrary, &places, None).unwrap();
    assert_eq!(deep, Path::new("places/new folder"));
    let moved = lib.move_node(Tree::InputLibrary, Path::new(hut.as_str()), &deep).unwrap();
    assert_eq!(lib.find_idea(&hut).unwrap().0, IdeaLoc::Library(deep.clone()));
    resolves(&lib, &Path::new("input/library").join(&moved).join("1-hut.png"), Store::Input);
    let renamed = lib.rename_node(Tree::InputLibrary, &deep, "alpine").unwrap();
    assert_eq!(renamed, Path::new("places/alpine"));
    assert!(matches!(
        lib.move_node(Tree::InputLibrary, Path::new("places"), Path::new("places/alpine")),
        Err(Error::Refused(_))
    ));
    let copy = lib.copy_node(Tree::InputLibrary, Path::new("places"), Path::new("")).unwrap();
    assert_eq!(copy, Path::new("places 2"));
    let ix = lib.scan().unwrap();
    assert_eq!(ix.ideas.iter().filter(|e| e.idea.title == "Hut").count(), 2);

    // ---- projects, runs, outputs
    lib.create_folder(Tree::Projects, Path::new(""), Some("clients")).unwrap();
    let (project, project_path) =
        lib.create_project(Path::new("clients"), Some("fox"), std::slice::from_ref(&fox)).unwrap();
    assert_eq!(project_path, Path::new("clients/fox"));
    assert_eq!(lib.load_idea(&fox).unwrap().projects, std::slice::from_ref(&project));
    assert!(matches!(lib.create_project(Path::new("clients/fox"), None, &[]), Err(Error::Refused(_))));
    let run = lib.create_run(&project, new_run(vec![ImageRef::input(photo.clone())], Mode::Generate, 4)).unwrap();
    let (_, run_def) = lib.find_run(&run).unwrap();
    let mut outs = Vec::new();
    for seed in 0..4 {
        let (id, link_path) =
            lib.write_output(&run, 0, seed, &sdserver_png(seed as u8), &recipe(&run_def, seed, None)).unwrap();
        assert_eq!(link_path.file_name().unwrap().to_string_lossy(), format!("{run}_r0_s{seed:04}.png"));
        let bytes = fs::read(lib.image_path(&ImageRef::output(id.clone()))).unwrap();
        assert!(png::text(&bytes, "parameters").unwrap().unwrap().starts_with("a fox"), "parameters kept");
        assert_eq!(lib.recipe(&id).unwrap().unwrap().seed, seed);
        outs.push((id, link_path));
    }
    // the output records its inputs and both carry a BLAKE3 pixel hash
    let photo_ref = ImageRef::input(photo.clone());
    let photo_bytes = fs::read(lib.image_path(&photo_ref)).unwrap();
    let photo_blake3 = png::text(&photo_bytes, png::KEY_BLAKE3).unwrap().unwrap();
    assert_eq!(photo_blake3, increart_core::import::decode_pixels(&photo_bytes).unwrap().blake3());
    let inputs = lib.recipe(&outs[0].0).unwrap().unwrap().inputs;
    assert_eq!(inputs.len(), 1);
    assert_eq!((inputs[0].role, &inputs[0].image), (InputRole::Ref(1), &photo_ref));
    assert_eq!(inputs[0].blake3, photo_blake3);
    assert_eq!(inputs[0].file, format!("input_images/{photo}.png"));
    assert!(inputs[0].original_name.is_some());
    let out_bytes = fs::read(lib.image_path(&ImageRef::output(outs[0].0.clone()))).unwrap();
    assert_eq!(
        png::text(&out_bytes, png::KEY_BLAKE3).unwrap().unwrap(),
        increart_core::import::decode_pixels(&out_bytes).unwrap().blake3()
    );
    let good = lib.review_output(&outs[0].1, Some(Verdict::Good)).unwrap();
    lib.review_output(&outs[1].1, Some(Verdict::Bad)).unwrap();
    let neutral = lib.review_output(&outs[2].1, Some(Verdict::Neutral)).unwrap();
    lib.review_output(&neutral, None).unwrap();
    assert_eq!(resolves(&lib, &good, Store::Output).id, outs[0].0);

    assert_eq!(lib.add_seeds(&run, 2, false).unwrap(), (0, Seeds { first: 4, count: 2 }));
    assert_eq!(lib.add_seeds(&run, 2, true).unwrap(), (1, Seeds { first: 6, count: 2 }));
    assert_eq!(lib.find_run(&run).unwrap().1.next_seed(), 8);

    let edit_mode = Mode::Edit { init: Some(ImageRef::output(outs[0].0.clone())), strength: 0.6 };
    let edit_run = lib.create_run(&project, new_run(vec![], edit_mode, 1)).unwrap();
    let (_, edit_def) = lib.find_run(&edit_run).unwrap();
    let (edited, _) = lib
        .write_output(
            &edit_run,
            0,
            0,
            &sdserver_png(9),
            &recipe(&edit_def, 0, Some(ImageRef::output(outs[0].0.clone()))),
        )
        .unwrap();
    let inputs = lib.recipe(&edited).unwrap().unwrap().inputs;
    assert_eq!(
        inputs.iter().map(|i| (i.role, i.image.clone())).collect::<Vec<_>>(),
        [(InputRole::Start, ImageRef::output(outs[0].0.clone()))]
    );
    assert_eq!(inputs[0].original_name, None);
    lib.set_cover(&project, Some(ImageRef::output(outs[0].0.clone()))).unwrap();

    // ---- collections, output → input
    lib.create_folder(Tree::Collections, Path::new(""), Some("best")).unwrap();
    let in_coll = lib.add_to_collection(Path::new("best"), &outs[0].0).unwrap();
    assert_eq!(lib.add_to_collection(Path::new("best"), &outs[0].0).unwrap(), in_coll);
    let from_out = lib.idea_from_output(&outs[1].0, true, IdeaLoc::Unreviewed).unwrap();
    let idea = lib.load_idea(&from_out).unwrap();
    assert_eq!(idea.text, "a fox in a café");
    assert_eq!(idea.source, IdeaSource::FromOutput { image: ImageRef::output(outs[1].0.clone()) });

    // ---- project tree: copy (setup only), move with a clashing name
    let copied = lib.copy_node(Tree::Projects, Path::new("clients/fox"), Path::new("")).unwrap();
    assert_eq!(copied, Path::new("fox"));
    let ix = lib.scan().unwrap();
    let copy = ix.projects.iter().find(|p| p.dir == Path::new("projects/fox")).unwrap();
    assert_ne!(copy.project.id, project);
    assert_eq!(copy.runs.len(), 2);
    assert!(copy.runs.iter().all(|r| r.revisions.is_empty()) && copy.outputs.is_empty());
    let moved = lib.move_node(Tree::Projects, Path::new("clients/fox"), Path::new("")).unwrap();
    assert_eq!(moved, Path::new("fox 2"));
    let ix = lib.scan().unwrap();
    assert!(ix.problems.is_empty(), "{:?}", ix.problems);
    let p = ix.project(&project).unwrap();
    assert_eq!(p.outputs.len(), 5);
    for o in &p.outputs {
        resolves(&lib, &o.path, Store::Output);
    }
    assert_eq!(lib.find_project(&project).unwrap(), root.join("projects/fox 2"));

    let report = common::verify(&root);
    assert!(report.trim_end().ends_with("problems: none"), "{report}");

    // ---- delete + clean up
    lib.delete_node(Tree::Projects, Path::new("fox")).unwrap();
    assert!(tmp.path().join("trash/fox").join("project.rsn").exists());
    assert!(lib.unreferenced().unwrap().is_empty());
    lib.delete_discarded_idea(&junk).unwrap();
    lib.delete_node(Tree::Projects, Path::new("fox 2")).unwrap();
    let mut gone = lib.unreferenced().unwrap();
    gone.sort();
    let mut expect = vec![
        ImageRef::input(sticker.clone()),
        ImageRef::output(outs[2].0.clone()),
        ImageRef::output(outs[3].0.clone()),
        ImageRef::output(edited.clone()),
    ];
    expect.sort();
    // outs[0]: in a collection; outs[1]: an idea's FromOutput source; the
    // photo: an idea image and a ref in outs[0]'s recipe.
    assert_eq!(gone, expect);
    let mut cleaned = lib.clean_up().unwrap();
    cleaned.sort();
    assert_eq!(cleaned, gone);
    assert!(!lib.image_path(&ImageRef::output(edited)).exists());
    assert!(tmp.path().join("trash").join(format!("{sticker}.png")).exists());
    let report = common::verify(&root);
    assert!(report.trim_end().ends_with("problems: none"), "{report}");

    // ---- reopen: counters continue, lock released
    drop(lib);
    let mut lib = Library::open(&root).unwrap();
    assert!(lib.problems().is_empty(), "{:?}", lib.problems());
    lib.set_clock(t0);
    let next = lib.new_idea(NewIdea::text("after reopen", "")).unwrap();
    let ix = lib.scan().unwrap();
    assert_eq!(ix.ideas.iter().filter(|e| e.idea.id == next).count(), 1);
    assert!(ix.ideas.iter().all(|e| e.idea.id <= next));
}

#[test]
fn copy_mode_library() {
    let (_tmp, mut lib) = setup(Some(LinkMode::Copy));
    let photo = lib.import_bytes(&jpeg_bytes(picture(20, 20, 3)), &original("a.jpg")).unwrap().image;
    let idea = lib.new_idea(NewIdea { images: vec![photo.clone()], ..NewIdea::text("A", "") }).unwrap();
    let (project, _) = lib.create_project(Path::new(""), None, std::slice::from_ref(&idea)).unwrap();
    let run = lib.create_run(&project, new_run(vec![], Mode::Generate, 1)).unwrap();
    let (_, def) = lib.find_run(&run).unwrap();
    let (out, at) = lib.write_output(&run, 0, 0, &sdserver_png(1), &recipe(&def, 0, None)).unwrap();
    assert!(!fsutil::is_symlink(&lib.path(&at)));
    let reviewed = lib.review_output(&at, Some(Verdict::Good)).unwrap();
    lib.create_folder(Tree::Projects, Path::new(""), Some("deep")).unwrap();
    lib.create_folder(Tree::Projects, Path::new("deep"), Some("er")).unwrap();
    let moved = lib.move_node(Tree::Projects, Path::new("new project"), Path::new("deep/er")).unwrap();
    assert_eq!(moved, Path::new("deep/er/new project"));

    let ix = lib.scan().unwrap();
    assert!(ix.problems.is_empty(), "{:?}", ix.problems);
    let p = ix.project(&project).unwrap();
    assert_eq!(p.outputs[0].image, ImageRef::output(out));
    assert_eq!(p.outputs[0].verdict, Some(Verdict::Good));
    assert!(reviewed.ends_with(p.outputs[0].path.file_name().unwrap()));
    let idea_dir = &ix.idea(&idea).unwrap().dir;
    resolves(&lib, &idea_dir.join("1-a.png"), Store::Input);
    assert!(lib.unreferenced().unwrap().is_empty());
}

#[test]
fn inbox_drop_becomes_ideas() {
    let (_tmp, mut lib) = setup(None);
    let drop = lib.path("input/inbox_drop");
    fs::write(drop.join("otter.txt"), "otter with a teacup\n").unwrap();
    fs::write(drop.join("IMG_1.png"), png_bytes(DynamicImage::ImageRgb8(picture(8, 8, 5)))).unwrap();
    fs::write(drop.join("notes.pdf"), b"%PDF-1.4").unwrap();
    let ideas = lib.import_inbox_drop().unwrap();
    assert_eq!(ideas.len(), 2);
    let ix = lib.scan().unwrap();
    let texts: Vec<_> = ix.ideas.iter().map(|e| (e.idea.text.as_str(), e.idea.images.len(), &e.loc)).collect();
    assert!(texts.contains(&("otter with a teacup", 0, &IdeaLoc::Unreviewed)));
    assert!(texts.contains(&("", 1, &IdeaLoc::Unreviewed)));
    assert!(ix.ideas.iter().all(|e| e.idea.source == IdeaSource::InboxFolder));
    assert_eq!(ix.inbox_drop, [PathBuf::from("input/inbox_drop/notes.pdf")]);
    assert!(drop.join("imported/otter.txt").exists() && drop.join("imported/IMG_1.png").exists());
    let src =
        lib.source_info(&ix.ideas.iter().find(|e| !e.idea.images.is_empty()).unwrap().idea.images[0]).unwrap().unwrap();
    assert!(src.original_path.unwrap().ends_with("input/inbox_drop/IMG_1.png"));
}

#[test]
fn names_and_refusals() {
    let (_tmp, mut lib) = setup(None);
    for bad in ["", "  ", "a/b", "a\\b", ".hidden"] {
        assert!(
            matches!(lib.create_folder(Tree::Projects, Path::new(""), Some(bad)), Err(Error::InvalidName { .. })),
            "{bad:?}"
        );
    }
    lib.create_folder(Tree::Projects, Path::new(""), Some("Alpha")).unwrap();
    lib.create_folder(Tree::Projects, Path::new(""), Some("beta")).unwrap();
    assert!(matches!(lib.rename_node(Tree::Projects, Path::new("beta"), "ALPHA"), Err(Error::InvalidName { .. })));
    assert_eq!(lib.rename_node(Tree::Projects, Path::new("Alpha"), "alpha").unwrap(), Path::new("alpha"));
    assert_eq!(lib.create_folder(Tree::Projects, Path::new(""), None).unwrap(), Path::new("new folder"));
    assert_eq!(lib.create_folder(Tree::Projects, Path::new(""), None).unwrap(), Path::new("new folder 2"));
    let (_, p) = lib.create_project(Path::new("alpha"), None, &[]).unwrap();
    assert!(matches!(lib.move_node(Tree::Projects, Path::new("beta"), &p), Err(Error::Refused(_))));
    assert!(matches!(lib.move_node(Tree::Projects, Path::new(".."), Path::new("")), Err(Error::Refused(_))));
    assert_eq!(lib.move_node(Tree::Projects, &p, Path::new("alpha")).unwrap(), p, "same place: no-op");
    let idea = lib.new_idea(NewIdea::text("x", "")).unwrap();
    lib.review_idea(&idea, Verdict::Neutral).unwrap();
    assert!(matches!(lib.rename_node(Tree::InputLibrary, Path::new(idea.as_str()), "y"), Err(Error::Refused(_))));
    assert!(matches!(lib.delete_discarded_idea(&idea), Err(Error::Refused(_))));
    let discarded = lib.delete_node(Tree::InputLibrary, Path::new(idea.as_str())).unwrap();
    assert_eq!(discarded, std::slice::from_ref(&idea));
    assert_eq!(lib.find_idea(&idea).unwrap().0, IdeaLoc::Discarded);
    assert!(matches!(Library::create(lib.root(), None), Err(Error::Refused(_))), "not empty");
}

#[test]
fn drafts_are_saved_copied_and_protect_their_images() {
    let (_tmp, mut lib) = setup(None);
    let a = lib.import_bytes(&jpeg_bytes(picture(16, 16, 1)), &original("a.jpg")).unwrap().image;
    let b = lib.import_bytes(&jpeg_bytes(picture(16, 16, 2)), &original("b.jpg")).unwrap().image;
    let (project, path) = lib.create_project(Path::new(""), Some("p"), &[]).unwrap();
    let draft = GenerateDraft {
        sweep: Default::default(),
        label: "r01".into(),
        prompt: "the fox from <image1>".into(),
        negative_prompt: String::new(),
        width: 1344,
        height: 896,
        preset: "Qwen-Image 2.1".into(),
        steps: 20,
        cfg: 2.0,
        flow_shift: None,
        seeds: 4,
        first_seed: 0,
        alpha: true,
        refs: vec![ImageRef::input(a.clone())],
        init: None,
        images: vec![],
        strength: 0.75,
        source_idea: None,
    };
    lib.save_generate_draft(&project, &draft).unwrap();
    let idea_draft =
        IdeaDraftFile { title: "t".into(), text: "x".into(), tags: String::new(), images: vec![b.clone()] };
    lib.save_idea_draft(&idea_draft).unwrap();

    let ix = lib.scan().unwrap();
    assert_eq!(ix.project(&project).unwrap().draft.as_ref(), Some(&draft));
    assert_eq!(ix.idea_draft.as_ref(), Some(&idea_draft));
    assert!(lib.unreferenced().unwrap().is_empty(), "draft refs and draft images are kept");

    let copy = lib.copy_node(Tree::Projects, &path, Path::new("")).unwrap();
    let ix = lib.scan().unwrap();
    let copied = ix.projects.iter().find(|p| p.dir == Path::new("projects").join(&copy)).unwrap();
    assert_eq!(copied.draft.as_ref(), Some(&draft), "a copied project keeps its draft");

    lib.clear_generate_draft(&project).unwrap();
    lib.clear_generate_draft(&project).unwrap();
    lib.clear_idea_draft().unwrap();
    let ix = lib.scan().unwrap();
    assert!(ix.project(&project).unwrap().draft.is_none() && ix.idea_draft.is_none());
    assert_eq!(lib.unreferenced().unwrap(), [ImageRef::input(b)], "only the copy's draft still holds image a");
}

#[test]
fn smart_is_reserved_at_the_top_of_collections() {
    let tmp = tempfile::tempdir().unwrap();
    let mut lib = increart_core::Library::create(&tmp.path().join("lib"), None).unwrap();
    let c = increart_core::Tree::Collections;
    let refused = |r: increart_core::error::Result<PathBuf>| {
        let e = r.unwrap_err().to_string();
        assert!(e.contains("reserved"), "{e}");
    };
    refused(lib.create_folder(c, Path::new(""), Some("Smart")));
    // fine below the top, and in other trees
    lib.create_folder(c, Path::new(""), Some("work")).unwrap();
    lib.create_folder(c, Path::new("work"), Some("smart")).unwrap();
    lib.create_folder(increart_core::Tree::Projects, Path::new(""), Some("smart")).unwrap();
    refused(lib.move_node(c, Path::new("work/smart"), Path::new("")));
    refused(lib.copy_node(c, Path::new("work/smart"), Path::new("")));
    refused(lib.rename_node(c, Path::new("work"), "SMART"));
    assert!(!tmp.path().join("lib/collections/smart").exists() && !tmp.path().join("lib/collections/SMART").exists());
}

fn usage(lib: &Library, r: &ImageRef) -> png::UsageCounts {
    lib.usage_counts(r).unwrap()
}

fn counts(refs: u64, starts: u64) -> png::UsageCounts {
    png::UsageCounts { refs, starts }
}

#[test]
fn usage_counts_live_in_the_images() {
    let (tmp, mut lib) = setup(None);
    let mut import = |seed| {
        let bytes = png_bytes(DynamicImage::ImageRgb8(picture(8, 8, seed)));
        ImageRef { id: lib.import_bytes(&bytes, &original("a.png")).unwrap().image, store: Store::Input }
    };
    let (a, b) = (import(1), import(2));
    assert_eq!(usage(&lib, &a), counts(0, 0), "imports start at zero");
    let len = fs::metadata(lib.image_path(&a)).unwrap().len();

    let (project, _) = lib.create_project(Path::new(""), Some("p"), &[]).unwrap();
    let edit = Mode::Edit { init: Some(b.clone()), strength: 0.5 };
    // a run naming a twice counts once
    lib.create_run(&project, new_run(vec![a.clone(), a.clone(), b.clone()], edit.clone(), 2)).unwrap();
    assert_eq!(usage(&lib, &a), counts(1, 0));
    assert_eq!(usage(&lib, &b), counts(1, 1));
    assert_eq!(fs::metadata(lib.image_path(&a)).unwrap().len(), len, "patched in place");

    // a run without seeds counts when it first generates
    let later = lib.create_run(&project, new_run(vec![a.clone()], Mode::Generate, 0)).unwrap();
    assert_eq!(usage(&lib, &a), counts(1, 0));
    lib.add_seeds(&later, 2, false).unwrap();
    lib.add_seeds(&later, 2, true).unwrap();
    assert_eq!(usage(&lib, &a), counts(2, 0));

    // ever used: deleting a run doesn't lower them, and a higher count is kept
    let root = lib.root().to_path_buf();
    let path_a = lib.image_path(&a);
    let path_b = lib.image_path(&b);
    let (at, chunk, ok) = png::find_header_chunk(&path_a, png::KEY_USAGE).unwrap().unwrap();
    assert!(ok);
    assert_eq!(chunk.data, b"increart:usage\0ref=000000002 start=000000000");
    png::patch_chunk(&path_a, at, &counts(7, 0).chunk()).unwrap();
    // a torn write: new data, old CRC
    let (at_b, _, _) = png::find_header_chunk(&path_b, png::KEY_USAGE).unwrap().unwrap();
    let mut bytes = fs::read(&path_b).unwrap();
    let data = at_b as usize + 8 + "increart:usage\0ref=".len();
    bytes[data..data + 9].copy_from_slice(b"000000000");
    fs::write(&path_b, &bytes).unwrap();
    assert!(!png::find_header_chunk(&path_b, png::KEY_USAGE).unwrap().unwrap().2);
    drop(lib);

    let mut lib = Library::open(&root).unwrap();
    let torn = !png::find_header_chunk(&path_b, png::KEY_USAGE).unwrap().unwrap().2;
    assert!(torn, "open doesn't check (the GUI does, in the background)");
    lib.check_all_usage().unwrap();
    assert_eq!(usage(&lib, &a), counts(7, 0));
    assert_eq!(usage(&lib, &b), counts(1, 1), "repaired from the runs");
    assert!(png::find_header_chunk(&path_b, png::KEY_USAGE).unwrap().unwrap().2);
    drop(tmp);
}

#[test]
fn imports_record_the_original_files_times_and_mode() {
    use std::os::unix::fs::PermissionsExt;
    let (tmp, mut lib) = setup(None);
    let file = tmp.path().join("fox.png");
    fs::write(&file, png_bytes(DynamicImage::ImageRgb8(picture(8, 8, 3)))).unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o640)).unwrap();
    let id = lib.import_file(&file).unwrap().image;
    let bytes = fs::read(lib.image_path(&ImageRef { id, store: Store::Input })).unwrap();
    let source: SourceInfo = serde_json::from_str(&png::text(&bytes, png::KEY_SOURCE).unwrap().unwrap()).unwrap();
    assert_eq!(source.original_mode, Some(0o640));
    assert!(source.original_mtime.is_some() && source.original_ctime.is_some() && source.original_atime.is_some());
    assert!(source.original_uid.is_some() && source.original_gid.is_some());
}

#[test]
fn transparency_is_read_from_the_pixels() {
    let (tmp, mut lib) = setup(None);
    // Qwen's "opaque" RGBA (alpha 253) vs a cut-out with a clear background
    let rgba = |clear: bool| {
        png_bytes(DynamicImage::ImageRgba8(RgbaImage::from_fn(8, 8, |x, _| {
            Rgba([x as u8, 0, 0, if clear && x < 4 { 0 } else { 253 }])
        })))
    };
    let (project, _) = lib.create_project(Path::new(""), Some("p"), &[]).unwrap();
    let run = lib.create_run(&project, new_run(vec![], Mode::Generate, 2)).unwrap();
    let def = lib.find_run(&run).unwrap().1;
    let (opaque, _) = lib.write_output(&run, 0, 0, &rgba(false), &recipe(&def, 0, None)).unwrap();
    let (clear, _) = lib.write_output(&run, 0, 1, &rgba(true), &recipe(&def, 1, None)).unwrap();
    let input = lib.import_bytes(&rgba(true), &original("cut.png")).unwrap().image;
    let expect = [ImageRef::input(input), ImageRef::output(clear)];
    let ix = lib.scan().unwrap();
    assert_eq!(ix.transparent.iter().cloned().collect::<Vec<_>>(), expect);
    assert!(!ix.transparent(&ImageRef::output(opaque)));

    // an index from before the transparent set is rebuilt on open
    let root = lib.root().to_path_buf();
    drop(lib);
    let index = root.join(increart_core::library::PIXEL_INDEX);
    let mut old: PixelIndexFile = increart_core::rsnfmt::read(&index).unwrap();
    (old.version, old.transparent) = (1, Default::default());
    increart_core::rsnfmt::write(&index, &old).unwrap();
    let lib = Library::open(&root).unwrap();
    assert_eq!(lib.scan().unwrap().transparent.into_iter().collect::<Vec<_>>(), expect);
    drop(tmp);
}
