#!/bin/sh
# VRAM-tier runs for KoboldCpp (docs/vram-tiers.md): image + PE on one
# card, one lane per card. A case whose result exists is skipped.
# Usage: scripts/bench-tiers.sh LANE  (LANE: a file of case lines "id|card|image-args-json|extra-json")
set -u
cd "$(dirname "$0")/.."
OUT=tmp/bench-tiers
HUB=/mnt/storage/huggingface/hub/models--Qwen--Qwen3-VL-8B-Instruct-GGUF/snapshots/f982a07559d4a2f6c8744d840bf6fccab30eea96
G=/mnt/storage/huggingface/gguf
models() { printf '{"diffusion_model":"%s/qwen_image_2.1-%s.gguf","llm":"%s/Qwen3VL-8B-Instruct-Q4_K_M.gguf","clip_l":"%s/mmproj-Qwen3VL-8B-Instruct-F16.gguf"}' "$G" "$1" "$HUB" "$HUB"; }
while IFS='|' read -r id card quant args extra; do
    [ -z "$id" ] && continue
    [ -e "$OUT/$id.json" ] && { echo "skip $id"; continue; }
    # the last server's memory must be gone, or the card's rise reads low
    until [ $(cat /sys/class/drm/card$card/device/mem_info_vram_used) -lt 1073741824 ]; do sleep 2; done
    port=$((5400 + card * 10 + $(ls $OUT/*.json 2>/dev/null | wc -l) % 10))
    case="{\"id\":\"$id\",\"card\":$card,\"count\":1,\"image\":{\"server\":\"kobold\",\"device\":\"$card\",\"port\":$port,\"models\":$(models "$quant"),\"args\":$args${extra:+,$extra}}"
    case="$case$(echo "$extra" | grep -q '"chat":true' && printf ',"pe":{"same":true,"port":%s,"asks":3}' $port)}"
    echo "$(date +%T) $id: $case"
    target/release/examples/bench "$case" "$OUT" >> "$OUT/$id.out" 2>> "$OUT/$id.err"
    echo "$(date +%T) $id done: $(tail -1 "$OUT/$id.out")"
done < "$1"
