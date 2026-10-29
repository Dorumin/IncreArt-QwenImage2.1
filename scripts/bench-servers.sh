#!/usr/bin/env bash
# Benchmarks image editing (Qwen-Image 2.1, 1 MP, one reference) and prompt
# enhance (Qwen3-VL 8B) on each supported single-GPU server configuration,
# with crates/increart-core/examples/bench.rs. Results: one JSON per case
# in OUT, written up in docs/benchmark-servers.html.
#
#   scripts/bench-servers.sh [OUT]
#
# The cards' halves of the list run at once (card1 and card2: the same GPU
# model); the first case runs on both as a control. PE on the CPU runs last,
# alone. Both GPUs must be otherwise idle. Cases with a result in OUT are
# skipped (delete its JSON to rerun). Repeat images vary by under 1%, so
# configurations taking minutes per image time one image after the first.
set -euo pipefail
cd "$(dirname "$0")/.."
OUT=${1:-tmp/bench-servers}
mkdir -p "$OUT"
cargo build -q --release -p increart-core --example bench
BENCH=target/release/examples/bench
HIP=/mnt/storage/sdcpp/build-hip/bin/sd-server
VK=/mnt/storage/sdcpp/build-vulkan/bin/sd-server
CACHE='"--conditioning-cache-size","32"'

# card → sd-server HIP / Vulkan device, KoboldCpp Vulkan number
dev() { case "$1:$2" in 1:hip) echo ROCm0;; 2:hip) echo ROCm1;; 1:vk) echo Vulkan1;; 2:vk) echo Vulkan2;; 1:k) echo 1;; 2:k) echo 2;; esac; }
sd() { # id card build port flags…
    local id=$1 card=$2 build=$3 port=$4; shift 4
    local bin=$HIP; [ "$build" = vk ] && bin=$VK
    local args=$CACHE; for a in "$@"; do args="$args,\"$a\""; done
    echo "{\"id\":\"$id\",\"card\":$card,\"image\":{\"server\":\"sd\",\"binary\":\"$bin\",\"device\":\"$(dev "$card" "$build")\",\"args\":[$args],\"port\":$port}}"
}
kobold() { # id card port flags…
    local id=$1 card=$2 port=$3; shift 3
    local args=""; for a in "$@"; do args="$args${args:+,}\"$a\""; done
    echo "{\"id\":\"$id\",\"card\":$card,\"image\":{\"server\":\"kobold\",\"device\":\"$(dev "$card" k)\",\"args\":[$args],\"port\":$port}}"
}
once() { sed 's/}$/,"count":1}/'; } # one timed image
pe() { # id card port gpu-or-null
    echo "{\"id\":\"$1\",\"card\":$2,\"pe\":{\"gpu\":$4,\"port\":$3}}"
}

run() { # case JSON…, one after another
    for c in "$@"; do
        local id; id=$(sed 's/^{"id":"\([^"]*\)".*/\1/' <<<"$c")
        [ -e "$OUT/$id.json" ] && { echo "$(date +%T) skip  $id: has a result"; continue; }
        echo "$(date +%T) start $id"
        "$BENCH" "$c" "$OUT" > "$OUT/$id.out" 2> "$OUT/$id.err" || echo "$(date +%T) $id: bench exited $?"
        echo "$(date +%T) done  $id: $(tail -1 "$OUT/$id.out" 2>/dev/null)"
    done
}

# PE beside the image job on one card: the image case plus a PE server
MIXED="$(sd mixed-sd-hip-fa+pe 1 hip 5317 --flash-attn on | sed 's/}}$/},"pe":{"gpu":1,"port":5318,"asks":3}}/')"
LANE1=(
    "$(sd sd-hip-fa.card1 1 hip 5311 --flash-attn on)"
    "$(sd sd-hip-nofa 1 hip 5312 --flash-attn off | once)"
    "$(sd sd-hip-offload 1 hip 5313 --flash-attn on --offload-to-cpu)"
    "$(kobold kobold-fa 1 5314 --sdflashattention | once)"
    "$(kobold kobold-offload 1 5315 --sdflashattention --sdoffloadcpu | once)"
    "$(pe pe-gpu 1 5316 1)"
    "$MIXED"
)
LANE2=(
    "$(sd sd-hip-fa.card2 2 hip 5321 --flash-attn on)"
    "$(sd sd-vk-fa 2 vk 5322 --flash-attn on | once)"
    "$(sd sd-vk-nofa 2 vk 5323 --flash-attn off | once)"
    "$(sd sd-vk-offload 2 vk 5324 --flash-attn on --offload-to-cpu | once)"
    "$(kobold kobold-nofa 2 5325 | once)"
)

run "${LANE1[@]}" >> "$OUT/lane1.log" 2>&1 &
run "${LANE2[@]}" >> "$OUT/lane2.log" 2>&1 &
wait
run "$(pe pe-cpu 1 5331 null)" >> "$OUT/lane-cpu.log" 2>&1
echo "all done: $OUT"
