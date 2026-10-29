#!/usr/bin/env bash
# Packages a library built by `demo_library build` as the example library
# that release zips ship beside the app:
#
#   scripts/package-demo-library.sh <library> <demo-sources> [out-dir]
#
# Each zip holds one folder, "Example library", without the caches (cache/
# names model files by their absolute paths; thumbnails are made again on
# open) or the lock, plus the sources' credits.json. Two zips:
#
#   increart-example-library.zip          Linux and macOS: links kept as
#                                         symlinks
#   increart-example-library-windows.zip  Windows: each link a copy of its
#                                         image (link_mode: Copy), as
#                                         unzipping can't make symlinks there
#
# Upload both to the `example-library` release, which the release workflow
# downloads from:
#
#   gh release create example-library --latest=false --title "Example library" \
#       --notes "Bundled into each release zip." increart-example-library*.zip
#   gh release upload example-library --clobber increart-example-library*.zip
set -euo pipefail

lib=${1:?library folder}
sources=${2:?demo sources folder (for credits.json)}
out=$(realpath -m "${3:-.}")
[[ -f $lib/library.rsn ]] || { echo "$lib is not a library" >&2; exit 1; }
[[ -f $sources/credits.json ]] || { echo "no credits.json in $sources" >&2; exit 1; }
grep -q 'link_mode: Symlink' "$lib/library.rsn" || { echo "expected a symlink library" >&2; exit 1; }
mkdir -p "$out"

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
dest="$stage/unix/Example library"
mkdir -p "$stage/unix"
cp -r "$lib" "$dest"   # symlinks stay symlinks
rm -rf "$dest/cache" "$dest/thumbs" "$dest/library.lock"
mkdir -p "$dest/cache" "$dest/thumbs/input" "$dest/thumbs/output"
cp "$sources/credits.json" "$dest/credits.json"

# nothing personal left behind: no absolute paths of this machine
if grep -rl -e "$HOME" -e /mnt/ -e /workspace "$dest"; then
    echo "the files above name paths of this machine" >&2
    exit 1
fi
# every link resolves inside the library
if find "$dest" -xtype l | grep .; then
    echo "the links above are broken" >&2
    exit 1
fi

# Windows: links become copies
win="$stage/windows/Example library"
mkdir -p "$stage/windows"
cp -r "$dest" "$win"
find "$win" -type l -print0 | while IFS= read -r -d '' l; do
    cp --remove-destination "$(readlink -f "$l")" "$l"
done
sed -i 's/link_mode: Symlink/link_mode: Copy/' "$win/library.rsn"

rm -f "$out/increart-example-library.zip" "$out/increart-example-library-windows.zip"
(cd "$stage/unix" && zip -qr -y -X "$out/increart-example-library.zip" "Example library")
(cd "$stage/windows" && zip -qr -X "$out/increart-example-library-windows.zip" "Example library")
du -h "$out"/increart-example-library*.zip
