#!/usr/bin/env bash
# Start one sd-server by hand, for poking at it before the GUI's launcher exists.
#
#   scripts/run-sdserver.sh            # GPU 0, port 1234, HIP build
#   scripts/run-sdserver.sh 1 1235     # GPU 1, port 1235
#   BACKEND=vulkan scripts/run-sdserver.sh 0 1234
#
# Then load Qwen-Image 2.1 over HTTP, e.g.:
#   curl -s localhost:1234/sdcpp/v1/models/load -H 'content-type: application/json' -d '{
#     "recipe": "qwen-image-21",
#     "models": {
#       "diffusion_model": "diffusion_models/qwen_image_2.1_int8_convrot.safetensors",
#       "llm": "text_encoders/qwen3vl_8b_int8_convrot.safetensors",
#       "vae": "vae/qwen_image_2.1_vae_bf16.safetensors" } }'
# (check GET /sdcpp/v1/model-files for the exact ids the server assigns).
set -euo pipefail

gpu=${1:-0}
port=${2:-1234}
BACKEND=${BACKEND:-hip}
SDCPP_ROOT=${SDCPP_ROOT:-/mnt/storage/sdcpp}
MODEL_DIR=${MODEL_DIR:-/mnt/storage/comfy-new/ComfyUI/models}

bin="$SDCPP_ROOT/build-$BACKEND/bin/sd-server"
# Device names come from `sd-server --list-devices`. Under Vulkan the Intel
# iGPU enumerates first (Vulkan0), so the R9700s are Vulkan1 and Vulkan2.
case "$BACKEND" in
    hip)    dev="ROCm$gpu" ;;
    vulkan) dev="Vulkan$((gpu + 1))" ;;
    *) echo "unknown backend: $BACKEND" >&2; exit 2 ;;
esac

exec "$bin" \
    --listen-ip 127.0.0.1 --listen-port "$port" \
    --model-dir "$MODEL_DIR" \
    --backend "$dev" \
    --fa \
    "${@:3}"
