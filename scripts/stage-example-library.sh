#!/usr/bin/env bash
# Copies the repo's example library to <dest> as release zips ship it: each
# link as the small stand-in file holding its target that git makes where it
# can't make symlinks (every unzip tool keeps it; the app makes it a link on
# first open). Taken from git's own record of the links, so the same however
# this checkout made them (symlinks, or stand-ins on Windows).
#
#   scripts/stage-example-library.sh <dest>
set -euo pipefail

dest=${1:?destination folder}
cd "$(dirname "$0")/.."
[[ ! -e $dest ]] || { echo "$dest is in the way" >&2; exit 1; }
# links followed: Git Bash on Windows can't copy symlinks (it can't make one
# before its target is there); they are replaced below anyway
cp -RL example-library "$dest"
rm -rf "$dest/cache" "$dest/thumbs" "$dest/library.lock"
git ls-files -s -z example-library | while IFS= read -r -d '' e; do
    [[ $e == 120000\ * ]] || continue
    path=${e#*$'\t'}
    at="$dest/${path#example-library/}"
    rm -f "$at"
    git cat-file blob "HEAD:$path" > "$at"
done
test -f "$dest/library.rsn"
if find "$dest" -type l | grep .; then echo "symlinks left" >&2; exit 1; fi
