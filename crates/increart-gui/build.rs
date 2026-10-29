//! Names the commit the app is built from (`INCREART_COMMIT`): its short
//! hash, `-dirty` with uncommitted changes, and its date. Empty when built
//! outside a git checkout (e.g. from a source archive).

use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    let s = String::from_utf8(out.stdout).ok()?;
    out.status.success().then(|| s.trim().to_owned())
}

fn main() {
    // again when HEAD moves or the index changes (a commit, a checkout, a stage)
    if let Some(dir) = git(&["rev-parse", "--git-dir"]) {
        for f in ["HEAD", "index", "refs"] {
            println!("cargo:rerun-if-changed={dir}/{f}");
        }
    }
    println!("cargo:rerun-if-changed=build.rs");
    let commit = git(&["log", "-1", "--format=%h %cs"])
        .filter(|s| !s.is_empty())
        .map(|s| {
            let (hash, date) = s.split_once(' ').unwrap_or((&s, ""));
            let dirty = git(&["status", "--porcelain", "--untracked-files=no"]).is_some_and(|s| !s.is_empty());
            format!("{hash}{}, {date}", if dirty { "-dirty" } else { "" })
        })
        .unwrap_or_default();
    println!("cargo:rustc-env=INCREART_COMMIT={commit}");
}
