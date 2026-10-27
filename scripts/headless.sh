#!/usr/bin/env bash
# Run a GUI command on a private virtual X display (Xvfb), so automated GUI runs
# (screenshot walkthroughs, synthetic-input tests) never open windows on the
# desktop or steal its input.
#
#   scripts/headless.sh env INCREART_SHOTS=$PWD/shots ./target/release/increart-prototype
#
# Uses the system Xvfb when installed (add `xvfb` to the Dockerfile's apt
# list). Otherwise it falls back to Debian's xvfb packages unpacked under
# $XVFB_ROOT (fetched on first use; no root needed). That copy's Xvfb runs
# /usr/bin/xkbcomp by absolute path, so it starts inside a user + mount
# namespace with the unpacked binaries overlaid on /usr/bin.
#
# The server listens only on the abstract socket (-nolisten unix), so nothing
# is written to the host's /tmp/.X11-unix.
set -euo pipefail

DISPLAY_NUM=${HEADLESS_DISPLAY_NUM:-93}
SCREEN=${HEADLESS_SCREEN:-1400x900x24}
XVFB_ROOT=${XVFB_ROOT:-$(cd "$(dirname "$0")/.." && pwd)/tmp/xvfb-root}
DEBS=(xvfb libxfont2 libpixman-1-0 libunwind8 libfontenc1 x11-xkb-utils xkb-data xserver-common libxkbfile1)

[ $# -gt 0 ] || { echo "usage: $0 <command> [args...]" >&2; exit 2; }

fetch_xvfb() {
    local dist debs=$XVFB_ROOT.debs
    dist=$(. /etc/os-release && echo "$VERSION_CODENAME")
    mkdir -p "$debs" "$XVFB_ROOT"
    for pkg in "${DEBS[@]}"; do
        local url=""
        for arch in amd64 all; do
            url=$(curl -sfL "https://packages.debian.org/$dist/$arch/$pkg/download" \
                | grep -o 'href="http[^"]*\.deb"' | head -1 | sed 's/^href="//; s/"$//') || true
            [ -n "$url" ] && break
        done
        [ -n "$url" ] || { echo "headless: no download for $pkg" >&2; exit 1; }
        curl -sfL -o "$debs/$(basename "$url")" "$url"
        dpkg-deb -x "$debs/$(basename "$url")" "$XVFB_ROOT"
    done
}

xvfb_args=(":$DISPLAY_NUM" -screen 0 "$SCREEN" -nolisten tcp -nolisten unix)
if command -v Xvfb >/dev/null; then
    Xvfb "${xvfb_args[@]}" >/dev/null 2>&1 &
else
    [ -x "$XVFB_ROOT/usr/bin/Xvfb" ] || fetch_xvfb
    unshare -rm sh -c '
        mount -t overlay overlay -o "lowerdir=$1/usr/bin:/usr/bin" /usr/bin
        shift
        exec "$@"' sh "$XVFB_ROOT" \
        env LD_LIBRARY_PATH="$XVFB_ROOT/usr/lib/x86_64-linux-gnu" \
        "$XVFB_ROOT/usr/bin/Xvfb" "${xvfb_args[@]}" -xkbdir "$XVFB_ROOT/usr/share/X11/xkb" >/dev/null 2>&1 &
fi
xvfb_pid=$!
trap 'kill $xvfb_pid 2>/dev/null; wait $xvfb_pid 2>/dev/null || true' EXIT

# Ready once the abstract socket @/tmp/.X11-unix/X<n> is listening.
for _ in $(seq 100); do
    grep -q "@/tmp/.X11-unix/X$DISPLAY_NUM\$" /proc/net/unix && break
    kill -0 $xvfb_pid 2>/dev/null || { echo "headless: Xvfb exited" >&2; exit 1; }
    sleep 0.1
done

DISPLAY=:$DISPLAY_NUM "$@"
