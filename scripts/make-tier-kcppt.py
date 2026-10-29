#!/usr/bin/env python3
"""Writes the VRAM-tier KoboldCpp templates (docs/vram-tiers.md) from the
official Qwen-Image 2.1 one (koboldcpp/kcppt on Hugging Face): its
settings, with each tier's model, memory and prompt-enhance ones on top.

Usage: scripts/make-tier-kcppt.py OFFICIAL.kcppt [OUT_DIR]
(OUT_DIR: crates/increart-core/kcppt, where the app reads them from)"""
import json, os, sys

HF = "https://huggingface.co"
IMAGE = HF + "/leejet/Qwen-Image-2.1-GGUF/resolve/main/qwen_image_2.1-{}.gguf"
CHAT = HF + "/Qwen/Qwen3-VL-8B-Instruct-GGUF/resolve/main/Qwen3VL-8B-Instruct-Q4_K_M.gguf"
MMPROJ = HF + "/Qwen/Qwen3-VL-8B-Instruct-GGUF/resolve/main/mmproj-Qwen3VL-8B-Instruct-Q8_0.gguf"
CPU, MAIN = -2, -1

# chat beside the image model (resident) or swapped with it (autoswap)
SWAP = {"admin": True, "routermode": True, "autoswapmode": True}
TIERS = {
    "8GB": dict(SWAP, sdmodel=IMAGE.format("Q4_K"), sdclipdevice=CPU, sdvramlimit=4096, contextsize=4096),
    "12GB": dict(SWAP, sdmodel=IMAGE.format("Q4_K"), sdclipdevice=MAIN, sdvramlimit=8192, contextsize=8192),
    "16GB": dict(SWAP, sdmodel=IMAGE.format("Q8_0"), sdclipdevice=MAIN, sdvramlimit=12288, contextsize=8192),
    "24GB": dict(sdmodel=IMAGE.format("Q8_0"), sdclipdevice=MAIN, sdvramlimit=10240, contextsize=8192),
    "32GB": dict(sdmodel=IMAGE.format("Q8_0"), sdclipdevice=MAIN, sdvramlimit=0, contextsize=8192),
}

official = json.load(open(sys.argv[1]))
out = sys.argv[2] if len(sys.argv) > 2 else os.path.join(os.path.dirname(__file__), "../crates/increart-core/kcppt")
os.makedirs(out, exist_ok=True)
for tier, own in TIERS.items():
    t = dict(official)
    # a server: no browser on launch (or on every autoswap reload)
    t.update(launch=False)
    # prompt enhance: the image model's text encoder, as a chat model
    t.update(model_param=CHAT, model=[], mmproj=MMPROJ, jinja=True, gpulayers=999)
    t.update(sdoffloadcpu=True, sdflashattention=True, sdtiledvae=512, sdvaedevice=MAIN, sdmaingpu=MAIN)
    t.update(own)
    path = os.path.join(out, f"Qwen-Image-2.1-{tier}.kcppt")
    json.dump(t, open(path, "w"), indent=1)
    open(path, "a").write("\n")
    print(path)
