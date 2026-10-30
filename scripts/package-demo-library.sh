#!/usr/bin/env bash
# Packages a library built by `demo_library build` as the example library
# that release zips ship beside the app:
#
#   scripts/package-demo-library.sh <library> <demo-sources> [out-dir]
#
# The zip, increart-example-library.zip (for every OS), holds one folder,
# "Example library", without the caches (cache/ names model files by their
# absolute paths; thumbnails are made again on open) or the lock, plus the
# sources' credits.json. Each link is a copy of its image, as some unzip
# tools (and Windows) can't make symlinks, and library.rsn names no link
# mode: the app probes one on first open, so new links are symlinks where
# they can be.
#
# The library is checked first with `increart --check-library` (the binary
# in $INCREART, else target/release or target/debug), which brings its
# metadata up to date: no problems and no seeds left to generate (they would
# be queued for whoever opens it).
#
# Upload it to the `example-library` release, which the release workflow
# downloads from:
#
#   gh release upload example-library --clobber increart-example-library.zip
set -euo pipefail

lib=${1:?library folder}
sources=${2:?demo sources folder (for credits.json)}
out=$(realpath -m "${3:-.}")
[[ -f $lib/library.rsn ]] || { echo "$lib is not a library" >&2; exit 1; }
[[ -f $sources/credits.json ]] || { echo "no credits.json in $sources" >&2; exit 1; }
grep -q 'link_mode: Symlink' "$lib/library.rsn" || { echo "expected a symlink library" >&2; exit 1; }
root=$(dirname "$(realpath "$0")")/..
increart=${INCREART:-}
for b in "$root/target/release/increart" "$root/target/debug/increart"; do
    [[ -n $increart || ! -x $b ]] || increart=$b
done
[[ -n $increart ]] || { echo "no increart binary: build it, or set INCREART" >&2; exit 1; }
report=$("$increart" --check-library "$lib") || { echo "$report" >&2; exit 1; }
echo "$report"
if grep -q 'seeds to generate' <<<"$report"; then
    echo "the library has runs still to generate: finish or cancel them" >&2
    exit 1
fi
grep -q 'metadata: 2' "$lib/library.rsn" || { echo "expected metadata 2 after the check" >&2; exit 1; }
mkdir -p "$out"

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
dest="$stage/Example library"
cp -r "$lib" "$dest"   # symlinks stay symlinks, for checking
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

# each link a copy of its image; the link mode probed on first open
find "$dest" -type l -print0 | while IFS= read -r -d '' l; do
    cp --remove-destination "$(readlink -f "$l")" "$l"
done
sed -i '/link_mode: /d' "$dest/library.rsn"

rm -f "$out/increart-example-library.zip"
(cd "$stage" && zip -qr -X "$out/increart-example-library.zip" "Example library")
du -h "$out/increart-example-library.zip"
