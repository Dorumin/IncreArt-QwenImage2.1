#!/usr/bin/env bash
# Point IncreArt's default config path at the workspace's local config, which
# survives container rebuilds (~/.config does not).
#
#   scripts/link-config.sh        # ~/.config/increart/config.rsn -> local/config.rsn
set -euo pipefail
src=$(cd "$(dirname "$0")/.." && pwd)/local/config.rsn
dst=${XDG_CONFIG_HOME:-$HOME/.config}/increart/config.rsn
[ -f "$src" ] || { echo "no $src" >&2; exit 1; }
mkdir -p "$(dirname "$dst")"
if [ -e "$dst" ] && [ ! -L "$dst" ]; then
    mv "$dst" "$dst.bak"
    echo "kept the old config as $dst.bak"
fi
ln -sfn "$src" "$dst"
echo "$dst -> $src"
