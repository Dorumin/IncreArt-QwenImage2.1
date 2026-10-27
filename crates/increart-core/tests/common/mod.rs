#![allow(dead_code)]

pub mod fake;

use std::path::{Path, PathBuf};
use std::process::Command;

/// Call first in every test: fsync is slow on overlay filesystems and tests
/// don't need durability.
pub fn init() {
    increart_core::fsutil::set_fsync(false);
}

pub fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

/// A copy of the sample library at `dir`. The generator runs once per test
/// binary; each caller gets its own copy.
pub fn sample_copy(dir: &Path) {
    init();
    static ONCE: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
    let master = ONCE.get_or_init(|| {
        let t = tempfile::tempdir().unwrap();
        make_sample(&t.path().join("lib"));
        t
    });
    increart_core::fsutil::copy_tree(&master.path().join("lib"), dir).unwrap();
}

/// Writes a fresh sample library (scripts/make-sample-library.py) into `dir`.
pub fn make_sample(dir: &Path) {
    let out = Command::new("python3")
        .arg(workspace().join("scripts/make-sample-library.py"))
        .arg(dir)
        .output()
        .expect("python3 runs");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
}

/// Runs scripts/verify-sample-library.py on `dir`; returns its summary line.
pub fn verify(dir: &Path) -> String {
    let out = Command::new("python3")
        .arg(workspace().join("scripts/verify-sample-library.py"))
        .arg(dir)
        .output()
        .expect("python3 runs");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap()
}
