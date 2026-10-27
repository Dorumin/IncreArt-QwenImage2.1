//! Prompt presets (DESIGN.md, Prompt presets): ready-made prompts for the
//! Generate form, grouped by task, shared by every library. The built-in
//! ones are `default-presets.rsn` (embedded); the user's are files in the
//! `prompt-presets` folder next to `config.rsn`, one per preset in a folder
//! per task (and group), over the built-in ones.
//!
//! A prompt names reference images as `<imageN>` and leaves blanks for the
//! user as `{slot}` (e.g. `{scene}`).

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use crate::error::{IoContext, Result};
use crate::{fsutil, rsnfmt};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename = "PromptPresets", default)]
pub struct PromptPresets {
    pub tasks: Vec<PromptTask>,
}

/// What a group of presets does: Modify style, Remove background, ….
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename = "PromptTask", default)]
pub struct PromptTask {
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub presets: Vec<PromptPreset>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename = "PromptPreset", default)]
pub struct PromptPreset {
    pub name: String,
    /// A heading within the task (empty: none).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub group: String,
    /// What the name means, loosely, and what you get.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub prompt: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub negative: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub favourite: bool,
}

impl PromptPresets {
    /// The `prompt-presets` folder next to the config file.
    pub fn dir_beside(config: &Path) -> PathBuf {
        config.with_file_name("prompt-presets")
    }

    /// The presets in `dir`, the built-in ones' copy there made current;
    /// with the preset files that didn't load (left alone), as messages. A
    /// `prompt-presets.rsn` beside it (the presets before the folder) is
    /// moved into a new folder and renamed `prompt-presets.rsn.old`.
    pub fn load_dir(dir: &Path) -> Result<(PromptPresets, Vec<String>)> {
        let old = dir.with_file_name("prompt-presets.rsn");
        let fresh = !dir.exists();
        std::fs::create_dir_all(dir).at(dir)?;
        write_defaults(dir)?;
        if fresh && old.is_file() {
            let list: PromptPresets = rsnfmt::read(&old)?;
            list.save_dir(dir)?;
            std::fs::rename(&old, old.with_extension("rsn.old")).at(&old)?;
        }
        let state_path = dir.join(STATE_FILE);
        let state: UserState = if state_path.exists() { rsnfmt::read(&state_path)? } else { UserState::default() };
        let mut user = vec![];
        let mut bad = vec![];
        for task_dir in fsutil::subdirs(dir)? {
            let folder = task_dir.file_name().unwrap_or_default().to_string_lossy().into_owned();
            for (group, path) in task_files(&task_dir)? {
                match read_preset_file(&path) {
                    Ok(f) => {
                        let stem = || path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
                        let task = if f.task.is_empty() { folder.clone() } else { f.task };
                        let preset = PromptPreset {
                            name: if f.name.is_empty() { stem() } else { f.name },
                            group: if f.group.is_empty() { group } else { f.group },
                            description: f.description,
                            prompt: f.prompt,
                            negative: f.negative,
                            favourite: false,
                        };
                        user.push((task, preset));
                    }
                    Err(e) => bad.push(format!("prompt preset not loaded: {e}")),
                }
            }
        }
        Ok((merge(&builtin(), &state, &user), bad))
    }

    /// Writes the presets into `dir` as changes to the built-in ones: a
    /// file per changed or added preset (those of files that no longer
    /// match are removed; files that don't load are left alone), and
    /// `user-state.rsn`.
    pub fn save_dir(&self, dir: &Path) -> Result<()> {
        let (state, user) = split(self, &builtin());
        write_defaults(dir)?;
        let state_path = dir.join(STATE_FILE);
        if state == UserState::default() {
            if state_path.exists() {
                std::fs::remove_file(&state_path).at(&state_path)?;
            }
        } else {
            rsnfmt::write(&state_path, &state)?;
        }
        let mut want = std::collections::HashMap::new();
        for (task, p) in &user {
            let mut folder = dir.join(file_name(task));
            if !p.group.is_empty() {
                folder.push(file_name(&p.group));
            }
            let base = file_name(&p.name);
            let mut path = folder.join(format!("{base}.rsn"));
            // a file that doesn't load isn't written over
            let mut n = 1;
            while want.contains_key(&path) || (path.exists() && read_preset_file(&path).is_err()) {
                n += 1;
                path = folder.join(format!("{base} ({n}).rsn"));
            }
            let file = PresetFile {
                name: p.name.clone(),
                task: task.clone(),
                group: p.group.clone(),
                description: p.description.clone(),
                prompt: p.prompt.clone(),
                negative: p.negative.clone(),
            };
            want.insert(path, file);
        }
        for task_dir in fsutil::subdirs(dir)? {
            for (_, path) in task_files(&task_dir)? {
                if !want.contains_key(&path) && read_preset_file(&path).is_ok() {
                    std::fs::remove_file(&path).at(&path)?;
                }
            }
        }
        for (path, file) in &want {
            if read_preset_file(path).ok().as_ref() != Some(file) {
                rsnfmt::write(path, file)?;
            }
        }
        // folders left empty (deepest first)
        let mut dirs: Vec<_> =
            fsutil::walk(dir)?.into_iter().filter(|p| p.is_dir() && !fsutil::is_symlink(p)).collect();
        dirs.reverse();
        for d in dirs {
            if std::fs::read_dir(&d).at(&d)?.next().is_none() {
                std::fs::remove_dir(&d).at(&d)?;
            }
        }
        Ok(())
    }

    /// As saved and loaded again: the user's presets where [`merge`] puts
    /// them.
    pub fn normalized(&self) -> PromptPresets {
        let defaults = builtin();
        let (state, user) = split(self, &defaults);
        merge(&defaults, &state, &user)
    }

    pub fn count(&self) -> usize {
        self.tasks.iter().map(|t| t.presets.len()).sum()
    }
}

/// The `{slot}` blanks of a prompt, as byte ranges (braces included). A
/// slot is a short name of letters, digits, spaces, `-` and `_`.
pub fn slots(prompt: &str) -> Vec<std::ops::Range<usize>> {
    let mut out = vec![];
    let mut from = 0;
    while let Some(s) = prompt[from..].find('{').map(|i| from + i) {
        let Some(e) = prompt[s + 1..].find(['{', '}']).map(|i| s + 1 + i) else { break };
        let inner = &prompt[s + 1..e];
        let ok = prompt.as_bytes()[e] == b'}'
            && !inner.trim().is_empty()
            && inner.len() <= 40
            && inner.chars().all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'));
        if ok {
            out.push(s..e + 1);
            from = e + 1;
        } else {
            from = e;
        }
    }
    out
}

/// A style sweep's settings (the Generate form in Sweep mode): which
/// presets get a run each.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename = "SweepSettings", default)]
pub struct Sweep {
    pub on: bool,
    pub source: SweepSource,
    /// 0: every preset of the source; else this many of them at random.
    #[serde(skip_serializing_if = "is_zero")]
    pub random: usize,
    /// Makes the random pick repeatable (**Reroll** changes it).
    #[serde(skip_serializing_if = "is_zero")]
    pub random_seed: u64,
    /// Ticked in the Presets window (source Selected).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub selected: Vec<PresetKey>,
    /// Values for the presets' `{slot}`s, by slot (`{scene}` → `scene`).
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub slots: std::collections::BTreeMap<String, String>,
    /// Taken out of the pick, each leaving the rest of it as it was.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub removed: Vec<Removed>,
    /// What gets a run each: a prompt preset (a style), or a setting's value.
    #[serde(skip_serializing_if = "is_zero")]
    pub vary: SweepVary,
    /// A setting sweep's values as typed ("8, 12, 20").
    #[serde(skip_serializing_if = "String::is_empty")]
    pub values: String,
    /// A model sweep's model presets, by name.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<String>,
}

/// What a sweep varies, run to run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SweepVary {
    /// The prompt: a prompt preset per run.
    #[default]
    Style,
    Steps,
    Cfg,
    FlowShift,
    /// img2img strength (with a start image).
    Strength,
    /// The model preset (each with its own steps, CFG and flow shift).
    Model,
}

impl SweepVary {
    pub const EVERY: [SweepVary; 6] = [
        SweepVary::Style,
        SweepVary::Steps,
        SweepVary::Cfg,
        SweepVary::FlowShift,
        SweepVary::Strength,
        SweepVary::Model,
    ];

    pub fn name(self) -> &'static str {
        match self {
            SweepVary::Style => "Style",
            SweepVary::Steps => "Steps",
            SweepVary::Cfg => "CFG",
            SweepVary::FlowShift => "Flow shift",
            SweepVary::Strength => "Strength",
            SweepVary::Model => "Model",
        }
    }

    /// The [`crate::model::SweepParam`] name (empty for Style).
    pub fn key(self) -> &'static str {
        match self {
            SweepVary::Style => "",
            SweepVary::Steps => "steps",
            SweepVary::Cfg => "cfg",
            SweepVary::FlowShift => "flow_shift",
            SweepVary::Strength => "strength",
            SweepVary::Model => "model",
        }
    }
}

/// A preset taken out of a sweep's pick.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Removed {
    pub preset: PresetKey,
    /// A random pick takes the next preset in its order instead (else the
    /// sweep has one style fewer).
    pub replace: bool,
}

fn is_zero<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SweepSource {
    /// The presets ticked in the Presets window.
    #[default]
    Selected,
    Favourites,
    /// Every preset of a task.
    Task(String),
    /// Every preset of a group within a task.
    Group(String, String),
}

/// A preset by task and name (survives reordering and edits elsewhere).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename = "Preset")]
pub struct PresetKey {
    pub task: String,
    pub name: String,
}

impl Sweep {
    pub fn is_default(&self) -> bool {
        *self == Sweep::default()
    }
}

/// The presets a sweep makes runs for, in list order: the source's, or
/// `random` of them picked with `random_seed`, less the removed ones.
pub fn sweep_presets<'a>(list: &'a PromptPresets, sweep: &Sweep) -> Vec<(&'a PromptTask, &'a PromptPreset)> {
    let all = list.tasks.iter().flat_map(|t| t.presets.iter().map(move |p| (t, p)));
    let v: Vec<_> = all
        .filter(|(t, p)| match &sweep.source {
            SweepSource::Selected => sweep.selected.iter().any(|k| k.task == t.name && k.name == p.name),
            SweepSource::Favourites => p.favourite,
            SweepSource::Task(task) => t.name == *task,
            SweepSource::Group(task, group) => t.name == *task && p.group == *group,
        })
        .collect();
    let removed = |(t, p): &(&PromptTask, &PromptPreset)| {
        sweep.removed.iter().find(|r| r.preset.task == t.name && r.preset.name == p.name)
    };
    if sweep.random == 0 || sweep.random >= v.len() {
        return v.into_iter().filter(|x| removed(x).is_none()).collect();
    }
    // a Fisher–Yates shuffle with a splitmix64 stream; its first `random`
    // are the pick, a replaced preset handing its place to the next one
    let mut x = sweep.random_seed;
    let mut next = move || {
        x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = x;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    };
    let mut idx: Vec<usize> = (0..v.len()).collect();
    for i in 0..idx.len() {
        let j = i + (next() % (idx.len() - i) as u64) as usize;
        idx.swap(i, j);
    }
    let mut keep = vec![];
    let mut places = sweep.random;
    for i in idx {
        if places == 0 {
            break;
        }
        match removed(&v[i]) {
            Some(r) if r.replace => {}
            Some(_) => places -= 1,
            None => {
                keep.push(i);
                places -= 1;
            }
        }
    }
    keep.sort();
    keep.into_iter().map(|i| v[i]).collect()
}

/// Which of these presets have this `{slot}` (name without braces).
pub fn needing_slot<'a>(
    chosen: &[(&'a PromptTask, &'a PromptPreset)],
    slot: &str,
) -> Vec<(&'a PromptTask, &'a PromptPreset)> {
    chosen.iter().filter(|(_, p)| slot_names([p.prompt.as_str()]).iter().any(|n| n == slot)).copied().collect()
}

/// A sweep run's prompt: the preset's prompt at the template's `{preset}`
/// (the preset's own prompt when the template has none), then the slots
/// filled in.
pub fn sweep_prompt(template: &str, preset: &str, slots: &std::collections::BTreeMap<String, String>) -> String {
    let text = if template.contains(PRESET_SLOT) { template.replace(PRESET_SLOT, preset) } else { preset.to_string() };
    fill_slots(text, slots)
}

/// `text` with its `{slot}`s filled in (those with a value).
pub fn fill_slots(mut text: String, slots: &std::collections::BTreeMap<String, String>) -> String {
    for (k, v) in slots {
        if !v.trim().is_empty() {
            text = text.replace(&format!("{{{k}}}"), v.trim());
        }
    }
    text
}

/// Where a sweep's template takes each preset's prompt.
pub const PRESET_SLOT: &str = "{preset}";

/// The `{slot}` names (without braces) of these texts, first-seen order,
/// `{preset}` left out.
pub fn slot_names<'a>(texts: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for t in texts {
        for r in slots(t) {
            let name = t[r.start + 1..r.end - 1].to_string();
            if name != "preset" && !out.contains(&name) {
                out.push(name);
            }
        }
    }
    out
}

const DEFAULTS: &str = include_str!("../default-presets.rsn");

/// The presets shipped with IncreArt (and **Restore defaults**), from
/// `default-presets.rsn`.
pub fn builtin() -> PromptPresets {
    static BUILTIN: LazyLock<PromptPresets> =
        LazyLock::new(|| rsnfmt::from_str(DEFAULTS).expect("default-presets.rsn is valid"));
    BUILTIN.clone()
}

/// What isn't in the user's preset files: `user-state.rsn` in the presets
/// folder.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename = "UserState", default)]
pub struct UserState {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub favourites: Vec<PresetKey>,
    /// Built-in presets deleted.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hidden: Vec<PresetKey>,
    /// Tasks that aren't built in, have a changed description, or are kept
    /// with no presets.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tasks: Vec<UserTask>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename = "Task", default)]
pub struct UserTask {
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
}

/// A user preset's file, `<task>/[<group>/]<name>.rsn`. A file dropped in
/// by hand may leave out the task and group: its folders give them.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename = "PromptPreset", default)]
struct PresetFile {
    name: String,
    task: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    group: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    description: String,
    prompt: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    negative: String,
}

const DEFAULTS_FILE: &str = "default-presets.rsn";
const STATE_FILE: &str = "user-state.rsn";

/// `list` as changes to `defaults`: the presets that differ from theirs or
/// aren't among them (by task), and the rest.
pub fn split(list: &PromptPresets, defaults: &PromptPresets) -> (UserState, Vec<(String, PromptPreset)>) {
    let mut state = UserState::default();
    let mut user = vec![];
    for t in &list.tasks {
        let dt = defaults.tasks.iter().find(|d| d.name == t.name);
        if dt.is_none_or(|d| d.description != t.description) || t.presets.is_empty() {
            state.tasks.push(UserTask { name: t.name.clone(), description: t.description.clone() });
        }
        for p in &t.presets {
            let key = PresetKey { task: t.name.clone(), name: p.name.clone() };
            if p.favourite {
                state.favourites.push(key);
            }
            let p = PromptPreset { favourite: false, ..p.clone() };
            if dt.and_then(|d| d.presets.iter().find(|d| d.name == p.name)) != Some(&p) {
                user.push((t.name.clone(), p));
            }
        }
    }
    for d in &defaults.tasks {
        let t = list.tasks.iter().find(|t| t.name == d.name);
        for p in &d.presets {
            if t.is_none_or(|t| !t.presets.iter().any(|x| x.name == p.name)) {
                state.hidden.push(PresetKey { task: d.name.clone(), name: p.name.clone() });
            }
        }
    }
    (state, user)
}

/// `defaults` with the user's changes. A user preset takes the place of the
/// built-in one of its name (when in the same group), else comes after its
/// group's last preset (or at the end), in name order; user tasks come after
/// the built-in ones, in name order.
pub fn merge(defaults: &PromptPresets, state: &UserState, user: &[(String, PromptPreset)]) -> PromptPresets {
    let lower = |s: &str| s.to_lowercase();
    let mut list = defaults.clone();
    for t in &mut list.tasks {
        t.presets.retain(|p| !state.hidden.iter().any(|k| k.task == t.name && k.name == p.name));
    }
    let mut extra: Vec<&str> =
        state.tasks.iter().map(|t| t.name.as_str()).chain(user.iter().map(|(t, _)| t.as_str())).collect();
    extra.retain(|n| !defaults.tasks.iter().any(|d| d.name == *n));
    extra.sort_by_key(|n| lower(n));
    extra.dedup();
    list.tasks.extend(extra.into_iter().map(|n| PromptTask { name: n.to_string(), ..Default::default() }));
    for u in &state.tasks {
        if let Some(t) = list.tasks.iter_mut().find(|t| t.name == u.name) {
            t.description = u.description.clone();
        }
    }
    let mut user: Vec<_> = user.iter().collect();
    user.sort_by_key(|(_, p)| (lower(&p.group), lower(&p.name)));
    for (task, p) in user {
        let t = list.tasks.iter_mut().find(|t| t.name == *task).expect("added above");
        let p = PromptPreset { favourite: false, ..p.clone() };
        match t.presets.iter().position(|x| x.name == p.name) {
            Some(i) if t.presets[i].group == p.group => t.presets[i] = p,
            old => {
                if let Some(i) = old {
                    t.presets.remove(i);
                }
                let at = t.presets.iter().rposition(|x| x.group == p.group).map_or(t.presets.len(), |i| i + 1);
                t.presets.insert(at, p);
            }
        }
    }
    list.tasks.retain(|t| !t.presets.is_empty() || state.tasks.iter().any(|u| u.name == t.name));
    for k in &state.favourites {
        let p =
            list.tasks.iter_mut().filter(|t| t.name == k.task).flat_map(|t| &mut t.presets).find(|p| p.name == k.name);
        if let Some(p) = p {
            p.favourite = true;
        }
    }
    list
}

/// A file or folder name for `name`: characters a file system may refuse
/// become `-`.
fn file_name(name: &str) -> String {
    let s: String = name.chars().map(|c| if c.is_control() || r#"/\:*?"<>|"#.contains(c) { '-' } else { c }).collect();
    let s = s.trim().trim_end_matches('.').to_string();
    if s.is_empty() || s.starts_with('.') { format!("_{s}") } else { s }
}

fn read_preset_file(path: &Path) -> Result<PresetFile> {
    rsnfmt::read(path)
}

/// The `.rsn` files of a task folder, with their group folder's name.
fn task_files(task_dir: &Path) -> Result<Vec<(String, PathBuf)>> {
    let mut out = vec![];
    for p in fsutil::walk(task_dir)? {
        let Ok(rel) = p.strip_prefix(task_dir) else { continue };
        let depth = rel.components().count();
        if p.extension().is_some_and(|e| e == "rsn") && !fsutil::hidden(&p) && depth <= 2 && p.is_file() {
            let group = if depth == 2 { rel.parent().unwrap().to_string_lossy().into_owned() } else { String::new() };
            out.push((group, p));
        }
    }
    Ok(out)
}

/// Writes the built-in presets' copy if it isn't current.
fn write_defaults(dir: &Path) -> Result<()> {
    let path = dir.join(DEFAULTS_FILE);
    if std::fs::read_to_string(&path).ok().as_deref() != Some(DEFAULTS) {
        fsutil::write_atomic(&path, DEFAULTS.as_bytes())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voxel_asks_for_3d_and_not_pixel_art() {
        let b = builtin();
        let modify = b.tasks.iter().find(|t| t.name == "Modify style").unwrap();
        let voxel = modify.presets.iter().find(|p| p.name == "Voxel").unwrap();
        assert!(voxel.prompt.contains("3D voxel render") && !voxel.prompt.contains("isometric"));
        assert!(voxel.prompt.ends_with("Keep the subject, pose and identity unchanged."), "the camera may change");
        assert_eq!(voxel.negative, "pixel art, 2D, flat, sprite, pixelated");
        let robot = modify.presets.iter().find(|p| p.name == "Animatronic").unwrap();
        assert!(robot.prompt.contains("physical animatronic figure"));
        assert!(robot.prompt.ends_with("Keep the subject, pose and identity unchanged."));
        assert!(robot.negative.starts_with("drawing, illustration, 2D"));
        let other = modify.presets.iter().find(|p| p.name == "Claymation").unwrap();
        assert!(
            other.prompt.ends_with("Keep the composition, pose and identity unchanged.") && other.negative.is_empty()
        );
    }

    #[test]
    fn builtin_set() {
        let p = builtin();
        assert_eq!(p.tasks.len(), 9);
        assert!(p.count() >= 90, "{}", p.count());
        assert_eq!(p.tasks[0].presets.len(), 69);
        for t in &p.tasks {
            for x in &t.presets {
                assert!(!x.description.is_empty() && x.prompt.contains("<image1>"), "{}", x.name);
            }
        }
    }

    #[test]
    fn slot_ranges() {
        let p = "Add {object} to <image1>, {not a slot!} and {colour}{";
        let s: Vec<_> = slots(p).into_iter().map(|r| &p[r]).collect();
        assert_eq!(s, ["{object}", "{colour}"]);
        assert!(slots("no {} slots {").is_empty());
    }

    #[test]
    fn sweeps_pick_and_fill() {
        let list = builtin();
        let mut sw =
            Sweep { source: SweepSource::Group("Modify style".into(), "3D & craft".into()), ..Default::default() };
        let names = |sw: &Sweep| sweep_presets(&list, sw).iter().map(|(_, p)| p.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&sw).len(), 18);
        sw.random = 4;
        sw.random_seed = 7;
        let a = names(&sw);
        assert_eq!(a.len(), 4);
        assert_eq!(a, names(&sw), "repeatable");
        sw.random_seed = 8;
        assert_ne!(a, names(&sw));
        let all = names(&Sweep { random: 0, ..sw.clone() });
        let pos: Vec<_> = a.iter().map(|n| all.iter().position(|x| x == n).unwrap()).collect();
        assert!(pos.windows(2).all(|w| w[0] < w[1]), "list order");

        sw = Sweep { selected: vec![PresetKey { task: "Text".into(), name: "Add sign".into() }], ..Default::default() };
        assert_eq!(names(&sw), ["Add sign"]);
        let mut slots = std::collections::BTreeMap::new();
        slots.insert("text".to_string(), " OPEN ".to_string());
        let p = sweep_presets(&list, &sw)[0].1;
        assert_eq!(sweep_prompt("", &p.prompt, &slots), p.prompt.replace("{text}", "OPEN"));
        assert_eq!(sweep_prompt("a fox, {preset}", "watercolour", &slots), "a fox, watercolour");
        assert_eq!(slot_names(["{preset} in {scene}", "{text} {scene}"]), ["scene", "text"]);
    }

    #[test]
    fn sweep_removals_keep_the_rest() {
        let list = builtin();
        let key = |n: &str| PresetKey { task: "Modify style".into(), name: n.into() };
        let names = |sw: &Sweep| sweep_presets(&list, sw).iter().map(|(_, p)| p.name.clone()).collect::<Vec<_>>();
        let mut sw = Sweep { source: SweepSource::Task("Modify style".into()), random: 10, ..Default::default() };
        // a seed whose pick has Double exposure
        while !names(&sw).iter().any(|n| n == "Double exposure") {
            sw.random_seed += 1;
        }
        let a = names(&sw);
        let rest: Vec<_> = a.iter().filter(|n| *n != "Double exposure").cloned().collect();

        sw.removed = vec![Removed { preset: key("Double exposure"), replace: false }];
        assert_eq!(names(&sw), rest, "omitted");

        sw.removed[0].replace = true;
        let b = names(&sw);
        assert_eq!(b.len(), 10);
        assert!(!b.iter().any(|n| n == "Double exposure"));
        assert!(rest.iter().all(|n| b.contains(n)), "replaced, the rest kept");
        let chosen = sweep_presets(&list, &sw);
        assert!(slot_names(chosen.iter().map(|(_, p)| p.prompt.as_str())).is_empty());

        sw.random = 0;
        assert_eq!(names(&sw).len(), 68, "all but the removed one");
        let chosen = sweep_presets(&list, &Sweep { removed: vec![], ..sw.clone() });
        let need = needing_slot(&chosen, "scene");
        assert_eq!(need.iter().map(|(_, p)| p.name.as_str()).collect::<Vec<_>>(), ["Double exposure"]);
    }

    #[test]
    fn the_defaults_load_and_split_to_nothing() {
        let b = builtin();
        assert_eq!(split(&b, &b), (UserState::default(), vec![]));
        assert_eq!(merge(&b, &UserState::default(), &[]), b);
    }

    #[test]
    fn a_folder_of_changes_over_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let folder = PromptPresets::dir_beside(&dir.path().join("config.rsn"));
        let (p, bad) = PromptPresets::load_dir(&folder).unwrap();
        assert_eq!((p, bad.len()), (builtin(), 0));
        assert_eq!(std::fs::read_to_string(folder.join(DEFAULTS_FILE)).unwrap(), DEFAULTS);
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 1, "only the defaults' copy");

        // a favourite, an edit, a deletion, a new preset, a new task
        let mut p = builtin();
        let modify = &mut p.tasks[0].presets;
        let clay = modify.iter().position(|x| x.name == "Claymation").unwrap();
        modify[clay].favourite = true;
        modify[clay].prompt.push_str(" Extra.");
        modify.retain(|x| x.name != "Voxel");
        let mine = PromptPreset {
            name: "Mine: v1".into(),
            group: "3D & craft".into(),
            prompt: "x".into(),
            ..Default::default()
        };
        modify.push(mine);
        p.tasks.push(PromptTask { name: "Odd / task".into(), description: "d".into(), presets: vec![] });
        let normal = p.normalized();
        p.save_dir(&folder).unwrap();
        let (back, bad) = PromptPresets::load_dir(&folder).unwrap();
        assert!(bad.is_empty(), "{bad:?}");
        assert_eq!(back, normal);
        let modify = &back.tasks[0].presets;
        let at = |n: &str| modify.iter().position(|x| x.name == n).unwrap();
        assert!(modify[at("Claymation")].favourite && modify[at("Claymation")].prompt.ends_with("Extra."));
        assert_eq!(at("Claymation"), clay, "an edited one keeps its place");
        assert!(!modify.iter().any(|x| x.name == "Voxel"));
        assert_eq!(modify[at("Mine: v1")].group, "3D & craft");
        assert_eq!(modify[at("Mine: v1") - 1].group, "3D & craft", "after its group's others");
        assert_eq!(back.tasks.last().unwrap().name, "Odd / task");
        assert!(folder.join("Modify style/3D & craft/Mine- v1.rsn").is_file());
        assert!(folder.join("Modify style/3D & craft/Claymation.rsn").is_file());

        // a file dropped in by hand: task and group from its folders; one
        // that doesn't load is reported and kept
        let drop = folder.join("Modify style/Pasta/Fusilli.rsn");
        std::fs::create_dir_all(drop.parent().unwrap()).unwrap();
        std::fs::write(&drop, "PromptPreset { name: \"Fusilli\", prompt: \"twirls\" }\n").unwrap();
        std::fs::write(folder.join("Modify style/broken.rsn"), "nope").unwrap();
        let (back, bad) = PromptPresets::load_dir(&folder).unwrap();
        assert_eq!(bad.len(), 1);
        let f = back.tasks[0].presets.iter().find(|x| x.name == "Fusilli").unwrap();
        assert_eq!((f.group.as_str(), f.prompt.as_str()), ("Pasta", "twirls"));

        // back to the defaults: only the copy and the broken file are left
        builtin().save_dir(&folder).unwrap();
        assert_eq!(PromptPresets::load_dir(&folder).unwrap().0, builtin());
        assert!(folder.join("Modify style/broken.rsn").is_file());
        assert!(!folder.join(STATE_FILE).exists() && !folder.join("Modify style/Pasta").exists());
    }

    #[test]
    fn the_old_file_moves_into_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let folder = PromptPresets::dir_beside(&dir.path().join("config.rsn"));
        let mut p = builtin();
        p.tasks[1].presets[0].favourite = true;
        p.tasks.truncate(2);
        rsnfmt::write(&dir.path().join("prompt-presets.rsn"), &p).unwrap();
        assert_eq!(PromptPresets::load_dir(&folder).unwrap().0, p);
        assert!(!dir.path().join("prompt-presets.rsn").exists());
        assert!(dir.path().join("prompt-presets.rsn.old").is_file());
    }
}
