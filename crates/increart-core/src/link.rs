//! Links from folders into the image stores (DESIGN.md, Links): relative
//! symlinks, else hard links, else copies, fixed per library.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use crate::container;
use crate::error::{Error, IoContext, Result};
use crate::fsutil;
use crate::model::StoreFormat;
use crate::model::{ImageRef, LinkMode, Store};
use crate::png;

#[cfg(unix)]
pub fn symlink(target: &Path, at: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, at)
}

#[cfg(windows)]
pub fn symlink(target: &Path, at: &Path) -> io::Result<()> {
    std::os::windows::fs::symlink_file(target, at)
}

/// Picks the library's link mode by trying each kind in `dir` (inside the
/// library, so hard links stay on one volume). `INCREART_LINK_MODE`
/// (`symlink`, `hardlink` or `copy`) picks it instead: tests run every mode
/// this way, and it gives a library that copies cleanly to another drive.
pub fn probe(dir: &Path) -> Result<LinkMode> {
    if let Ok(m) = std::env::var("INCREART_LINK_MODE")
        && !m.is_empty()
    {
        return match m.to_ascii_lowercase().as_str() {
            "symlink" => Ok(LinkMode::Symlink),
            "hardlink" => Ok(LinkMode::Hardlink),
            "copy" => Ok(LinkMode::Copy),
            _ => Err(Error::Refused(format!("INCREART_LINK_MODE={m}: expected symlink, hardlink or copy"))),
        };
    }
    let probe = dir.join(".link-probe");
    fs::create_dir_all(&probe).at(&probe)?;
    let target = probe.join("target");
    fs::write(&target, b"probe").at(&target)?;
    let sym = probe.join("sym");
    let hard = probe.join("hard");
    let mode = if symlink(Path::new("target"), &sym).is_ok() && fs::read(&sym).is_ok_and(|b| b == b"probe") {
        LinkMode::Symlink
    } else if fs::hard_link(&target, &hard).is_ok() {
        LinkMode::Hardlink
    } else {
        LinkMode::Copy
    };
    fs::remove_dir_all(&probe).at(&probe)?;
    Ok(mode)
}

/// Creates a link at `at` to the store file `target` (absolute). Fails if
/// something exists at `at`.
pub fn create(mode: LinkMode, target: &Path, at: &Path) -> Result<()> {
    let dir = at.parent().expect("link has a parent");
    fs::create_dir_all(dir).at(dir)?;
    match mode {
        LinkMode::Symlink => {
            let rel = pathdiff::diff_paths(target, dir).expect("both absolute");
            symlink(&rel, at).at(at)
        }
        LinkMode::Hardlink => fs::hard_link(target, at).at(at),
        LinkMode::Copy => {
            if fsutil::exists(at) {
                return Err(io::Error::from(io::ErrorKind::AlreadyExists)).at(at);
            }
            fs::copy(target, at).map(drop).at(at)
        }
    }
}

/// The store image a symlink points at, from its target text (the last two
/// components, `<store>_images/<id>.png` or `.webp`), without following it.
pub fn symlink_ref(at: &Path) -> Option<ImageRef> {
    let target = fs::read_link(at).ok()?;
    let mut comps = target.components().rev();
    let file = comps.next()?;
    let store = comps.next()?;
    let (Component::Normal(file), Component::Normal(store)) = (file, store) else {
        return None;
    };
    let id = StoreFormat::of_name(file.to_str()?)?.1.parse().ok()?;
    Some(ImageRef { id, store: Store::from_dir(store.to_str()?)? })
}

/// The store image a link refers to: from the target path for a symlink,
/// from `increart:id` for a hard link or copy (whose store is `store`, as
/// known from where the link lives). `None` for a file that isn't a link.
pub fn read_ref(at: &Path, store: Store) -> Result<Option<ImageRef>> {
    if fsutil::is_symlink(at) {
        return Ok(symlink_ref(at));
    }
    match container::header_text(at, png::KEY_ID) {
        Ok(Some(id)) => Ok(Some(ImageRef { id: id.parse()?, store })),
        Ok(None) | Err(Error::Format { .. }) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Recreates every symlink under `dir` after the directory was moved (their
/// relative targets are only right at the old depth). Other kinds of link
/// don't need it.
pub fn refresh_symlinks(root: &Path, dir: &Path) -> Result<()> {
    let paths = if fsutil::is_symlink(dir) { vec![dir.to_path_buf()] } else { fsutil::walk(dir)? };
    for p in paths {
        if !fsutil::is_symlink(&p) {
            continue;
        }
        if let Some(r) = symlink_ref(&p) {
            let target = r.path(root);
            let want = pathdiff::diff_paths(&target, p.parent().unwrap()).unwrap();
            if fs::read_link(&p).at(&p)? != want {
                fs::remove_file(&p).at(&p)?;
                symlink(&want, &p).at(&p)?;
            }
        }
    }
    Ok(())
}

/// Moves a link or a directory holding links, then fixes symlink targets.
pub fn move_with_links(root: &Path, from: &Path, to: &Path) -> Result<()> {
    if fsutil::exists(to) {
        return Err(Error::Refused(format!("{} already exists", to.display())));
    }
    if let Some(dir) = to.parent() {
        fs::create_dir_all(dir).at(dir)?;
    }
    fs::rename(from, to).at(from)?;
    refresh_symlinks(root, to)
}

/// Absolute form of a library path, for diffing.
pub fn absolute(p: &Path) -> Result<PathBuf> {
    std::path::absolute(p).at(p)
}
