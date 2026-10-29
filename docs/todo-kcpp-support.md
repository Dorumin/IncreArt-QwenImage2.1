# TODO: KoboldCpp support

Status: **as the PE server: done** (DESIGN.md, PE server and requests; v1.123.1
nocuda build in `local/koboldcpp/`, Qwen3-VL-8B-Instruct with vision). As an image
backend: researched, not started. KoboldCpp could serve two roles: the prompt
enhancer (PE) server, and an image backend alongside sd-server.

Investigated on 2026-10-26 against **KoboldCpp v1.123.1** (13 Oct 2026). I used
the `koboldcpp-linux-x64-nocuda` build, run with `--nomodel` on localhost. Its
API was probed live, and the bundled `koboldcpp.py` was read after `--unpack`.
The binary, help text and API docs dump are in `tmp/koboldcpp/`, which is
gitignored. Nothing was run with a model loaded, so speed, output quality and
RGBA handling are **unmeasured**.

## Summary

- **As the PE server: good fit, low cost.** It offers an OpenAI-compatible
  `/v1/chat/completions` with vision through `--mmproj`, which is the
  interface Prompt rewriting already plans for.
- **As an image backend: possible, but a different model of operation,** and
  likely slower here. Deferred until measured (see Next steps).

## Model and platform support

- Qwen Image 2.1 generation and editing were added in 1.123, with a `.kcppt`
  template. The ComfyUI int8 convrot files **don't work**:
  KoboldCpp grows to 29 GiB on them and makes no image (2026-10-28, see
  [backend-performance.md](backend-performance.md)), so the app hides them
  from KoboldCpp and its presets use the GGUFs.
- Progress and previews were added in 1.119, along with `/v1/images/edits`.
  The image LoRA limit is 10, and LoRAs can be chosen at runtime.
- **No ROCm build for Linux.** The official Linux builds are CUDA
  (`koboldcpp-linux-x64`), Vulkan + CPU (`-nocuda`) and `-oldpc`. The ROCm
  fork (YellowRoseCx) last released v1.104 on 2025-12-30, Windows only. On this
  machine the option is the Vulkan build (it detects both R9700s through
  rocminfo) or building from source with hipBLAS. Our sd-server is a HIP
  build, so expect Vulkan KoboldCpp to be slower until measured.

## As the PE server

- Endpoint: `/v1/chat/completions` (OpenAI), and also `/v1/messages` and
  `/v1/responses`. Images are sent as `image_url` parts when `--mmproj` is
  loaded; up to 64 attachments per request since 1.119.
- **Connect**: works with no KoboldCpp-specific code once the PE client
  exists.
- **Start**: a second launcher next to llama-server, e.g.
  `--model <pe.gguf> --mmproj <mmproj.gguf> --host 127.0.0.1 --port <p>
  --usevulkan [ids] --skiplauncher --quiet`.
- Switching between the t2i and i2i PE models without a restart:
  `--admin --admindir <dir>` with one `.kcpps` per model.
  `GET /api/admin/list_options` lists them and
  `POST /api/admin/reload_config {filename}` swaps. Use `--adminpassword`
  unless the server only listens on localhost. `--adminunloadtimeout`
  covers `idle_stop_s`.
- **Don't co-host PE and image gen in one process.** KoboldCpp can load an
  LLM and an image model together, but every request holds the same model
  lock, so a rewrite would wait behind a ~30 s image.
- Health check: `GET /api/extra/version` returns
  `{"version", "llm", "txt2img", "vision", "admin", …}`.
  `GET /api/extra/perf` returns `queue` and `idle`.

## As an image backend: differences from sd-server

| sd-server (what the app uses) | KoboldCpp |
| --- | --- |
| Async jobs: `POST img_gen` → id, poll `jobs/{id}`, `jobs/{id}/cancel` | **Blocking** `POST /sdapi/v1/txt2img` / `img2img`; the response is the image |
| Progress and previews in the job poll | `GET /sdapi/v1/progress?genkey=<key>&skip_current_image=0`. Only works if the request carried the same `genkey`; otherwise it reports idle |
| Explicit cancel | **No cancel endpoint**: closing the connection aborts the generation. Sending `kcpp_extra_args.keep_image_gen_on_disconnect` turns that off |
| Up to 64 pending jobs per server | **One generation at a time**; others block in the HTTP queue (`--queuelimit`, default 10, then 503 "Server is busy") |
| `recipes`, `model-files`, `models/load {recipe, models:{slot: id}}` | **One image model fixed at launch.** `/sdapi/v1/sd-models` returns only that one. Changing models means a `.kcpps` swap through admin mode. No file catalogue, IDs or hashes |
| Typed errors (503 `model_not_loaded`, 409, 422, 429) | **Failure is HTTP 200 with `"images": [""]`**: treat an empty image as an error. Other errors are generic 500 or 503 |
| Result: base64 PNG | `{"images":[b64], "info": "<stringified JSON>", "parameters": {}}`; one image per request |

### Request limits and changes (`sd_generate` in `koboldcpp.py`)

- **Prompt truncated to 3000 UTF-8 bytes.** Long PE rewrites could be cut.
- **The prompt is modified:** it is always sent as `prompt + ", " + forced`,
  where `forced` is the `add_sd_prompt` field and is often empty. The same
  happens to the negative prompt. Conditioning therefore differs from
  sd-server for the same text, and `prompt_sent` would no longer be what the
  model saw. Runs on the two backends wouldn't be comparable.
- **Reference images: at most 4** (`extra_images_max = 4`), sent as
  `extra_images` (b64). With `send_as_refimg: 1`, the img2img `init_images[0]`
  becomes the first reference instead of an init image.
- **Clamps:** `cfg_scale` is at least 1. Steps are capped at 80 by default
  (request fields `add_sd_step_limit` / `remove_limits` change this).
  `denoising_strength` defaults to 0.6.
- `seed: -1` picks a random seed from 100000–999999. We always send a seed.
- Other fields accepted: `sampler_name`, `scheduler`, `distilled_guidance`,
  `flow_shift`, `shifted_timestep`, `clip_skip`, `eta`, `lora`,
  `cache_mode` / `cache_options`, and `--gendefaults` server-side defaults
  merged into each request. Samplers are listed at `/sdapi/v1/samplers` and
  schedulers at `/sdapi/v1/schedulers`.
- `GET /sdapi/v1/get_last.json?genkey=<key>` returns the last result again.
  It is useful after a dropped connection.

### Launch flags relevant to us

`--sdmodel`, `--sdllm`, `--sdvae`, `--sdflashattention`, `--sdmaingpu <id>`,
`--sdclipdevice <id|CPU>` (default **CPU**: set it to the GPU),
`--sdvaedevice`, `--sdoffloadcpu`, `--sdvramlimit <MB>`, `--sdquant 0/1/2`,
`--sdtiledvae <maxres>` (default 512; 0 disables tiling),
`--sdconvdirect off|vaeonly|full`, and `--sdclamped` / `--sdclampedsoft`
(leave unset).

### Work it would take

A backend abstraction in the scheduler and engine, which assume the
sd-server `Client` today. A second client would map our job model onto
blocking requests: one in-flight request per server, a `genkey` per seed
for progress, and cancel by dropping the connection. A launcher for the
model, and recipes that record the backend. Recipes whose prompt or refs
exceed the limits above must be refused or flagged.

## Next steps

1. **PE:** when the PE client is built, support KoboldCpp both as Connect and
   as a Start launcher next to llama-server.
2. **Image backend: deferred.** Worth revisiting if sd-server falls behind on
   new models, or if Vulkan KoboldCpp proves competitive on the R9700s.
3. Measurement to settle (2), when the GPUs are idle: load Qwen-Image 2.1 in
   Vulkan KoboldCpp on one card and compare s/step with sd-server at the same
   resolution and steps. Check that transparent (RGBA) output survives, and
   whether the PNG carries metadata.

## Sources

- KoboldCpp releases: <https://github.com/LostRuins/koboldcpp/releases>
- Release notes v1.123, v1.122, v1.118:
  <https://newreleases.io/project/github/LostRuins/koboldcpp/release/v1.123>
- koboldcpp-rocm fork: <https://github.com/YellowRoseCx/koboldcpp-rocm>
- Embedded API docs: `GET /api` on a running instance (copy saved in
  `tmp/koboldcpp/apidocs.html`)
