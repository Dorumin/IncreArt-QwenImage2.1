# IncreArt setup guides

Step-by-step setup on a fresh machine, written to be followed literally by a
person or a coding agent. Each step has **Goal**, **Preconditions**,
**Commands**, **Expected result**, **Verify** and **If it fails**.

IncreArt itself is only the GUI: it generates images through a local image
server (KoboldCpp or stable-diffusion.cpp's `sd-server`) and, optionally,
rewrites prompts through a chat server (prompt enhance, "PE").

## Order

| # | Guide | Needed for |
|---|---|---|
| 1 | [01-build-increart.md](01-build-increart.md) | everyone: download a release or build |
| 2 | [02-models.md](02-models.md) | everyone (KoboldCpp users can let the app download) |
| 3 | [03-koboldcpp.md](03-koboldcpp.md) | the KoboldCpp path (recommended) |
| 4 | [04-sd-server.md](04-sd-server.md) | the sd-server path (AMD + ROCm, fastest measured) |
| 5 | [05-prompt-enhancer.md](05-prompt-enhancer.md) | optional prompt rewriting; the KoboldCpp wizard already sets it up |
| 6 | [06-configure-increart.md](06-configure-increart.md) | everyone: first run, `config.rsn` |
| 7 | [07-troubleshooting.md](07-troubleshooting.md) | when something fails |

Do 1, then 3 **or** 4, then 6. Guide 2 is a reference for both paths; guide 5
only if you want a separate chat server.

## Pick a path by hardware

The KoboldCpp wizard picks a preset per GPU from its memory: a GPU takes a
preset when it reports at least 90% of the preset's size
(`crates/increart-core/src/tiers.rs`).

| GPU memory reported | Preset | Image model | Prompt enhance on the same GPU |
|---|---|---|---|
| < 7.2 GiB | none: the wizard can't use the GPU | – | – |
| ≥ 7.2 GiB | 8 GB | Q4_K, text encoder on CPU | swapped with images |
| ≥ 10.8 GiB | 12 GB | Q4_K | swapped |
| ≥ 14.4 GiB | 16 GB | Q8_0 | swapped |
| ≥ 21.6 GiB | 24 GB | Q8_0 | stays loaded |
| ≥ 28.8 GiB | 32 GB | Q8_0 | stays loaded |

System RAM: the presets keep weights in pinned RAM. You need roughly 10–13.5
GiB free per image GPU, plus ~1.5 GiB per KoboldCpp process
([../vram-tiers.md](../vram-tiers.md)).

| Hardware | Path | KoboldCpp build | Status |
|---|---|---|---|
| NVIDIA, Linux | KoboldCpp, runtime **CUDA** | `koboldcpp-linux-x64` | supported in code; **not measured** (all timings are AMD + Vulkan) |
| NVIDIA, Windows | KoboldCpp, runtime CUDA | `koboldcpp.exe` | supported in code; **untested** |
| AMD, Linux, easiest | KoboldCpp, runtime **Vulkan** | `koboldcpp-linux-x64-nocuda` | measured (Radeon AI PRO R9700, ~204–215 s per 1 MP edit at 20 steps) |
| AMD, Linux, fastest | sd-server built with HIP (ROCm) + int8 convrot models | – | measured: ~103 s per image on the same card ([../backend-performance.md](../backend-performance.md)) |
| Intel or other Vulkan GPU | KoboldCpp, runtime Vulkan | `-nocuda` build | **untested** |
| Apple silicon | KoboldCpp, runtime Metal | `koboldcpp-mac-arm64` | supported in code; **untested** |
| CPU only | no image preset exists for CPU | – | image generation on CPU is **not covered**. Prompt enhance alone runs on CPU (guide 5) |

If unsure: Linux + any GPU with ≥ 8 GB → KoboldCpp path with the wizard.

## Facts these guides rely on

- Binary: `increart` (`increart.exe` on Windows) in an unzipped release, or
  `target/release/increart` when built.
- Settings: `config.rsn`; see guide 1 and 6 for its location and
  `INCREART_CONFIG`.
- Logs of servers the app starts: `~/.local/state/increart/logs/` on Linux.
- Model weights are never redistributed by this project; you download them
  from their publishers and their own licences apply.
