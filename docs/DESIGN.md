# IncreArt — Qwen-Image 2.1 batch generation & review

Status: **draft**. Architecture deliberately mirrors IncreMusic-Yue2
(`~/Programming/incremusic-yue2`); read its `docs/DESIGN.md` for the parts
described here as "same as Yue2".

Licence: GUI source is MIT. sd.cpp is MIT; model weights carry their own
licences and are never redistributed by this project.

## Proofs of concept

Throwaway prototypes that settle parts of this design. Their decisions are
folded into this document; each has a writeup.

| POC | Scope | Status | Writeup | Code |
| --- | --- | --- | --- | --- |
| POC1 | Whole GUI with mock data and a mock backend: screen structure, both review queues, folder trees and Thunar-like folder views, collections and smart folders, library model (stores, links, IDs, pixel hash), plus an on-disk sample library and verifier | complete (October 2026) | [POC1.md](POC1.md) | `tmp/gui-prototype/` (git-ignored; see the writeup) |

## Goals

- An **input review queue**: ideas (text, images, or both) collected into an
  inbox, triaged with the same keyboard flow, and promoted into projects.
- Projects containing runs of many seeds, generated across both GPUs.
- Text-to-image **and** edit with reference images (Qwen-Image 2.1 `ref_images`).
- **Transparent (RGBA) output** as a first-class option, preserved end to end.
- **Prompt rewriting** with the official Qwen-Image 2.1 prompt-enhancer model,
  keeping both the original and the rewritten prompt.
- An **output review queue**: fast keyboard review of generated images
  (good / neutral / bad), grid and side-by-side compare.
- Two libraries:
  - **input library**: kept ideas and reference images, reusable as prompts,
    refs or edit sources in any project;
  - **output library**: reviewed generations across all projects.
- Full lineage both ways: idea → project → run → image, and image → refs/parent.
- Any output can be fed back into the input library (as a ref or a new idea).
- Every output is reproducible from the recipe embedded in the file.

Non-goals for v1: mask painting / inpainting UI, LoRA training, upscaling
pipeline (sd-server's `upscale` endpoint can be wired later).

## The two review queues

The app is built around two symmetric queues, each feeding a library:

```
            ┌──────────────── R / I (output → input) ◄───────────────┐
            ▼                                                        │
ideas ──► INPUT QUEUE ──review──► INPUT LIBRARY ──promote (P)──► PROJECT
(text,    (Idea inbox)            (kept ideas,                    │ runs × seeds
 images,                           starred shortlist,             ▼ (both GPUs)
 drop                              ref images)              OUTPUT QUEUE ──review──► OUTPUT LIBRARY
 folder)                                                    (Review tab)            (all projects)
```

- Both queues use the same verdict keys (`1`/`2`/`3` or `G`/`N`/`B`, arrows,
  `Ctrl+Z`) and the same mechanism: an item's folder is its verdict. (`Space`
  differs: it promotes an idea in the Idea inbox and toggles grid / single in
  Review.) (On the
  input side the folder records keep vs. discard; the star is a flag in
  `idea.rsn`, so kept ideas can live in user folders — see Input library
  folders.)
- The output queue (Review) is worked oldest-first by default (the Idea
  inbox grid shows the newest first); the output queue can also be
  filtered to one run.
- The input queue is global (one inbox). The output queue merges the
  `unreviewed/` folders of Review's scope: all projects (the default), a
  folder of projects, or one project.
- Unreviewed counts show as badges on the Idea inbox and Review tabs; the
  Input library and Output library tabs show how many items they hold (kept
  ideas; kept, i.e. good + neutral, images).
- "Queue" in this document always means a review queue. Pending generation
  work is called **jobs** (see Scheduling and the Jobs panel).

## Backend: stable-diffusion.cpp `sd-server`

One `sd-server` process per GPU, each on its own port, either started by the
app or already running elsewhere and connected to; which one is chosen at
start-up (see Startup: servers). Device names come from `sd-server
--list-devices` and are matched case-insensitively; this project writes them as
`ROCm0`/`ROCm1` (HIP) or `Vulkan1`/`Vulkan2` (Vulkan, where the Intel iGPU is
`Vulkan0`):

```
sd-server --listen-port 1234 --backend ROCm0 --model-dir /mnt/storage/comfy-new/ComfyUI/models --fa --conditioning-cache-size 32
sd-server --listen-port 1235 --backend ROCm1 --model-dir ...
```

### Startup: library

The app always starts by asking which library to open; it never reopens one
by itself, unless the command line says so (see Startup: command line). The dialog lists the recent libraries (`recent_libraries` in
`config.rsn`, most recent first, at most 10; the last used one tagged "last used",
the tag in a muted accent colour): a click opens one, × removes it from the list
(the folder is left alone). With none yet it says what a library is.
Opening a library moves it to the top of the list. While it opens, a
progress bar shows under the buttons (reading every project, run and idea:
under a second, about 1 s on a cold disk).
The bottom row's main button names the last used one, **Open
increart_library ⏎** (the full path in its tooltip); Enter does the same.

Below, two buttons open a wizard each:

- **Create a new library…**: where should it live? `~/IncreArt`
  (preselected), `IncreArt` in the current folder (when that is elsewhere),
  or somewhere else (typed or **Browse…**). **Create library** makes it;
  the folder is created if missing. A non-empty folder is refused before
  trying; one that is already a library offers **Open it instead**.
- **Open an existing library…**: libraries found in those places (and not
  in the recent list) to pick, or a folder typed or browsed to. It says
  whether the folder is a library (has `library.rsn`); an empty or missing
  folder offers **Create a library here instead**.

### Startup: command line

Each start-up screen can be skipped from the command line (`increart
--help`):

- `--library <folder>` opens that library; `--library last` the most recent
  one.
- `--servers <preset>` opens that preset (name in any case) and goes on to
  the main window while the servers start, as **Continue in background**
  does; `--servers last` the preset opened last. `--no-servers` continues
  without servers.

A screen is only skipped when its argument is valid: a folder without
`library.rsn`, an unknown or empty preset, or a preset with Start servers
while sd-server or a models folder is missing leaves that screen up, with a
line saying why. `--servers` is used once, when the first library opens.
Unknown arguments print the usage and exit with status 2.

### Switching libraries

The top bar's right end names the open library (**increart_library ⌄**, the
full path in its tooltip). Its menu lists the recent libraries (the open one
ticked), **Open an existing library…** and **Create a new library…** (the
startup wizards) and **Show in the file manager**. Switching keeps the
servers connected and skips the servers screen: drafts are saved, the engine
stops (runs with seeds left continue when their library is opened again; while
runs are generating it asks first), the new library opens, and the engine
restarts on it. Everything else about the old library is dropped (selection,
back / forward history, searches, folder views); settings, servers and prompt
presets stay.

Picking from the wizards, or a library that fails to open, shows the library
screen titled **Switch library**, with **Back to <library>** (`Esc`) to return
to the open one (the engine restarts if it was stopped). There the open
library is tagged "open now" (and can't be removed from the list) and the main
button names the last used other one.

### Startup: servers

When the app starts (after the library is opened) a modal dialog asks how to
reach sd-server. Its choices are **presets** saved in `config.rsn` (see
Config file → Servers):

- The preset list (the dialog is one column; editing and settings open
  modals). The one used last is tagged "last used",
  the tag in a muted accent colour. Each row shows its servers, e.g. "Start ROCm0 on
  :1234 · Start ROCm1 on :1235" or "Connect to http://10.0.0.2:1234".
  **A click opens a preset**. The bottom row's main button names the last
  used one, **Open ROCm0 only ⏎**, and Enter does the same; not when the
  dialog was reopened from the main window while servers run (**Close**
  keeps them). Each row has
  ✎ **Edit** and 🗑 **Delete** (which asks first). Mid-session (Settings →
  Servers), opening a preset asks before stopping the servers in use (and
  says how many runs are generating).
- With no presets, **Set up servers…** opens a wizard: the sd-server
  binary and models folder, then the devices from `--list-devices` with
  every GPU ticked (not the CPU) on ports 1234, 1235, …, or a server
  already running to connect to. **Save as preset and open** names the
  preset after its devices ("ROCm0 + ROCm1", or "Running server") and
  opens it.
- **sd-server settings…** (bottom right) opens a modal with what every
  preset's Start servers share: the binary, the models folder, leave
  started servers running, and the devices (`sd-server --list-devices`,
  **List again**). When a preset starts servers and the binary or models
  folder is missing, the dialog says so with **Fix…**, which opens it.
- ✎ or **New preset** opens the editor modal: the name, then the server
  rows. **+ Start server** adds a device and port (1234, 1235, … by
  default); the app launches it and stops it when it exits. The listed
  devices are buttons too: a click adds a Start server on that one (devices
  already in the preset are greyed). **+ Connect** adds a URL; connected
  servers are left running when the app exits. A preset can mix both kinds.
  Esc cancels; a click outside doesn't. The editor's buttons:
  - **Save** stores the preset, renaming it in place.
  - **Save and open** stores it and opens it.
  - **Open without saving** opens the edited servers once and leaves the
    preset as it was.
  - **Cancel** drops the edits.
- **Continue without servers** asks first: browsing, review and the
  libraries work; Generate queues runs that wait for a server. The first
  Queue of a session with no server (and none starting) warns that the run
  will only wait, with **Set up servers…**, **Queue anyway** and **Cancel**;
  it doesn't ask again until the app restarts.
- While the servers come up the dialog is **locked**: presets, server rows
  and settings are disabled and dimmed ("locked while the servers open"),
  and per-server progress stays readable ("starting — 12 s" followed by the
  server log's latest line, e.g. "294/396 · 4.38GB/s" while tensors load;
  up; or the error with the tail of a started server's log). When servers
  were in use they're stopped first ("stopping the servers in use first"). Two buttons:
  - **Continue in background**: the main window opens at once; the top bar
    shows each server as "starting… <latest log line>" / "failed", failures are reported in
    the status bar, and the engine starts with the servers that came up when
    the last one has answered.
  - **Cancel**: stops opening, and the servers already started.
- Stopping servers (Cancel, Continue without servers, opening another
  preset) shows each as "◌ stopping…" in the top bar until its process has
  exited and freed its GPU memory; then the status bar says "Stopped
  sd-server gpu0, gpu1: its GPU memory is free".
  Each opening is numbered; reports from a cancelled or replaced attempt
  are ignored (and a server it started after all is stopped).
- If one fails (and you stayed in the dialog) it stays open: retry, change
  the preset, or continue with the servers that did come up.
- **Leave started servers running when IncreArt exits** (`sdserver:
  leave_running`, off by default): started servers outlive the app with
  the model loaded, and the next session's Start finds them on their ports,
  so it is ready at once; the GPU memory stays in use meanwhile.
- Using a preset records it as `last_preset`.
- The top bar's **Servers…** opens a status window: each server's state and
  model, servers still starting, how many runs are queued or generating, and
  **Change servers…** (the dialog), **Jobs** and **Close**.
- Reopened from the main window, the dialog can be left again: **Close**
  (or Esc) goes back keeping the servers in use, and **Continue without
  servers** becomes **Stop the servers, continue without** while servers
  are in use. Opening a preset asks first if servers are in use, then
  stops the ones the app started.
- A **Start** server finds an sd-server already answering on its port (e.g.
  one left over from a crash) and uses it instead of failing on a port in
  use.

Only the native async API (`/sdcpp/v1/...`) is used:

| Use | Endpoint |
| --- | --- |
| Health / features | `GET capabilities` |
| Model catalog | `GET recipes`, `GET model-files`, `POST models/refresh` |
| Load / status | `POST models/load {recipe, models:{slot: file_id}}`, `GET models/status` |
| Submit | `POST img_gen` → 202 `{id}` |
| Poll + preview | `GET jobs/{id}` (status, `preview.step/total_steps/b64_json`, `result`, `error`) |
| Cancel | `POST jobs/{id}/cancel` |

Errors to handle explicitly:

- `img_gen`: 503 `model_not_loaded` / `model_loading` (wait or trigger load),
  400/422 (request bug → fail the seed, keep the run), 429 (queue full →
  back off).
- `models/load`: 409 `model_busy` (jobs still pending on that server), 400
  `invalid_model_config` (previous model stays loaded), 422 `model_load_failed`
  (server is left with no model). The load request blocks until done; poll
  `models/status` from elsewhere for progress.
- Jobs: a cancelled generating job stays `generating` with
  `cancel_requested: true` until it reaches a checkpoint; keep polling until
  `cancelled`. Finished jobs are purged after a TTL (≥ 60 s), so results must
  be fetched as soon as a job completes. At most 64 jobs may be pending per
  server.

Model file IDs are opaque; the GUI resolves them from `model-files` and stores
both the ID and the file's content hash in the recipe.

Default Qwen-Image 2.1 selection (existing ComfyUI tree):

- `diffusion_model`: `diffusion_models/qwen_image_2.1_int8_convrot.safetensors`
- `llm`: `text_encoders/qwen3vl_8b_int8_convrot.safetensors`
- `vae`: `vae/qwen_image_2.1_vae_bf16.safetensors`
- `tae` (optional, for previews): `taeqi2_1.safetensors` from
  `madebyollin/taeqi2_1`, not downloaded yet. It also supports RGBA.

`llm_vision` is only needed with a separate GGUF text encoder. The safetensors
encoder includes the vision tower: confirmed 2026-10-24 with a three-ref
`ref_images` job (see Measurements).

Referring to reference images in prompts: for Qwen-Image 2.1, sd.cpp puts
each ref into the encoder prompt as `<image1><|vision_start|>…<|vision_end|>
<image2>…` (in `ref_images` order) before the user's text, so prompts name
them **`<image1>`, `<image2>`, …** ("Picture 1" is the Qwen-Image-Edit
2509/2511 convention, "Image 1" Mage-Flow's). The Generate panel's ref
chips show these tags; a chip's **T** button puts its tag in the prompt at the
cursor (replacing a selection; at the end if the prompt was never focused),
spaced from the words around it, and returns to the prompt after it. Any
prompt template or PE rewrite must keep them.
For the text encoder a transparent ref is composited onto white; the
diffusion side gets the VAE-encoded ref (alpha handling there unverified).

Built-in recipes:

| Recipe | Diffusion weights | Steps | Sigmas | CFG |
| --- | --- | --- | --- | --- |
| `qwen-image-21` | base (above) | 20 | resolution-dependent flow schedule | recipe default 1; sd.cpp example uses **6.0**. Pick the preset value by A/B test. |
| `qwen-image-21-turbo` | **separate Turbo weights** (`Qwen/Qwen-Image-2.1-Turbo`, not downloaded yet) | 8 | fixed `qwen-image-2.1-turbo` sigmas | 1.0 |

The GUI's presets set CFG explicitly rather than inheriting the recipe default.
Switching between base and Turbo changes the model selection and therefore
triggers a load (see Scheduling). Dimensions are snapped to multiples of 32;
the recipes default to 2048², but the GUI's default is 1024² (see measurements
below).

## Transparency (RGBA)

Qwen-Image 2.1 decides whether to output alpha from the **prompt**; there is no
API switch. The documented format is:

> `This is an RGBA image with transparency. <description>. The image has alpha channel and the background is transparent.`

- **No Transparent toggle** (removed 2026-10-25). Ask for transparency in the
  prompt itself, e.g. "… remove the background and make it transparent". The
  prompt is sent exactly as typed; older runs keep their recorded `alpha: true`
  and the prompt that was sent.
- Why: the wrapper above breaks edits. With a reference image (`<image1>`) it
  made the model return an empty, fully transparent canvas (alpha ≈ 1/255
  everywhere, a flat purple underneath) for every edit prompt tried, while the
  same edit asked for in plain words ("make it transparent") worked. It does
  work for text-to-image, but one way of asking is simpler than two.
- Output format is always PNG (alpha survives; never JPEG). `ref_images` and
  `init_image` are sent with their native channels, so RGBA inputs keep their
  alpha.
- Review, library thumbnails and compare draw images over a checkerboard,
  toggleable to a solid colour. Thumbnails are WebP with alpha.
- Library filter and smart folder: `transparent`. It must test the pixels,
  not the channel count: Qwen-Image 2.1 returns RGBA PNGs for every prompt,
  and ordinary images come back with alpha 253–255 throughout (measured, see
  below). An image counts as transparent when at least 5% of its pixels have
  alpha below 200. It is worked out once per store image, when it is
  written, imported or first indexed, and kept in `cache/pixel-index.rsn`
  (not with the thumbnail: thumbnails are made lazily, so counts would be
  wrong until every image had been shown). The run's recorded `alpha` is
  history only; nothing filters on it.

## Prompt rewriting

Qwen ships two fine-tuned Qwen3.5-9B prompt-enhancer (PE) models: one for
text-to-image and one for edit (i2i). The i2i one is already present:
`text_encoders/qwen3.5_9b_qwen_image_2.1_pe_i2i.int8_convrot.safetensors`.

- sd-server cannot run chat models, so rewriting is a **separate, optional
  service**: an OpenAI-compatible chat endpoint (`/v1/chat/completions`).
  llama.cpp's `llama-server` is the default choice. It needs GGUF weights,
  because the ComfyUI int8_convrot safetensors file can't be used directly.
  The T2I PE model, and GGUFs for both, still need to be obtained.
- The PE system prompts come from the official Qwen-Image 2.1 repo and are
  stored in config, not hard-coded.
- GPU placement: the PE server runs on demand rather than permanently. It
  shares a GPU with an sd-server, or runs on CPU, because both GPUs are busy
  during batches. Rewriting is a short, interactive step, so CPU is
  acceptable.
- Where it's used:
  - Idea inbox / input library: `W` (or Rewrite in a card's menu) queues the
    idea for the PE server; rewriting is asynchronous (seconds to a minute,
    possibly on CPU), so nothing blocks. The idea's card shows the state
    ("queued for rewrite", "rewriting… 40%", "rewrite ready — open to
    review"). Opening the idea (Edit idea window) shows the
    original-vs-rewritten diff with the added words highlighted: **Accept**
    stores the result as a variant alongside the original (both are kept),
    **Reject** drops it. Card right-click menus offer **Accept all ready
    rewrites (N)**. The top bar's PE status shows the server state, with the
    queue in its tooltip; clicking it opens the server status (the
    **Servers** modal), which has a Prompt enhance section. Until PE is
    built that section says it isn't connected and why.
  - New idea form: **Rewrite with PE** suggests a rewrite of the draft, to
    use as the text, keep as a variant, or discard.
  - Generate panel: a **Rewrite** button (T2I or i2i model, depending on
    whether refs are attached; the i2i model also receives the ref images)
    with a diff view of original vs rewritten before accepting.
- Generation always records `prompt_original` and `prompt_sent`. Rewriting is
  never automatic at generation time, so a run's prompt is fixed and
  reproducible.

### PE server and requests

- Config (`config.rsn`), next to the sd-server presets:
  ```rsn
  pe: Pe {
      // Start: the app runs llama-server on demand; Connect: an existing
      // OpenAI-compatible endpoint.
      server: Start { binary: "llama-server", port: 1240, device: "CPU", args: ["-c", "8192"] },
      // server: Connect { url: "http://127.0.0.1:8080" },
      models: {
          "t2i": PeModel { file: "/models/qwen3.5-9b-pe-t2i-Q4_K_M.gguf" },
          "i2i": PeModel { file: "/models/qwen3.5-9b-pe-i2i-Q4_K_M.gguf", mmproj: "/models/…-mmproj.gguf" },
      },
      system_prompts: { "t2i": "…", "i2i": "…" },  // from the Qwen-Image 2.1 repo
      idle_stop_s: 300,
  }
  ```
  With `Start`, the server is launched when the first rewrite is queued and
  stopped after `idle_stop_s` without work; switching between the t2i and
  i2i model restarts it (llama-server serves one model).
- Requests: one chat completion per rewrite (system prompt + the user's text;
  for i2i, the ref images as `image_url` parts in `ref_images` order).
  Rewrites run one at a time from a queue the GUI shows (top bar PE status).
- **Reference tags are preserved**: the user's text may name refs as
  `<image1>`, `<image2>`, …; a rewrite that drops or renumbers a tag that was
  in the input is flagged in the diff view (and the tags are listed in the
  request so the model keeps them).
- The recipe's `rewrite` field records the PE model file's SHA-256 (cached
  like the diffusion weights) and the system prompt's hash.
- Not yet obtained: GGUF weights for both PE models (see Open questions).

## Caching

Three separate caches are involved; only the first is worth tuning:

| Cache | What | Where / lifetime | Cost if missed |
| --- | --- | --- | --- |
| Conditioning (sd.cpp) | Text-encoder output per prompt + refs | LRU in host RAM, per server, `--conditioning-cache-size N` (default 4); lost on restart or model reload | Full Qwen3-VL encode (the first encode in the log took 50 s, including lazy weight load) |
| Prefix (sd.cpp, Qwen-Image 2.1) | Per-layer K/V of text + ref tokens | VRAM, **one sampling run only** (released at the end) | About one extra step of work per image (~1.5 s of ~31 s) |
| Thumbnails (IncreArt) | `thumbs/{input,output}/<image-id>.webp` | Disk, deletable | Re-decode |

Decisions:

- **Raise the conditioning cache** to 32 entries per server. Each entry is
  only the encoder output (a few MB), so the RAM cost is small. Seeds of one
  run share an entry. Interleaving several runs and A/B variants needs more
  than the default 4.
- **Disk persistence** is not supported by sd.cpp today. The useful target is
  the conditioning cache (small, expensive to recompute, lost on restart).
  Plan: an upstream patch adding `--conditioning-cache-dir`. Key = hash of
  model file hashes + prompt + ref image hashes + encoder params; value =
  serialized `SDCondition`. Until that lands, the scheduler keeps runs sticky
  to a server (see below) to get the most out of the in-RAM cache.
- Persisting the **prefix cache** to disk is not planned. It is 0.5–4 GiB per
  condition, and reading it back would cost about as much as the single step
  it saves. `qwen_image_2_1_prefix_cache_type=q8_0` is an option if VRAM is
  tight.

## Crates (same split as Yue2)

- `increart-core` — config (RSN), server launcher/attacher, sd-server client,
  PE (chat) client, scheduler, image I/O + metadata, library/filesystem
  layout, history.
- `increart-gui-core` — `AppState` + pure `update(state, msg) -> effects`
  reducer; all behaviour tested headlessly.
- `increart-gui` — eframe/egui view; thin, no logic.

## File format: RSN

Every structured file IncreArt writes (`config.rsn`, `library.rsn`,
`idea.rsn`, `project.rsn`, `run.rsn`, `cache/pixel-index.rsn`) and the
`increart:recipe` PNG chunk use [RSN](https://github.com/khonsulabs/rsn)
(the `rsn` crate, a Rust-like object notation) through serde. It reads like Rust
literals: `Name { field: value }` structs (the name is optional when read,
and checked when present), unit and struct enum variants (`Generate`,
`Edit { init: ImageRef { … }, strength: 0.75 }`), `[…]` lists, `{ "key": value }`
maps, `//` comments. IncreArt writes 4-space indentation and `Option`
values bare (`source_idea: "…"` or `None`); `Some(…)` is also accepted on
read. Unknown fields are ignored on read, so older builds can open files
written by newer ones. The one exception is
`increart:source`, which is JSON (it describes a foreign file and may be
read by other tools).

## Config file

User settings live in one RSN file, `config.rsn`, in
`$XDG_CONFIG_HOME/increart/` (default `~/.config/increart/config.rsn`). It
holds machine and user preferences, not library data, plus the recent
libraries (see Startup: library). Older configs' single `library` entry is
read into that list. Other sections (PE system prompts) are added as they
are specified. The GUI rewrites the whole file when a setting is saved, so hand
comments are not preserved.

### Servers

```rsn
Config {
    server_presets: [
        ServerPreset { name: "Both GPUs", servers: [
            Start { name: "gpu0", port: 1234, device: "ROCm0" },
            Start { name: "gpu1", port: 1235, device: "ROCm1" },
        ] },
        ServerPreset { name: "Remote box", servers: [
            Connect { name: "box", url: "http://10.0.0.2:1234", device: "ROCm0" },
        ] },
    ],
    last_preset: "Both GPUs",
    sdserver: SdServer {
        binary: "/mnt/storage/sdcpp/build-hip/bin/sd-server",
        model_dirs: ["/mnt/storage/comfy-new/ComfyUI/models"],
        args: ["--fa", "--conditioning-cache-size", "32"],
        preview: "none",
        commit: "fd21ecf",
    },
    resolutions: [ … ],
}
```

- `Start` servers take `host` (default `127.0.0.1`), `port`, `device` and
  optional per-server `args` (e.g. `--offload-to-cpu`). The command line is
  `--listen-ip host --listen-port port --backend device`, then a
  `--model-dir` per `model_dirs` entry, then `sdserver.args`, then the
  server's own `args`. Output goes to `sd-server-<name>.log` in the app's log
  directory.
- `Connect` servers take a `url`; `device` is optional and only recorded in
  recipes.
- `sdserver.preview` is the preview mode sent with every job (`none`,
  `proj`, `tae`, `vae`); `commit` is recorded in recipes as `sdcpp`.

### Resolutions

The Generate panel's size list comes from `resolutions` in `config.rsn`. One
list is shared by every recipe, base and Turbo alike.

```rsn
Config {
    resolutions: [
        Resolution { name: "Square",        width: 1024, height: 1024, default: true },
        Resolution { name: "Square L",      width: 1344, height: 1344 },
        Resolution { name: "Square XL",     width: 2048, height: 2048 },
        Resolution { name: "Portrait 2:3",  width:  896, height: 1344 },
        Resolution { name: "Landscape 3:2", width: 1344, height:  896 },
        Resolution { name: "Wide 16:9",     width: 1664, height:  928 },
    ],
}
```

- Next to the size: **Scale…** (to megapixels or a side, keeping the
  aspect) and **Aspect** (1:1, 5:4, 4:3, 3:2, 16:10, 16:9, 2:1, 21:9, each
  landscape or portrait; swap width and height; or the aspect of a reference,
  "Like <image1>"), keeping about the same megapixels, rounded to 32.
- If `resolutions` is missing or empty, the built-in list above is used (and
  written back the first time the list is edited).
- Width and height must be multiples of 32 (sd.cpp rounds output sizes up to
  32 and rejects ref images that aren't aligned). Values that aren't are
  snapped to the nearest multiple on load, with a warning in the log.
- Exactly one entry is the default. If none is marked, the first one is; if
  several are, the first marked one wins.
- `name` is optional. The dropdown shows `W × H` followed by the name.
- Duplicate sizes are allowed in the file but flagged in the editor.

**Resolution editor** (window opened from an **Edit…** button next to the size
dropdown in the Generate panel):

- One row per entry: name, width, height, then derived aspect ratio (reduced,
  e.g. `3:2`, or `≈16:9` when close), megapixels, and an estimated time per
  image.
- The estimate scales the measured warm time (34 s for 1024², 20 steps, one
  R9700) by pixel count and by the Generate panel's current step count. It is
  shown as `≈` and is only a rough guide; replace the constant with measured
  per-size timings once the 2048² benchmark exists.
- Width/height fields snap to multiples of 32 when editing finishes and show a
  note when snapping changed the value. Sides outside 256–2048 get a warning
  (not an error).
- Row actions: set default, duplicate, swap W/H, move up/down, delete (not the
  last entry). Window actions: add, reset to built-ins, save, cancel.
- Saving writes `config.rsn` immediately. Existing runs are unaffected: a run
  stores its exact width and height, never a reference to a list entry.

The Generate panel's dropdown also has a **Custom W × H** entry for one-off
sizes (same snapping and warnings), with a **Save to list** button that adds
it as a new entry.

## Data model

```
Inbox ── Idea (unreviewed) ──review──► Input library (kept ideas + ref images)
                                            │ promote (one or many ideas)
                                            ▼
Project ─┬─ Run (prompt template, params, refs, N seeds)
         │    └─ Revision (edited params; new seeds go here)
         │         └─ Item (one seed → one image) ──review──► Output library
         └─ sources: the ideas it was promoted from
```

### Seeds

A run's seeds are consecutive: a first seed plus a count, always sent to
sd-server explicitly (never `-1`/random).

- **The first seed auto-advances per project**: the Generate panel's
  **First seed** starts at one past the highest seed any run of the project
  has used (generated, pending, cancelled or failed), so a new run gets fresh
  seeds. It is derived from the project's `run.rsn` files, so it survives
  restarts; a new project starts at 0. It is set when a project is selected
  and again after each run is queued.
- For A/B comparison, the **0** button next to the field restarts at seed
  0, so runs line up seed for seed: seed 3 of `r01` and seed 3 of `r02 cfg6`
  then differ only in the run's settings. (Until 2026-10-24 this was the
  default.)
- Adding seeds to an existing run, including seeds for a new revision,
  continues after that run's highest seed, so a run never generates the same
  seed twice.
- A seed is pending until its output link exists in the project. Cancel
  records the dropped seeds in `run.rsn` (`cancelled: [Seeds { first, count
  }]`), and a seed that fails for good (bad request, generation error) is
  recorded with its error (`failed: [SeedFailure { rev, seed, error }]`), so
  neither is queued again after a restart.
- Each stored image's timing is recorded in `run.rsn` too (`timings:
  [SeedTiming { rev, seed, device, done, elapsed_s, queued_s, step_s }]`,
  see Timing), and in its recipe (`elapsed_s`, `queued_s`, `step_s`).

### IDs

Ideas, projects, runs, input images and output images all have IDs of the
form `YYYY.MM.DD-HH.MM-NNNN`, e.g. `2025.01.30-23.45-0000`:

- The timestamp is the creation time in **UTC**, to the minute: when an idea
  or project was created, a run queued, an input image imported, an output
  image written. UTC keeps names in true chronological order (no DST jumps)
  and identical across machines.
- `NNNN` counts from `0000` within that minute, **separately for each kind**.
  So `2025.01.30-23.45-0000` can be an idea, a run and an output image at the
  same time; a reference always knows which kind it means from where it
  appears (`source_idea`, `projects`, an `ImageRef`'s `store`).
- Sorting by ID is sorting by creation time.
- IDs are never reused or changed. Names people see (project directory names,
  idea titles, folder names) are separate and can change freely.

Allocation: the app is the only writer. It holds `library.lock` while the
library is open, and a second instance refuses to open it. On opening, it
scans the existing IDs of each kind; after that it hands out the next number
for the current minute from memory. Store images are additionally created
with an exclusive create, so even a wrong count can't overwrite a file; it
just moves on to the next number. Past 9999 in one minute the counter grows a
fifth digit (names then stop sorting exactly; not expected in practice).

### Ideas (input side)

An `Idea` is a small folder: `idea.rsn` plus any attached images.

- `idea.rsn`: id (see IDs), title, text (prompt draft, notes, or both),
  `variants: Vec<PromptVariant>` (rewritten or hand-edited alternatives, each
  with its origin: `Manual` or `Rewritten { model, system_prompt_hash }`),
  tags, source (`Manual`, `Clipboard`, `DroppedFile { original_path }`,
  `InboxFolder`, `FromOutput { image: ImageRef }`), created time,
  `projects: Vec<ProjectId>` it has been promoted into, `starred: bool`.
- Attached images live in the input image store
  (`input_images/<image-id>.png`, see Image stores), and the same picture
  used by several ideas and as a ref is stored once. `idea.rsn` lists the
  image IDs (authoritative); the idea folder also holds links to them named
  `<n>-<original name>.png`, so it can be browsed in a file manager.
- An image with no text is still a valid idea ("make something like this").
  A reference image in the input library is stored as such an image-only
  idea, so every library entry is an idea folder that can be filed.

Ways in: a text box / "new idea" hotkey, paste from clipboard (text or image),
drag and drop files, and a watched drop folder (`input/inbox_drop/`) so ideas
can be added from outside the app (phone sync, browser saves, scripts).

Whatever the way in, images are imported into `input_images/` straight away
(re-encoded as PNG, see Image stores) and the new idea in the Idea inbox links
to them; the inbox has no image storage of its own. A file taken from
`inbox_drop/` is then moved to `inbox_drop/imported/`, so the watcher doesn't
import it twice and the original is still there until you clear it out.

Input review uses the same keys as output review, but the verdicts mean
something different. Good and neutral both move the idea into the input
library (top level of `input/library/`), differing only in `starred`; bad
moves it to `input/discarded/`:

| Key | Output queue | Input queue |
| --- | --- | --- |
| good | keep, best | **keep**, starred: shortlist for new projects |
| neutral | keep | **keep** in the input library |
| bad | reject (out of the way, still recoverable) | **discard** (`input/discarded/`, still recoverable) |
| `P` / `Space` | — | **promote**: new project (or add to an open one) seeded with the idea's text as prompt and its images as refs |
| `W` | — | **rewrite** the idea's text with the PE model (adds a variant) |

Promoting always asks for the new project's name first (an idea's title
needn't suit a project): the first idea title that is a valid, free name is
the guess (numbered when taken), else the field is empty; an invalid or taken
name is refused. **Add & promote** in the New idea form asks the same way and
keeps the draft until the name is given. By default the project goes in a new
folder of its own name (**In a new folder of the same name**, ticked; the
folder follows the name as it is typed); untick it to put the project in the
location itself. The dialog shows where the project will go
(`→ projects/clients/acme/Fox cafe/Fox cafe`); **Create here** (Enter) makes it
there. **Choose location…** is a second step: a tree of the project folders
(with a filter) to pick from, and the new folder in it, named in place (renamed
there it no longer follows the project's name, and the step 1 box says
"In a new folder “…”"; ✕ takes it away, **New folder in "…"** brings it back).
It is only made on Create, and an existing folder of that name, in any case,
is used. **Back** returns to the name. The first location is where the last
promote went (this session), if that folder still exists, else the one below;
after a project in its own folder, that is the folder's parent (so projects
don't nest).

Promoting several selected ideas at once makes one project with all of them
as sources (e.g. a style image from one idea + a subject from another). An
idea with several variants can be promoted as one run per variant.

Promoted images become `ref_images`, never an implicit `init_image`;
`init_image` is only sent for an explicit Edit run.

### Output → input

An output image can feed new work, pointed at in place (an `ImageRef` into
`output_images/`, no copy), so recipes' `refs` / `parent` lead back to it and
its project. Clean up keeps any output another run refers to.

- **How a project uses such images** is a project setting (`project.rsn`
  `edit_with`), shown in the Generate panel: **references** (the default:
  `ref_images`, Qwen-Image 2.1's native editing; the model both reads the
  image through the vision encoder and gets its VAE latents) or **img2img
  start** with a strength (`init_image`: sampling starts from a noised copy,
  keeping the layout; the recipe's `parent`). A copied project keeps it.
- Actions on an image (Review: keys, right-click in Single and Grid; Output
  library: right-click), each following the target project's setting:
  - **Use in current project**: adds it to the selected project's form.
  - `E` **Edit this image**: a fresh form in the image's own project with the
    image; with references the prompt starts as `<image1> ` for the change.
  - `N` **New project from this image**: a project next to the image's,
    named "<project> from s0003" (opened for rename), references, the image's
    prompt prefilled.
  - `D` **Duplicate project with this image**: a copy of the image's project
    (settings, sources, draft; no images), the form started from the image's
    run (prompt, size, steps, CFG, transparency, refs) plus the image.
  - `R` to the input library as a ref (an image-only idea, pixels imported
    into the input store), `I` a new idea in the Idea inbox with its prompt.
    Lineage is kept via `source: FromOutput`.
- The right-click menus of project tiles and the project tree have
  **Duplicate** (the same copy, no image). It first asks for the copy's name
  in a dialog ("<name> copy", flagged if taken or invalid), then copies next
  to the project and selects the copy.
- Up to 9 references (Qwen-Image 2.1's stated limit; sd.cpp enforces none):
  the Generate panel counts them ("3 / 9") and warns past 9. Each ref costs
  about as much attention as the output (refs are resized to the output's
  pixel count), which the time estimate includes.
- Each image in the Generate panel has **use size** next to its dimensions:
  it sets the output size to that image's aspect, in multiples of 32 within
  256–2048 (greyed out when the size already matches).

### Recipe

`Recipe` (embedded in each PNG as an `iTXt` chunk `increart:recipe`, RSN):

- `prompt_original`, `prompt_sent` (after rewrite and/or the RGBA wrapper),
  negative_prompt, width, height, seed, `alpha: bool`
- sample params: method, scheduler, steps, cfg, flow_shift, custom_sigmas.
  An empty scheduler and `flow_shift: None` are left out of the request, so
  the recipe's own (resolution-dependent) flow schedule applies
- `mode`: `Generate` | `Edit { init: ImageRef?, strength }`
- `refs: Vec<ImageRef>` — `ImageRef { id, store: Input | Output }`.
  The image is always at `<store>_images/<id>.png`, so a reference
  never goes stale, however the folders it is linked from are reorganised.
- `parent: Option<ImageRef>` — the image this was edited from (lineage)
- `source_idea: Option<IdeaId>`, and the PE model + system-prompt hash if
  rewritten
- model selection: recipe id + slot → (file id, sha256)
- sd.cpp commit, IncreArt version, GPU/backend, elapsed seconds
- `inputs: Vec<InputImage>` — the images it was made from, filled in by the
  library when the output is stored: `role` (`Ref(n)` for `<imageN>`, or
  `Start` for the img2img start image), `image` (`ImageRef`), `file` (the
  store file, e.g. `input_images/<id>.png`), `original_name` (the file an
  input was imported from, if known) and `blake3` (its BLAKE3 pixel hash).
  The output's own BLAKE3 is its `increart:blake3` chunk.

**Metadata upgrade.** `library.rsn` records `metadata: 1` once every store
image has `increart:blake3` and every output recipe its `inputs`. A library
opened without it is upgraded first (inputs, then outputs): only those text
chunks are added, the pixels aren't re-encoded. A store file is replaced
atomically, except in hard-link libraries, where it is rewritten in place so
the links keep sharing it. Images that can't be upgraded are listed as
problems.

**Use counts.** How often an image was used is derived from the run records:
the runs naming it as a ref, the runs starting from it, and the images those
runs made. A run counts once it has generated (has a revision), so a project
duplicated without seeds doesn't count. It shows on idea cards (their images
added up; the input library can sort by it, **order: most used**), in
Generate's image list and in the Output library's lineage panel.

The two run counts are also kept in every store image (inputs and outputs), as
`increart:usage` (`tEXt`): `ref=000000003 start=000000001`. They are "ever
used" counts: one more ref per run naming the image (however often), one more
start per run starting from it, added when the run first generates (its first
revision) and never lowered, so they survive the run being deleted. The text
is always 29 bytes (nine digits each, at most 999999999), so a new count is
written over the old chunk in place: the chunk type, data and CRC in one
write, then a sync; the file keeps its size and isn't replaced. Copies in
copy-mode projects aren't updated. A write cut short leaves a wrong CRC.
Each time a library opens, every store image's counts are checked against the
runs: a missing, torn or unreadable chunk is rewritten from them, and a count
lower than the runs show is raised. The check reads every store image (about
6 s for 500 images on a cold spinning disk), so `Library::open` leaves it to
the caller: the GUI runs it in the background after opening, one image at a
time (library operations go on meanwhile), with a progress bar in the status
bar ("checking images 120 / 950"); opening another library stops it. A failure to update the counts is listed
as a problem; it doesn't stop the run.

sd-server also embeds its own `parameters` tEXt chunk (A1111-style text plus
full `SDCPP` JSON). It is kept as-is. `increart:recipe` is authoritative
because it carries hashes and lineage.

Requests send refs as base64 PNG in `ref_images`; the image bytes are read from
the library at submit time and checked against their `increart:pixels`.

### Pixel hash

Besides its ID, every image has a content fingerprint, `pixel_sha256`: the
SHA-256 of its decoded pixels, not of the file bytes. Rewriting metadata (the
recipe chunk, sd-server's `parameters` chunk) or re-encoding at another
compression level doesn't change it. It is used to spot duplicates on import,
to verify files, and in cache keys; it is not how images are referenced.

Canonical encoding, version 1 (hashed in this order):

1. the ASCII tag `increart-pixels-v1`
2. width and height, each as a big-endian `u32`
3. channel count as one byte: 3 (RGB) or 4 (RGBA)
4. the rows, top to bottom, 8 bits per channel, no filter bytes or padding

Decoding rules: grey and palette images are expanded to RGB; an image has 4
channels exactly when the file has alpha (an alpha channel or `tRNS`), even
if every pixel is opaque; 16-bit channels keep the high byte. The version tag
lets the encoding change later without colliding with old hashes.

Every image also carries a second fingerprint, `increart:blake3`: BLAKE3
of the same canonical encoding (hex, 64 characters). The SHA-256 stays the
key of the pixel index and of verification; the BLAKE3 is what an output's
recipe records for each image it was made from (Recipe, `inputs`).

Model weights are the exception: they are identified by the SHA-256 of the
file bytes.

### Image stores

All images live in two flat stores, named by image ID, and every image there
is a PNG:

- `input_images/<image-id>.png`: everything that came in from outside
  (idea attachments, reference images, edit sources).
- `output_images/<image-id>.png`: everything sd-server generated.

Store files are never renamed or moved. Idea folders and project folders hold
**links** to them (see Links), so the folder structure is still browsable and
review still works by moving files, but moving or deleting a link never
touches the image.

**Inputs are re-encoded as PNG on import.** Lossy formats (JPEG, WebP) decode
slightly differently across decoder versions, so their pixel hash wouldn't be
stable, and one format keeps everything downstream simple. On import:

1. Decode; apply the EXIF orientation; convert to sRGB if an ICC profile says
   otherwise; reduce to 8-bit RGB or RGBA (keeping alpha if the file has it).
2. Compute `pixel_sha256`. If the input store already has an image with that
   hash (pixel index, below), reuse it.
3. Otherwise allocate an image ID and write the PNG with these chunks:
   - `increart:id` (`tEXt`): the image ID.
   - `increart:pixels` (`tEXt`): the pixel hash.
   - `increart:blake3` (`tEXt`): the BLAKE3 pixel hash.
   - `increart:usage` (`tEXt`): the use counts, `ref=000000000 start=000000000`
     (see Use counts).
   - `increart:source` (`iTXt`, JSON): what the original was, e.g.
     ```json
     {
       "version": 1,
       "original_name": "glaze.jpg",
       "original_path": "/home/user/Downloads/glaze.jpg",
       "original_format": "jpeg",
       "original_size": 482113,
       "original_sha256": "<sha256 of the original file bytes>",
       "original_mtime": "2026-10-19T08:12:44Z",
       "original_ctime": "2026-10-19T08:12:44Z",
       "original_atime": "2026-10-20T17:03:10Z",
       "original_created": "2026-10-19T08:12:43Z",
       "original_mode": 420, "original_uid": 1000, "original_gid": 1000,
       "original_width": 3024, "original_height": 4032,
       "imported": "2026-10-21T09:00:00Z",
       "conversions": ["exif-orientation: rotate 90", "icc: Display P3 -> sRGB"]
     }
     ```
   - `eXIf`: the original EXIF block, unchanged.

The original file's times (modified, status changed, last accessed before
the import read it, and birth time where the filesystem keeps one),
permission bits and owner are read before its bytes; those not known are
left out, and images imported before they were recorded don't have them. The
original file's bytes are not kept. When an identical picture is imported
again, the store file keeps the first import's `increart:source`; each idea's
`source` still records where its copy came from.

**Outputs are PNG only.** Requests set `output_format: "png"` (also
sd-server's default). IncreArt then adds `increart:id`, `increart:pixels`,
`increart:blake3`, `increart:usage` and `increart:recipe` as extra chunks without re-encoding the pixels, keeps
sd-server's `parameters` chunk, and writes the result to
`output_images/<image-id>.png`.

`increart:id` lets the app identify a file reached through a hard link or a
copy (see Links), whose own name is not the ID. `increart:pixels` is checked
against a real hash when an image is first indexed. The **pixel index**
(`pixel_sha256 → image ID`, per store) is kept in `cache/`, and rebuilt from
the `increart:pixels` chunks by a scan when missing.

**Clean up.** Deleting a project or a discarded idea only removes links, so
store images can become unreferenced. A **Clean up library** command (never
automatic) marks every image still referenced (links, `idea.rsn` images,
`run.rsn` refs, recipes' `refs` / `init` / `parent`, `FromOutput` sources)
and moves the rest to the system trash. An input image used as a ref by any
kept output therefore survives even after its idea is gone.

## Scheduling (same as Yue2, plus model switching)

Runs are queued; each free server takes the next pending seed of the
unfinished run that has had the fewest seeds dispatched since it was queued
(round-robin), so all unfinished runs share the GPUs. A newly queued run
therefore starts producing images right away instead of waiting behind a long
batch (at ~34 s per seed, a 256-seed run would otherwise block everything for
over an hour). A server that errors is backed off and retried; its seed goes
back to the pool. Stopping a run comes in two kinds, on each run in Jobs, in
its details window, and for every run at once in the Jobs header:

- **Finish & stop** drops the pending seeds and cancels in-flight jobs that
  are still waiting on the server, but lets the job already generating finish
  and keeps its image (the run shows "finishing N images…" until then). The
  worker tells the two apart by the job's last polled status.
- **Stop now** = `jobs/{id}/cancel` on every in-flight job + drop pending
  seeds; images being generated are thrown away. **Stop all now** asks first.

- **Shallow submission**: at most 2 jobs in flight per server (one generating,
  one queued). Everything else stays in IncreArt's own pool. This keeps
  cancel cheap, stays well under the 64-job limit, and means results are
  fetched promptly (before the job TTL expires).
- **Ties**: among runs that are equally due, the one whose last seed went
  out longest ago goes first, so every round visits the runs in queue order
  (a style sweep on one GPU fills its contact sheet style by style, column
  by column). There is no per-server affinity: with sd-server's conditioning
  cache at 32 entries, alternating between runs rarely costs a re-encode.
- **Order**: the toggle in the jobs popover and Projects → Jobs picks
  **Spread evenly** (the round-robin above; the default) or **One at a
  time** (every free slot goes to the earliest queued run with seeds left,
  so runs finish one after another in queue order). It is saved in the
  config (`job_order`) and applies to the running queue at once. Queueing a
  style sweep while it is One at a time asks first: **Spread evenly &
  queue**, **Keep one at a time & queue** or Cancel (one at a time would
  fill the contact sheet a style at a time).
- **Model switching**: a run whose model selection differs from a server's
  loaded model marks that server *draining*. It gets no new submissions; once
  its in-flight jobs finish, it calls `models/load` (409 means "not drained
  yet, retry") and then resumes. The other server keeps working on whatever
  matches its model. Avoid switching both servers back and forth: prefer the
  server that is already idle or loaded with the target model.
  Concretely: work for a model no server has makes one server drain for it
  (an idle one first, else the one with fewest jobs in flight), but never
  the last server of a model that still has pending work; a server with
  nothing to do for its own model switches to the most due one. With a
  single server, runs for a second model therefore wait until the first
  model's work is done (no reload ping-pong). A model whose load fails
  leaves its runs waiting (shown as stuck) instead of retrying in a loop.
- **Warm-up**: after launch or load, each server runs one small throwaway job
  (e.g. 512², 1 step) so lazy weight loading doesn't land on the first real
  seed.

32 GB per GPU fits the int8 diffusion model + int8 text encoder;
`--offload-to-cpu` is a per-server config option if it doesn't.

## Filesystem layout

```
<library_root>/                       e.g. /mnt/storage/sdcpp/increart_lib
  library.rsn                         format version + link mode (see Links)
  library.lock                        held while the app has the library open
  input_images/<image-id>.png         input image store (PNG, see Image stores)
  output_images/<image-id>.png        output image store (PNG)
  input/                              input queue + input library
    inbox_drop/                       watched; anything dropped here becomes an idea
      imported/                       originals already imported
    unreviewed/<idea-id>/             Idea inbox
      idea.rsn
      1-glaze.png                     link → ../../../input_images/<image-id>.png
    library/<folder>/…/<idea-id>/     input library: folder tree of kept ideas
    discarded/<idea-id>/              discarded ideas (recoverable)
  projects/                           folder tree (see Project folders)
    <folder>/…/<project>/             output queue + output library (per project)
      project.rsn                     project id + source idea ids (+ cover image, image titles)
      runs/<run-id>/run.rsn
      unreviewed/<run-id>_r<rev>_s<seed>.png  link → output_images/<image-id>.png
      reviewed/{good,neutral,bad}/...         links
  collections/<folder>/…/<image-id>.png  output-library collections: links into output_images/
  thumbs/{input,output}/<image-id>.webp   shared cache, deletable
  cache/pixel-index.rsn               pixel hash → image ID, transparent images; deletable
  cache/conditioning/                 future: disk conditioning cache (needs sd.cpp patch)
```

The output library is the union of every project's `reviewed/` folders; the
input library is every idea under `input/library/`. Both are made of links;
the images themselves are in the stores.

Output links are named `<run-id>_r<rev>_s<seed>.png`, e.g.
`2026.10.19-09.00-0000_r0_s0003.png`: run `2026.10.19-09.00-0000`, revision
0, seed 3. The seed is zero-padded to at least 4 digits (`s0042`). The
`r`/`s` markers keep the name from reading like an image ID; within a project
the links sort by run, then revision, then seed (in numeric order as long as
the seeds in a run have the same number of digits, which padding guarantees
below 10000). The image's own ID is the link's target.

### Links

Links are **relative** (e.g. `../../../../output_images/<image-id>.png`), so the
library can be moved or mounted elsewhere. Their kind is fixed per library in
`library.rsn` (`link_mode`), chosen when the library is created by trying, in
order:

1. **Symlinks**: the default on Linux and macOS. Clearly show which file is
   the real one.
2. **Hard links**: no special permission needed on Windows, but only on NTFS
   and within one volume (the library is one directory tree, so that holds).
   They look like ordinary files to every tool; `increart:id` identifies them.
3. **Copies**: last resort, e.g. on FAT32/exFAT drives, which support no
   links at all. Works, but uses the space twice.

Windows: symlinks are supported on NTFS, but **creating** one needs
Developer Mode or administrator rights (following them doesn't), so an
ordinary Windows install usually ends up on hard links. A library on a Linux
machine shared to Windows over SMB keeps its symlinks: Samba follows links
that stay inside the share by default, and these do.

The app treats all three the same: it reads a link's image ID from the
target path (symlink) or from `increart:id` (hard link, copy).

Review moves files or idea folders between folders (like Yue2); the folder is
the source of truth for the verdict on both sides (for ideas: inbox, library
or discarded; the star lives in `idea.rsn`). A `notify` watcher picks
up external moves/deletes. Deleting goes to the system trash.

### Drafts

Forms that haven't been submitted yet are autosaved **2 s after the last
edit**, and immediately when switching projects, adding the idea, or
quitting:

- **Generate**: one draft per project, `projects/<project>/draft.rsn`
  (`GenerateDraft { label, prompt, negative_prompt, width, height, preset,
  steps, cfg, flow_shift, seeds, first_seed, alpha, refs, source_idea }`; the
  model preset by name). Selecting a project restores its draft (a fresh
  form if it has none); the label moves on if a run already uses it, and the
  first seed continues after the project's seeds unless the draft chose 0.
  A project with runs but no draft (e.g. runs made before drafts existed)
  starts from its latest run's prompt, refs and settings.
  Queueing keeps the draft (with the next label and seed) so a variation is
  one edit away; **Clear** resets it. Copying a project copies its draft.
- Run labels count from `r00` (a sweep counts once), always past the highest
  `rNN` the project already has, so libraries numbered from `r01` carry on.
- **New idea**: `input/draft.rsn` (`IdeaDraft { title, text, tags, images }`;
  the images are already in the input store). Restored when the library is
  opened, removed when the idea is added.
- **Clean up** counts draft refs and draft images as references.
- The forms show "unsaved changes…" while a save is pending and "draft
  saved" for a moment after.

### Generate history (prompt history)

The last **1000** substantial edits are kept and can be viewed and restored
(`increart_core::history`), as JSON lines next to what they belong to:

- a project's Generate form: `projects/<project>/prompt-history.jsonl` (the
  name from when it held only the prompt; a duplicated project starts with an
  empty history). Each entry has the prompt and negative and, as `form`, the
  whole form as the autosaved draft has it: size, model preset, steps, cfg,
  flow shift, seeds, the image list with each image's roles, strength,
  source idea and sweep settings. A **queued** checkpoint has `runs`, the
  IDs of the runs it made (a sweep's all). Every entry has `after`, the
  project's newest run before it (an edit "since r05"). Entries from before
  this have only the texts: their runs are found by the label in "queued
  r05" (or the sweep's name), and `after` by time;
- the New idea form (text and title): `input/new-idea-history.jsonl`;
- an idea's text (Edit idea): `<idea folder>/text-history.jsonl` (moves with
  the idea).

When an entry is made:

- **edit**: at a pause of 2 s (the draft autosave), after typing or after
  changing a setting. Identical to the newest entry (texts and settings;
  the label and first seed, which move on by themselves, don't count):
  nothing. Fewer than ~20 characters changed (word diff) within 60 s of the
  previous edit: the previous edit is **replaced** instead of adding one, so
  typing in bursts gives one entry per burst; a change to settings alone
  counts as small, so trying steps or cfg values gives one entry too.
- **checkpoints**, never merged and trimmed last: **queued r05** (the form
  a run or sweep was queued with, recorded once the library has made the
  runs), **before clear**, **before restore**, **before tag update**,
  **before preset**, **saved** (an idea added from the New idea form, or
  saved in Edit idea). A project's checkpoints have the whole form too.
- The file is appended to (a merge rewrites the last line) and compacted
  to 1000 entries once it reaches 1100, dropping the oldest plain edits
  first.

**History…** (next to the prompt, in New idea, and in Edit idea) opens a
window: entries newest first (age, kind, first line), search, "checkpoints
only", a word diff of the selected entry against the current text, and
**Restore** / **Restore with title** / **Copy**; ↑/↓ browse, Enter or
double-click restores. Restoring first records the current text or form
("before restore"), so it can itself be undone from the history.

A project's window, **Generate history**, also shows the settings and runs:

- Each row: the settings changed since the entry before ("steps 20 → 30 ·
  +1 image · 1024×768"), or "prompt only" for an older entry; an edit's
  row says which run it followed ("since r05"); a queued row has a chip per
  run, in the run's current label (hover: project, model, prompt, images
  made and reviewed). A click on a chip opens the run's details, its images
  button the run's images in the Output library (a source of its own,
  "run r05 of fox-cafe"). A sweep's runs are one chip, "r05 – r12 · sweep
  styles", opening the sweep's contact sheet. A run since deleted is "deleted
  run", struck through.
- The selected entry: "queued as · 12 images, 4 reviewed" with its chips,
  the word diff, and its settings compared with the current form.
- Filters: **settings changes** (entries whose settings differ from the
  entry before), and **led to:** a run, which shows only the entries since
  the run before it, up to that run's queued checkpoint.
- **Restore all** (Enter, double-click) puts back the prompt and every
  setting; the label and first seed stay the next run's. Images no longer in
  the library are left out and a model preset that's gone keeps the current
  one, with a message. **Restore prompt only** and **Restore prompt +
  negative** keep the settings. Restoring opens the project in Generate if
  it isn't. History entries don't protect their images from Clean up.
- **Show in history** in a run's details (Jobs, the history's chips) and
  **history** next to the run in the Output library's lineage panel open
  the project's history filtered to what led to that run, at its queued
  checkpoint.

### Prompt presets

Ready-made prompts for the Generate form, by task: **Modify style** (69
styles under headings: Anime & cartoon, Painting, Drawing, 3D & craft, Photo &
film, Retro & print, Graphic), **Remove background**, **Repair / restore**,
**Edit content**, **Relight / mood**, **Extend / reframe**, **Character**,
**Combine images**, **Text**; 106 built in, in
`crates/increart-core/default-presets.rsn` (embedded in the app; its header
notes the conventions below). They are shared by every library, in a
`prompt-presets` folder next to `config.rsn`:

```
prompt-presets/
    default-presets.rsn            the built-in set, rewritten on every start
    user-state.rsn                 favourites, deleted built-ins, user tasks
    Modify style/                  a folder per task (name made file-safe)
        3D & craft/                a folder per group, if the preset has one
            Fusilli.rsn            one preset per file
            Claymation.rsn         a built-in one, changed: it replaces it
    Remove background/
        Mine.rsn
```

Only what differs from the built-in set is written: a changed or added preset
is a file (its task, group, name, description, prompt and negative); a deleted
built-in one, a favourite, a user task (and its description, or a built-in
task's changed one, or a task kept with no presets) are in `user-state.rsn`.
So presets added to the built-in set in a new version show up, and a file
copied into a task folder (from someone else) is a new preset: a hand-made one
may leave out `task` and `group`, which then come from its folders, and `name`
(the file name). Loading puts the built-in set first, in its order, minus the
deleted; a user preset takes the place of the built-in one of its name when in
the same group, else goes after the last preset of its group (or at the end of
the task), in name order; user tasks come after the built-in ones, in name
order. So there is no reordering. Saving removes the files of presets no
longer there, and folders left empty.

A preset file that doesn't load is reported and left alone (skipped, never
written over or removed); a `user-state.rsn` that doesn't load is reported
and the built-in set is used, nothing saved. A `prompt-presets.rsn` (the
single file of earlier versions) is moved into a new folder on the first start
and renamed `prompt-presets.rsn.old`.

A preset has a name, an optional group (a heading within its task), a
description, the prompt, an optional negative, and a favourite flag. The
description loosely defines the name for someone who doesn't know the term,
then says what you get ("Claymation: stop-motion animation with plasticine
figures: soft clay, fingerprints, slightly lumpy handmade shapes"). Artist,
studio and brand names are left out of the built-in set.

Prompts name reference images `<image1>`, `<image2>`… and leave blanks as
`{slot}` (a short name in braces: `{colour}`, `{scene}`). Modify style prompts
read "Convert <image1> into …. Keep the composition, pose and identity
unchanged." Styles that turn a flat image into 3D (**Voxel**, **Animatronic**)
keep only "the subject, pose and identity", so the camera may change, and
carry a negative prompt (Voxel's "pixel art, 2D, flat, sprite, pixelated";
Animatronic's also rules out drawings and a generic sci-fi android): kept to
the composition, the model kept the flat front view and voxels came out as 2D
pixel art.

- **Presets…** (next to **History…** above the prompt, or the prompt's
  right-click menu) opens a window over the images pane (resizable both
  ways; the lists take its height): ★ Favourites and the
  tasks on the left (task descriptions in their tooltips), the presets on the
  right (name, description underneath, the full prompt in the tooltip), and a
  search over names, descriptions, groups and prompts in every task.
- Click a preset to **replace** the prompt (and the negative, when the preset
  has one); the old prompt goes to the prompt history ("before preset").
  **Shift+click** inserts it at the prompt's cursor instead. The first
  `{slot}` of what came in is selected, ready to be typed over; with none,
  the cursor goes after it.
- A preset needing more references than the form has (its highest
  `<imageN>`) is greyed out, its tooltip saying how many.
- While the prompt has a `{slot}`, a red line under it names them and
  **Queue** refuses, selecting the first one.
- ☆ / ★ marks a favourite (saved at once).
- **Edit presets…**: a modal over a working copy. Tasks (add, delete,
  rename, description), the task's presets (add after the selected one,
  duplicate, delete), and the selected preset's name, group
  (typed, or picked from the task's groups), task (moves it), description,
  prompt (with the references and slots it needs below), negative. **Restore
  defaults** puts the built-in set in the working copy (saved, it removes
  every user preset file). **Save** checks that
  tasks and presets have names, unique (ignoring case; presets within their
  task), and prompts; **Cancel** drops everything.
- **Save as preset…** (above the prompt, the prompt's right-click menu, or
  the window's **Save prompt as preset…**): the prompt and negative as a new
  preset: task (the one shown in the window), name (unique in the task),
  group, description.

### Sweeps

Trying one prompt in many styles, or at many values of a setting, at once. A
sweep is a group of ordinary runs, one per prompt preset or per value, made
by one **Queue**; nothing about generating or storing them is special.

- Generate's **Mode**: **Single prompt** | **Sweep**, and the sweep box's
  **Vary**: **Style** (a prompt preset per run, below) or a setting (see
  *Setting sweeps*). In a style sweep the prompt is a **template**: `{preset}` marks where each preset's prompt goes
  ("cute robot mascot sticker, {preset}"); a template without it is replaced
  by the preset's prompt (a note says so).
- **Styles from**: the presets ticked in the Presets window (in sweep mode a
  click there ticks or unticks instead of using the preset; **Tick all** and
  **Untick all** act on the presets listed, the task, favourites or search
  results, Tick all leaving out those needing more reference images than the
  form has; **Clear all ticks (N)** unticks every one). In a style sweep the
  window's task list has **☑ Ticked (N)** under ★ Favourites: every ticked
  preset, under its task's heading, in the list's order (no Tick all / Untick
  all there). One unticked there stays listed, to tick again, until another
  view is chosen; ticks whose preset was renamed or deleted (skipped by the
  sweep) come last under **No longer there**, struck through, to untick. The
  window opens on Ticked when something is ticked. Tasks with ticks show
  "(ticked / presets)", and a task's group headings "· N ticked". Or ★
  Favourites, a whole task, or a group within a task, each with its count. **Use** all of
  them or a random K; the pick is repeatable (its seed is kept in the draft)
  and **Reroll** picks again.
- Presets needing more reference images than the form has are left out, and
  the panel says how many. `{slot}`s of the template and the chosen presets
  are asked once, under the source, and filled in every run; Queue refuses
  while one is empty. Under each slot a note says what needs it (the template,
  and/or the chosen presets by name), each preset with **Replace** (a random
  pick takes the next style in its order instead; only when one is left) and
  **Omit** (one style fewer). Either leaves the rest of the pick as it was;
  the removed ones are listed with **Put back**, and **Reroll** or another
  source puts them back. A preset's negative, when it has one, replaces the
  form's.
- The summary reads "K styles × N seeds = X images ≈ time"; above 100 images
  Queue asks first. Every style gets the same seeds, so a column of the
  contact sheet compares styles on one seed.
- The sweep takes the form's label (`r05`, one number for the whole sweep);
  its runs are labelled `r05 · Claymation` and tagged in `run.rsn`
  (`sweep: Sweep { name: "r05", task: "Modify style", preset: "Claymation" }`).
  The scheduler's round robin already starts every style before any gets its
  second seed.

**Setting sweeps.** Vary **Steps**, **CFG**, **Flow shift**, **Strength**
(only with a start image) or **Model**: a run per value, everything else as
the form has it, the same seeds in each.

- **Values**: typed (`8, 12, 20, 30`; commas, spaces or semicolons), sorted,
  repeats dropped; **from … to … by … Fill** writes a range (at most 50).
  Choosing a setting starts from values around the form's own (steps 20 →
  `10, 15, 20, 30, 40`; CFG 2 → `1, 1.5, 2, 3, 4`; strength `0.3 … 0.9`).
  Steps are whole, 1–200; CFG 0–30; flow shift above 0 to 20; strength above
  0 to 1. A wrong value is named under the field and Queue refuses.
- **Model**: a checkbox per model preset (the form's ticked to start); each
  run takes its preset's own steps, CFG and flow shift, so models are
  compared at their own settings.
- The prompt is a plain prompt: `{preset}` is refused, other `{slot}`s must be
  filled in. The summary reads "K values × N seeds = X images ≈ time", the
  time summed run by run (steps change it); at CFG 1 or below a note says
  the negative prompt isn't used.
- Runs are labelled `r05 · cfg 3` and tagged with the value
  (`sweep: Sweep { name: "r05", task: "", preset: "", param: SweepParam {
  name: "cfg", value: "3" } }`; `name` is `steps`, `cfg`, `flow_shift`,
  `strength` or `model`, whose value is the preset's name). Older runs
  without `param` read as style sweeps.
- On the contact sheet each row is a value ("20 steps", "cfg 3", "shift 3.1",
  "strength 0.6", a model's name), in order; a CFG row at 1 or below says
  "no negative prompt". The row's button is **Use these settings**.
- Tiling the sheet's picks keeps rows and columns, so with the **style /
  setting · seed** caption ("cfg 3 · seed 5") the tiled image is a labelled
  comparison chart; **settings** captions any image with its run's
  "20 steps · cfg 3 · shift 3.1 · strength 0.6".

**Review.** A project's runs are listed under it in Review's scope, newest
first: a single run by its label (`r03 (unreviewed)`, scoping
Review to that run's images), a sweep once (`sweep r05 (unreviewed)`). Only
the selected project's runs are shown, unless **runs of all projects** is
ticked (kept for the session). The sweep scope adds a **Contact sheet** view: a
row per style, a column per seed, the newest revision of each seed; `…` for
an image still to come, `–` for one cancelled or failed. Cells fill in as
images arrive.

- Click selects a cell, double-click opens the image; 1/G, 2/N, 3/B rate the
  selected image and move right; arrows move; Ctrl+Z undoes.
- Each row: the style, its counts, **G N B** (rate the row's *unreviewed*
  images only), and **Use this style** (its run in Generate as a single
  prompt, with the next label and seed; the old prompt goes to the history).
- **N more seeds for K kept styles** adds a revision of N seeds to every
  style with a Good image (of the styles shown).
- **Filter by name**: only the rows whose style (or run label) has every
  word typed, any case; the seed columns stay. Esc or ✕ clears it; leaving
  the sweep clears it too.
- **Zoom** (− / slider / +, or Ctrl+scroll over the sheet): cells from 64 to
  320 points, the thumbnails' own size, so they stay sharp. Kept in
  `config.rsn` (`sheet_cell`).
- Leaving the sweep scope leaves the sheet for Single.
- Looking at a sweep's image anywhere in Review (Single, Grid, A/B compare),
  the top bar shows **Sweep r05 contact sheet**: it opens that sweep's sheet
  with the image selected.

### Tiling

Several images can be picked in Review (Grid, the contact sheet, Single's
filmstrip) and in the Output library's Grid, then rated together or laid
out as one image.

- **Picking**: Ctrl+click toggles an image (starting from the one the cursor
  is on), Shift+click adds the range from the last picked, a drag on empty
  space draws a box (Ctrl+drag adds to the picks), Ctrl+A picks everything
  shown. A plain click, a click on empty space or Esc drops the picks; so
  does another tab, scope or source. Picks are numbered in the order picked,
  which is the order they are tiled in.
- While anything is picked, a bar shows **N picked · Good Neutral Bad ·
  Tile… · Clear**, and in Review 1/G, 2/N, 3/B rate every picked image (each
  is a separate Ctrl+Z). Rated images leave Review's queue, so the picks go
  with them there; the contact sheet keeps them.
- **Tile…** (T) opens the Tile window. Picks from the contact sheet keep
  their rows and columns (holes included); others start about square
  (⌈√n⌉ columns). Drag a tile onto another to swap them, or onto any empty
  cell (an extra row below takes drops); click one to give it a caption or
  a span (2 × 1, 2 × 2, …: the tiles in its way move to free cells).
  **Columns** re-places what no longer fits; **Reflow** packs the tiles in
  order.
- **Text blocks**: right-click an empty cell (or double-click it) → **Add
  text here**. A text block is a tile like an image (drag, span, remove)
  holding lines of text, word-wrapped and centred in its cells, in the
  caption colour; it has no caption and isn't a source.
- **Title / subtitle**: lines centred above the grid (a gap below them), the
  subtitle at half the title's size, in the caption colour. Their text
  belongs to the layout, not the settings.
- **Text sizes** (captions, the title, each text block) are **auto**, a
  **% of the cell**'s height, or exact **px**; the panel shows what each
  comes to. Auto: captions 5 % of the cell; the title 4 % of the tiled
  image's width, at least 10 % of the cell (so more columns, a larger
  title); a text block as large as fits its cells, up to the title's size.
  Auto and % follow the scale, so 25 % looks like 100 % only smaller.
  Switching keeps the size it came to. A caption too wide shrinks (down to
  60 %) before it's cut short with "…".
- Cells are the size of the largest image (at the chosen **scale**: 25 /
  50 / 100 % or any), with a **gap** between and around them; images
  **fit** (letterboxed) or **fill** (cropped to the cell). **Background**:
  transparent, dark, white, black or any colour. **Captions**: one line
  under each tile (shrunk, then cut short with …), all filled from the style or setting
  (a sweep's row: "Claymation", "cfg 3"), that and the seed, the seed, the
  settings ("20 steps · cfg 3"), title, run label or prompt, then edited
  one by one; size (above) and colour. A caption can hold placeholders, filled per
  image when shown and composed: `{n}` (the tile's number), `{style}`,
  `{seed}`, `{settings}`, `{steps}`, `{cfg}`, `{shift}`, `{strength}`,
  `{title}`, `{run}`, `{prompt}`, `{project}`; unknown ones stay as typed.
  **Format … Apply** gives every image the same one (the format is kept in
  `tile`). The
  settings are kept in `config.rsn` (`tile`); the layout is not.
- **Copy** puts the tiled image on the clipboard, **Save PNG…** writes a
  file, **Add to input library** makes a new idea at the top of the input
  library with the image, its source `Tiled { images }` (they count as used,
  so Clean up keeps them). Composing reads the full-size images on its own
  thread; over 250 MP is refused.

### Opening the file manager

**Open in file manager** / **Show in file manager** (folders, projects,
idea folders, collections, the drop folder, images, the library path in
the status bar) opens the folder (a file's folder) with `file_manager`
from `config.rsn` if set (the command, the folder appended), else the
first of `xdg-open`, `gio open`, `thunar`, `nautilus`, `dolphin`, `nemo`,
`pcmanfm` that works; if none does, the status bar says how to set one up.

### Project folders

Projects are organised in a tree of folders and subfolders, and the tree is
real directories under `projects/`, so it looks the same in a file manager.

- A directory containing `project.rsn` is a project; any other directory is a
  folder. Projects don't nest inside projects.
- A project's identity is the ID in `project.rsn`, never its path. Ideas
  (`projects: Vec<ProjectId>`), recipes and the scheduler refer to projects
  by id, so renaming or moving a project or folder breaks nothing. The
  `ImageRef` points into a store, never into a project, so it is unaffected.
- The scheduler resolves a project's current path when it writes a result, so
  moving or renaming a project mid-run is safe.
- Names are directory names: not empty, no `/` or `\`, not starting with `.`,
  and unique among siblings, case-insensitively. Folders and projects share
  one namespace per parent.
- Tree operations (right-click on a folder, a project, or the empty area for
  the top level):
  - **New folder**: created with a free name ("new folder", "new folder 2",
    …) and opened for rename.
  - **New project**: asks the name first (the same modal, titled New
    project, prefilled with a free "new project" name); the project is made
    on **Create**, and Cancel / Escape makes nothing.
  - **Rename**: a modal over a dimmed window (also from `F2` and the
    folder views, and for collections). Enter or **Rename** commits;
    Escape, **Cancel** or a click outside cancels. An invalid name disables
    Rename; a clashing one is refused with a message.
  - **Copy** / **Cut** then **Paste here** on a folder (or the top level).
    Cut + paste moves (a directory rename). Copy duplicates a project's setup
    — `project.rsn` with its sources and run definitions, as unqueued
    templates — but **not** its images; copying a folder copies its subtree
    the same way. A pasted name that clashes gets a number suffix. Pasting a
    folder into itself or a descendant is refused.
  - **Delete…**: confirmation showing the folders, projects and images
    inside and how many pending seeds will be cancelled, then the whole
    directory goes to the system trash. That only removes links; the images
    stay in `output_images/` until **Clean up library**.
  - **Drag and drop** moves, like cut + paste: drop on a folder to move into
    it, on a project to move beside it, or on "All projects" / the empty
    area for the top level. The target is outlined blue, or red when the drop
    is refused (a folder into itself); dropping where the item already is
    does nothing.
- Keyboard shortcuts act on the selected node, on the Projects tab, whenever
  no text field has focus: `F2` rename, `Delete` delete (with the same
  confirmation), `Ctrl+C` / `Ctrl+X` copy / cut, `Ctrl+V` paste into the
  selected folder (or beside the selected project, or the top level). These
  don't clash with Review's keys: Review uses no F-keys or Delete, and
  egui delivers Ctrl+C/X/V as clipboard events, not key presses, so they
  never trigger Review's `C`. Copy/Cut also put the node's path on the
  system clipboard as text, because egui only reports Ctrl+V when the
  clipboard isn't empty.
- Selecting a folder scopes Review to every project beneath it (the "all
  projects" view is the top level). Generate needs a single project.
- Folders collapse: the caret before a folder (or a double-click on it)
  hides what it holds, and **Collapse all folders** / **Expand all folders**
  on "All projects" act on the whole tree. A collapsed folder shows its
  unreviewed count and a sparkle when one of its runs is active, and is
  accent-coloured when the open project is inside. Searching shows matches
  inside collapsed folders without expanding them. Which folders are
  collapsed is remembered per library in the config
  (`collapsed_folders`); deleted or renamed folders drop out.
- New projects from Promote land, unless another location is chosen, in a
  folder of their own name where the last promote went, else at the top level.

### Input library folders

The input library has the same kind of folder tree as projects, stored as
directories under `input/library/`, with the same tree operations, drag and
drop and shortcuts (see Project folders). The differences:

- Leaves are ideas. An idea's directory is its ID, so folder names must be
  unique among sibling folders but idea titles may repeat. Renaming an idea
  edits its title (free text, may be empty).
- Kept ideas from the Idea inbox land at the top level; filing them is a
  separate step.
- **Copy** duplicates an idea (new ID, same text, variants and tags;
  new links to the same store images, nothing is copied; `projects` starts empty). Copying a
  folder copies its subtree.
- **Delete** on an idea discards it (to `input/discarded/`, undoable with
  `Ctrl+Z`), with no confirmation. Deleting a folder asks first, then removes
  the folder and discards every idea inside; `Ctrl+Z` brings those ideas back
  at the top level.
- Cards in the grid are drag sources too: dragging a selected card carries
  the whole selection, so several ideas can be filed at once. With several
  cards selected, `Delete` and `Ctrl+C`/`Ctrl+X` act on all of them.
- Selecting a folder limits the card grid to that folder and its subfolders;
  the other filters (text, starred, has images) apply on top.

## GUI

Top-level tabs, in pipeline order: **Idea inbox** · **Input library** ·
**Projects** (Grid · Folder · Generate · Jobs; shows the project count, and
✦ while generating) ·
**Review** (the output queue) · **Output library**. Review is top-level, like
the Idea inbox, because it is the busiest screen during batches and works
across projects. Clicking a server status in the top bar (ROCm0 ●, PE …) opens the
server status window (as the Servers… button does); the PE status's tooltip
shows the rewrite queue. The statuses take the room left of the top bar's buttons: in full
(`ROCm1 ◐ warming up`), then name and dot, then dots only; with no room for
those the Servers… button shows the worst dot (and, in a narrow window, only
its icon; the library name is cut to 10 characters). Tooltips give the full
status. The window is at least 1000 × 640.

Each tab label is an icon, a Title Case name and a count: 📥 **Idea Inbox
(7)**, 📚 **Input Library**, 🚀 **Projects** (✨ while runs generate),
👍 **Review**, 🖼 **Output Library**. When the full labels would leave the
right side less than its compact width (a narrow window or a large zoom),
every tab drops its name and keeps its icon and count; the tooltip names it.

**Back / forward** (← → at the top left, `Alt+←/→`, the mouse's side
buttons): a history of places like a browser's. A place is the tab, the
sub-tabs, and what the side panels select (project or project folder,
input-library folder, Review scope, Output library source); each user action
that changes it is one step (Show outputs, which switches tab and source, is
one). A new step drops the forward ones; up to 100 are kept. In a folder view
the keys and mouse buttons go through that view's own history instead.

- **Idea inbox**: tabs **New idea** · **Grid** · **Folder** · **Import** ·
  **Discarded**. (PE rewrites have no tab; see Prompt rewriting.) Files dragged from the desktop onto any Idea
  inbox tab are imported as new ideas (images re-encoded as usual, text files
  become the idea's text); the window shows a drop highlight while dragging. Opening the Idea inbox shows **New idea** when there are no
  ideas to review, **Grid** otherwise. Shows the count of unreviewed ideas as a
  badge on the top-level tab.
  - **Grid** (input review): idea cards (title, first images, text, variant
    count, PE rewrite state), newest first. Idea cards here and in the Input
    library are all one size: the first image square across the full width
    (a "T" when there is none), then the title, a line of counts, two lines
    of text and the tags, each cut to fit. On every idea card (here, the
    Input library and Discarded), double-click shows the first image full
    size (the editor when there's no image), and a pencil button on the
    image (on hover, or when selected) opens the editor. Keys as in the
    table above, plus `Enter`/`T` = open the **Edit idea** window (title, tags,
    text, variants, images, and a finished PE rewrite's diff with Accept /
    Reject). Right-click a card: keep in input library ★ / keep in input library / discard, promote, rewrite
    with PE, accept all ready rewrites, edit, copy text.
  - **New idea** (`Ctrl+N` from anywhere): title, text, tags, images (drop /
    paste / browse), **Rewrite with PE**. **Add to inbox** (review it later),
    **Add & keep** (straight into the input library, into a chosen folder),
    **Add & promote** (keep it and start a project). The draft is kept while
    switching tabs.
  - **Folder**: the file-manager view (see Projects → Folder) rooted at the
    inbox's own places only: `inbox_drop/` (waiting files, and `imported/`
    originals), `unreviewed/` (one folder per idea, holding its image links)
    and `discarded/`, with a short Places list instead of a tree. Its
    location and history are its own, independent of Input library →
    Folder. Moving an idea folder between `unreviewed/` and `discarded/` (drag,
    or cut + paste) discards or restores it, undoably; `Del` on a discarded
    idea asks to delete it permanently. `unreviewed/` has no subfolders: the
    inbox is a queue, filing happens in the library. Files in `inbox_drop/`
    can be opened (= imported now) or deleted; `imported/` originals can be
    viewed or deleted. `F2` on an idea opens the Edit idea window.
  - **Import**: drop-folder watcher status and files waiting (with
    **Import now**); recent imports with what changed on import (from
    `increart:source`) and duplicates detected by pixel hash ("existing
    image reused"); import settings (strip GPS, delete `imported/`
    originals after N days, clear them now; see Open questions).
  - **Discarded**: discarded ideas (`input/discarded/`), newest first.
    Right-click: restore to the Idea inbox, restore to the input library,
    delete permanently (asks first; the idea's folder goes to the system
    trash, its images stay until Clean up). **Empty discarded…** deletes them
    all. Restores can be undone with `Ctrl+Z`.
- **Input library**: tabs **Grid** (default) · **Folder** (see Projects →
  Folder) · **Discarded** (the same Discarded view as in the Idea inbox). In
  Grid, the left panel holds the filters (tag / starred / text /
  has-images / promoted-or-not), selection actions, and the library folder
  tree (see Input library folders); the grid shows the selected folder's
  ideas, newest first (or most used first); cards show PE rewrite state like the inbox's. Multi-select → promote
  (button or `Space`); drag an image into a project's reference strip; drag
  cards onto a folder to file them. Right-click a card: star/unstar, promote,
  add its images as refs to the current project, rewrite with PE, accept all
  ready rewrites, edit, copy text, back to the Idea inbox, discard. When the card is part of a multi-selection, the
  bulk actions (star, promote, rewrite, back to inbox, discard) apply to the
  whole selection. A plain click on a card opens the **idea panel** on the
  right: the idea's images (double-click: full size), text, tags, created
  date and folder, what its images were used for, the projects it was
  promoted into (links), and Edit…, Promote, Add as refs, Copy text, Open
  folder; × closes it. While the panel is closed it opens a moment after
  the click, so a double-click (full size) doesn't open it; while it's
  open, a click shows the clicked idea at once. Ctrl/Shift+click only
  selects.
- **Projects** side panel: **New project** and **New folder** (at the top
  level; a folder's right-click menu makes them inside it) at the top, then a search field that narrows the tree to
  the projects and folders whose names contain the text (a matching folder
  shows all it holds; `Esc` or × clears it), **runs** (list the selected
  project's runs under it), the project order (**Newest first** or
  **A–Z**, any case; saved in the config as `projects_by_name`; the grid uses
  it too) and **narrow to siblings** (below; ticked by default, unticked
  saved as `project_click_keeps_scope`), then the tree: in each folder, its subfolders by name, then its
  projects in that order. A **Keys** section is pinned to the bottom (open
  by default, foldable): on the Folder tab the folder view's keys, else an
  opened image's (← → previous / next, rate, undo, E / D / R / I, zoom, Esc),
  and Alt+← → / Ctrl+N from anywhere.
- **Projects → Grid**: a flat overview: one tile per project in the folder
  selected in the tree and all its subfolders, in the tree's order, no
  folder tiles. Selecting a project (in the grid or the tree) narrows it to
  the project's folder only when that holds other projects (here and below)
  and **narrow to siblings** is ticked; otherwise the grid keeps showing what
  it did, if the project is in it, else the project's folder (the one above
  when the project is alone in it, as Promote's own folders are). After
  Back / Forward that last rule decides. Each tile's cover is its best image: highest
  rated (good > neutral > unreviewed > bad), most recent among equals. A
  project with no output image yet shows its references and start images
  (its Generate draft's, then its runs', up to 8) as small thumbnails
  scattered around the empty cover, clear of its icon in the middle; the
  tile also shows the project's folder path. Tiles show the unreviewed count as a badge on the
  cover, good / neutral / bad / unreviewed counts below it, and a
  marker while runs are generating. A breadcrumb changes the scope; click a
  project to select it, double-click it to review it; right-click gives the
  same menu as the tree.
- **Projects → Folder**, and the same as **Input library → Folder**: a
  file-manager view (like Thunar) of the tree's on-disk layout. For projects
  that includes each project's `runs/`, `unreviewed/` and
  `reviewed/{good,neutral,bad}/` with the image links; in the library an idea
  is a folder holding its image links.
  - Toolbar: back / forward / up (`Alt+←/→`, `Backspace`), a path bar of
    clickable segments that turns into an editable path with `Ctrl+L`, a
    metadata toggle (`Ctrl+H`, shows `project.rsn` / `run.rsn` / `idea.rsn`),
    and **Icons** (with a size slider) / **Details** (name, kind, info, size,
    modified) / **Compact** modes. Directories are plain folder icons; image
    files show thumbnails.
  - Side pane: the tree, kept in sync both ways.
  - Selection: click, `Ctrl`-click, `Shift`-click, rubber band on empty
    space, `Ctrl+A`, arrow keys, type-ahead. `Enter` or double-click opens: a
    folder navigates, an unreviewed image opens in Review, a reviewed one in
    the Output library, an idea image opens the Edit idea window.
  - Operations: `F2`, `Del`, `Ctrl+C/X/V`, drag and drop and a right-click
    menu (open, open in system file manager, rename, copy, cut, paste into,
    delete, properties); empty space has its own menu (new folder / project /
    idea, paste, open in system file manager, properties). Folders, projects
    and ideas behave as in the tree. Image links can be moved (cut + paste or
    dragged) between `unreviewed/` and `reviewed/*/` of their own project,
    which is a review (undoable); moving them into another project is
    refused (use → input ref). They can't be renamed (the name carries run,
    revision and seed) or copied; deleting one moves the link to the trash,
    the image stays in the store until Clean up. Project subfolders, idea
    images and metadata files are managed by the app.
  - Status bar: item count, selection count and size, current path.
  - Later: a dual-pane split (`F3`).
- **Projects sidebar**: the project folder tree (see Project folders), with
  per-project unreviewed counts; the selected project lists its runs under
  it, newest first, as in Review's scope (a single run by its label, a
  sweep once as `sweep r02`), each with its unreviewed count and ✦ while
  running. Click: the run's details (a sweep's newest run's); right-click:
  Review this run / the contact sheet, Show outputs (a single run), Details,
  Open in Generate, Show in history. The **runs** checkbox above the tree
  hides them (kept in `config.rsn`, `hide_project_runs`).
- **Generate panel**: prompt (+ **Rewrite** with diff; **Presets…** and
  **Save as preset…**, see Prompt presets), negative, size presets
  from `config.rsn`, editable, plus a one-off custom size (see Config file →
  Resolutions), steps/cfg/flow-shift,
  seeds count and first seed (see Seeds), preset recipe, **Transparent** toggle, reference strip
  (chips tagged `<image1>`…, "N / 9"), with **Add image file…** (system file
  picker; imported into the input store), **From library…** (a picker over
  nearly the whole window, browsing like the library screens: a source tree
  with the input library and its folders, and the output library's
  projects, collections and smart folders (by project as a tree, by date,
  by source idea, transparent); a text filter (idea title, text and tags, or
  the run's prompt); Kept / Good / All for outputs; a thumbnail size slider.
  Click images to choose them, numbered with the `<imageN>` they will be;
  images already referenced are marked and can't be chosen; double-click
  for the full-size preview. The footer counts the references after adding
  (red past 9). **Add N reference(s)** or Enter adds them, Esc or Cancel
  closes, and it reopens on the last source), and image files
  dropped on the panel (imported as references, not as ideas); the img2img
  start image with strength (see Output → input).
- **Status bar jobs indicator** (bottom right): an overall progress bar and
  "N run(s) · done / total seeds" (or "no jobs · N server(s) ready"). Click
  it for a popover: each run in the engine (project · label, prompt, a bar
  over its seeds that counts partly sampled seeds by their steps, one thin
  bar per seed being sampled, **Cancel**), the server states, and **Open
  Projects → Jobs** / **Change servers…**. Cancelled and failed seeds don't count
  towards the total.
- **Interface zoom** (status bar, left of the bell): "+ 100% −" scales the
  whole interface from 70% to 300%. The buttons and Ctrl + / Ctrl − step to
  the next 10%, Ctrl 0 is 100%; the percentage can also be typed (it
  applies on Enter) or dragged. It's kept in the config (`ui_zoom`), not
  per library. egui's own keyboard zoom is off, so the config always has it.
  A divider separates it from the bell.
- **Messages** (status bar, after the library path): the latest one, with
  "(+N more)" when there are others and ✕ to dismiss it. Clicking it opens
  a popover with every message, newest first, each with ✕, and **Clear all**.
- **Jobs**: per-run progress and per-server state (loading / draining /
  generating), with a live preview frame from `jobs/{id}`. Preview is `"proj"`
  by default; `"tae"` once TAEQI2.1 is in the `tae` slot.
- **Job details**: clicking a run in Jobs or in the jobs popover opens a
  modal: its progress, the prompt and negative, the input images as
  thumbnails labelled by their tag (`<image1>`, `<image2>`…, **start image**;
  click one to see it large over the modal), and the settings (created, mode
  and strength, model, size, steps / cfg, sampler, flow shift, alpha, seeds
  per revision, sweep, source idea, cancelled seeds), with the errors of
  failed seeds. **Copy prompt**, **Open in Generate** (the run's settings in
  the form, as Use this style), **Cancel run** while it is in the engine,
  **Close** / Esc.
- **Review** (output queue, top-level tab): a left panel picks the scope
  (all projects / a folder / a project, with unreviewed counts), independent
  of the Projects tab's selection so browsing projects doesn't change the
  queue; "review this project" actions (double-click in Projects → Grid,
  opening an unreviewed image in a folder view) set it. Single view + grid view + A/B compare (`C` or
  the A/B button pins the current image as A and shows the next as B; both can
  be rated, and A/B can be swapped), over a
  checkerboard for alpha images. Double-clicking Single's image opens it
  full size in a modal (click or Esc closes; Review's keys wait meanwhile). The
  modal (the same one every large preview uses) is a fixed box nearly the
  window's size, and the image zooms inside it: wheel or pinch at the
  pointer, `+`/`-`, `0` fit, `1` 100%, and a bar under the image (−, %, +,
  Fit, 100%, ✕); zoomed in, dragging pans and a click no longer closes it.
  Opened from Review (single or grid) or the Output library's grid, `←`/`→`
  (or ‹ › with "3 / 18" in the bar) step through that list, and the review
  cursor or Output selection follows, so closing leaves you on that image.
  Opened from the contact sheet, `↑`/`↓` go to the same seed one style up or
  down (skipping cells with no image yet), and the selected cell follows.
  There (contact sheet included) the image can also be rated and used without
  closing: `1`/`2`/`3` (or `G`/`N`/`B`; `1` then no longer zooms), `Ctrl+Z`,
  `E`/`D`/`R`/`I`, the bar's right end (↶ undo · Good · Neutral · Bad, the
  current verdict marked · ⋯ the use menu), or right-click (the verdicts over
  the use menu). In Review's queue the rated image leaves it and the preview
  goes on to the next one (closing after the last); on the contact sheet it
  steps on; in the Output library it stays. A use that opens another tab
  closes the preview. The bar's ⌨ button opens a popover listing the keys
  that apply there (navigate, rate, use, zoom, close). The keys are listed in a collapsible
  **Keys** section pinned to the bottom of the scope panel: `1`/`2`/`3` (or `G`/`N`/`B`) =
  good/neutral/bad, arrows = next/prev, `Space` = toggle grid/single, `C` =
  compare with pinned, `E` = new edit run from this image, `R` = add to input
  library as ref, `I` = new idea from this image, `Ctrl+Z` = undo last review
  action.
- **Output library**: the place for finished work, as opposed to Projects
  (where images are made and triaged). Tabs **Grid** (default) · **Folder**.
  The full-height left panel has, from the top: the **search** box with
  **in Images | Project names** under it (Images: an image's title or its
  run's prompt; Project names: narrows the "by project" smart folder to the
  projects and folders whose names have the text, a matching folder with all
  it holds, every folder shown open; switching takes the text along), the
  filters (has refs, has alpha, and one checkbox per rating: good, neutral,
  bad, unreviewed), then the **sources**, collections first and smart folders
  after; whichever is picked drives Grid and Folder:
  - **All projects** (the default), first in the collections list as a
    virtual collection of everything, then **All collections**.
  - **Collections**: your own folder tree of kept images, independent of
    projects (e.g. "Portfolio / animals", "Client picks / acme"). An image can
    be in several collections. Fill them by dragging images from Grid onto a
    collection, or with **Add to collection ▸** in the right-click menus of
    the Output library, Review and the folder views (which also offers **New
    collection**). On disk: `collections/<folder>/…/<image-id>.png`, links into
    `output_images/`; removing an image from a collection removes only that
    link, deleting a collection removes its links (asks first), the images
    keep their rating and project.
  - **Smart folders** (virtual, read-only, computed): by date (per day, from
    the image IDs), by source idea, by project, transparent. **By project**
    follows the project tree: folders and subfolders (each a source of its
    own, its projects' kept images together) down to the projects, showing
    only what has kept images; in the side panel a folder holding the
    selected source starts open, and each project expands to its runs with
    kept images, newest first (a sweep's runs by style: `r02 · Claymation`),
    each a source of its own (open while one of them is shown).
  - **Grid**: flat grid of the source's images, under a row of preset tabs
    that each count the source's images: **Kept** (good + neutral, the
    default) · **Good** · **Rejected** (bad) · **Unreviewed** · **All**.
    Clicking a tab replaces the filters (sets its ratings, clears text, refs
    and alpha); once the filters are changed the tab shows "•" and clicking
    it again resets them. **Rejected** follows the source, with **Delete these
    N rejected…** (asks first; the links go to the system trash, the images
    stay in the store until Clean up) and right-click to re-rate, send back to
    Review or **Delete permanently…**. **Unreviewed** has **Review them**
    (Review scoped to the source). Double-clicking an image opens it full
    size over the window (the image preview: click, Esc or a click outside
    closes it; right-click copies it). Selecting an image shows the
    lineage view (source idea → parent ← image → children) with its recipe;
    double-click any of its images for the full-size preview. Right-click: change rating, back to Review, edit run, → input ref, → new
    idea, add to collection, **Use as project cover** (the Projects Grid then
    shows it instead of the best-rated image), **Export…**, show project, copy
    path. **Export these N…** exports everything shown.
  - **Folder**: the file-manager component over the collections tree, with a
    `smart/` folder at the top for the smart folders. Collections behave like
    folders (new, rename, move, copy, delete); images in them can be moved or
    copied between collections, removed (`Del`), opened (Grid + lineage),
    exported, or set as project cover. Its location and history are its own.
    Projects → Folder stays the place for project files (`runs/`,
    `unreviewed/`, triage); the "by project" smart folder is the project
    tree with only the kept images.
  - **Titles**: an output image can be given a title (**Set title…** in the
    Grid's right-click menu, `F2` on the selected image, or the pencil in
    the lineage view). A small dialog edits it: Enter saves, an empty title
    removes it. Titles are stored per project in `project.rsn` (`titles`,
    image ID → title; not copied when a project's setup is copied), show
    in the image's tooltip and lineage view, and are matched by the text
    filter (and the reference picker's).
  - **Show outputs (N kept)** jumps here from a project or project folder:
    the right-click menus of project tiles, the project tree's projects and
    folders, the button at the top left of a hovered tile, the Projects
    Grid's breadcrumb (for the folder in view), and the Projects Folder
    view's right-click menus: on a folder, project or run, or on empty
    space for the folder or project being viewed. It opens Grid on the Kept
    tab for that project or every project under the folder, and selects it
    in the "by project" smart folder: the tree starts over, every folder
    collapsed but the ones on the way to it (that header open too, even if
    you closed it), the row scrolls into view, and a
    project-name search that would hide it is cleared. The other
    filters are kept, but a banner says how many images they hide, with
    **Reset filters**; if the source has no kept images it says so, with
    **Show all N, bad included** (the All tab) and **Review them** when some
    are waiting in Review. The banner
    goes when another source is picked or it is dismissed. The lineage panel
    links back to the image's project. A plain click on an image opens the
    lineage panel, but while it's closed only a moment after the click, so
    a double-click (full size) doesn't open it; while it's open, a click
    shows the clicked image at once.
  - **Export…** (an image, a selection, a collection, a smart folder, or a
    folder in either folder view): copies the PNGs to a chosen folder, with
    or without the embedded metadata, named by run/revision/seed or by image
    ID. The library is unchanged.
- **Icons**: Phosphor (regular) via the `egui-phosphor` crate, registered as
  a fallback font after the text font.

### Timing

Every image is timed as it is made, and the statistics come from those
records, so they cover runs from earlier sessions too (images made before
timing existed have none).

- **Recorded per image** (`run.rsn` `timings`, written with the image):
  `elapsed_s` generating on the server (sd-server's job `started` →
  `completed`), `queued_s` waiting in the server's queue first (`created` →
  `started`), `step_s` seconds per sampling step, timed by the engine from
  the progress reports (first to latest step of the first pass; 0 when the
  job sent no previews), the device, and when it was stored.
- **Per image**: in its tooltips (Review, Output library, contact sheet) and
  the Output library's lineage panel: "generated in 34.2 s · 1.71 s/it ·
  waited 1.0 s · ROCm0".
- **Per run** (job details window, **Timing**): per image (mean, fastest,
  slowest), per step (measured mean, else `≈` generating time ÷ steps, which
  also counts encoding and decoding), GPU time (summed), wall clock (first
  image started → last stored, so runs sharing the GPUs and pauses count),
  server queue wait, and per device. Below it, the project's totals.
- **Live** (each run in Jobs, and "now" in its details): seconds per step of
  the images sampling now (from the GUI's own clock as their steps arrive),
  seconds per image, how many are done and in what time, and roughly the
  time left: the images left (partly sampled ones by their steps) at the
  run's seconds per image, as many at once as are sampling now.

Durations read as "0.84 s", "34.2 s", "4 min 12 s", "2 h 05 min".

### Notifications

**Settings** (⚙ at the far right of the top bar) has a **Notifications** tab with three
independent alerts. The two conditions below each sound when its condition starts holding, then
again every *repeat* minutes while it still holds; once the condition ends it
is reset, and the next time it holds it counts again from the start.

- **GPU idle**: a server that is ready (not starting or loading a model) with
  no job for more than *X* minutes. It applies to **any GPU** or to **all
  GPUs** (every server idle that long). It counts only while servers are
  running; with none started there is nothing to wait for. The engine reports
  `idle` with each server state; the GUI notes when each one became idle.
- **Review backlog**: more than *N* images waiting in Review (the count the
  Review tab shows, over every project).

The third is a moment, not a condition: **GPU queue cleared** sounds once, at
once, when a GPU that had work goes idle (ready, no job in flight and none
left for it in the pool), for **each GPU** or **only when all GPUs are done**
(no server still has work). A server that was idle from the start, or that
dropped out (connecting, unreachable), doesn't count. It doesn't repeat, and
a snoozed or held-back one is not played later; the bell shows it until the
window gets the focus or a GPU gets work again.

Each alert has its own sound: a built-in one (chime, double beep, bell),
synthesized as a WAV in the temp folder at the chosen volume, or a file of
your own. The sound is played by a **player** command, so it works on any OS:

- **automatic** tries, in order, the players usual on the OS and uses the
  first one found: `paplay`, `pw-play`, `aplay` on Linux; `afplay` on macOS;
  PowerShell's `Media.SoundPlayer` on Windows.
- a ready-made command from that list, or **your own command**, split like a
  shell would (quotes, backslashes) but run without a shell. Placeholders:
  `{file}`, `{volume}` (0.00–1.00), `{volume_percent}`, `{volume_pa}`
  (0–65536, PulseAudio's scale). A built-in sound is already at the chosen
  volume, so its `{volume}` is 1.00.

**Test** plays a sound now and says which player played it or why none did.
Options: only when IncreArt isn't the active window (a sound held back for
this, or by a snooze, plays as soon as that ends, if the alert still holds);
also ask the window manager for attention (taskbar flash); also put the reason
in the status bar's messages. The attention request is withdrawn (on X11 the
urgency hint cleared) once the window has the focus or nothing holds any more,
so the taskbar entry doesn't stay lit.

The **bell** in the status bar shows the state: crossed out when every alert
is off (a click opens the settings), ringing while an alert holds, `zz` when
snoozed, a warning when the last sound failed. Its menu says what holds, and
snoozes for 30 min, 1 h or until IncreArt restarts. Snoozing isn't saved;
the settings are, in `config.rsn` (`notifications`, left out while default).

The checks run once a second from the app's `logic` step, which eframe also
calls while the window is minimized or hidden, with a clock of the app's own
(egui's input time stops while the window is hidden).

## Testing

- gui-core reducer: headless unit tests (same pattern as Yue2).
- core client: tests against a mock HTTP server replaying recorded sd-server
  responses (job lifecycle, 503/409/422 paths, cancel-while-generating, job
  TTL expiry) and recorded PE chat responses.
- Scheduler: drain → load → resume, round order, back-off.
- Optional integration tests gated on `INCREART_SDSERVER_URL` and
  `INCREART_PE_URL`.
- Library format: `scripts/make-sample-library.py` writes a deterministic
  on-disk sample library (stores, links, IDs, pixel hashes, RSN/JSON chunks)
  and `scripts/verify-sample-library.py` checks one independently; together
  they are the fixture for `increart-core`'s loaders and writers.
  `increart-core`'s tests parse a freshly generated sample (every RSN file
  must also re-serialize byte for byte), then build a library through the
  API alone and run the verifier on it. Tests turn `fsync` off
  (`fsutil::set_fsync(false)`); on an overlay filesystem it costs about a
  second per write.
- gui-core flows (`crates/increart-gui-core/tests/flows.rs`): the reducer
  with the real runtime and a real temp library (create library, add /
  review / restore / promote ideas, queue a run, trees, drop files, review +
  undo) and the server dialog as a pure reducer.
- The app's screenshot tour (`INCREART_TOUR`, see [gui/README.md](gui/README.md))
  drives every screen through the reducer and saves one PNG per screen.
  `INCREART_TOUR_SCENES=01,02,03,99` runs a subset; scene `99` edits a
  project's prompt and closes the window on the next frame (inside the 2 s
  autosave delay), checking that the draft is flushed on quit: afterwards
  `projects/experiments/glass-fish/draft.rsn` must hold the edit.
- GUI runs that need no human input (screenshots, synthetic input) run on a
  private Xvfb display via `scripts/headless.sh`, never on the desktop. POC1's
  screenshot walkthrough and documentation tour are described in
  [POC1.md](POC1.md#6-testing).

## Measurements so far

HIP build, one R9700, 1024², 20 steps, base int8 model, sd.cpp `fd21ecf`:

- First image after load: 132 s (text encode incl. lazy load 50 s, diffusion
  tensor load 45 s, sampling 77 s incl. a 47 s first step).
- Warm image, same prompt: 34 s (1.56 s/it, conditioning cache hit, decode
  2.5 s).

Through IncreArt's engine (2026-10-24, `examples/smoke.rs`: one server on
ROCm0, 1024², 20 steps, **CFG 6**, recipe flow schedule):

- sd-server up in 14 s; `models/load` returns in 2 s (weights load lazily);
  the 512² 1-step warm-up then took 145 s, absorbing the lazy load as
  intended.
- 65.5–66 s per image (63 s sampling, 2.5 s decode): CFG above 1 runs the
  model twice per step, so CFG 6 costs about double the CFG 1 timing above.
  The resolution editor's time estimate must scale with CFG.
- RGBA prompt (`cute robot mascot sticker`): 65% of pixels fully
  transparent. Plain prompts: RGBA output with alpha 253–255.
- With `preview: none` the job JSON carries no step progress at all, so the
  GUI only sees queued → generating → completed. Per-step progress needs a
  preview mode (`proj` is the cheapest); worth measuring its cost.

Three reference images at once (2026-10-24, `examples/refs.rs`, same
server; refs 1024² each, output 1344×896, CFG 6, prompt "Place the three
subjects side by side in one row, left to right: the red fox from image 1,
… image 2, … image 3 … Plain pastel yellow seamless studio background"):

- Layout, order and background followed in both seeds with a plain
  template prompt; subjects recognisably taken from their refs.
- 237–289 s per image vs 66 s without refs: each 1024² ref adds ~4096
  tokens to every attention call. Downscaling refs (e.g. to 512²) is the
  obvious lever; to be measured against fidelity.
- Quality problems: both images washed out / over-bright with a glow; in
  seed 0 the fox's fur turned flat green with purple blotches. Purple is the
  hidden colour under the transparent area of the RGBA robot ref, so alpha
  refs may leak; CFG 6 may also be too high with refs.
- Same refs and seeds at **CFG 2** with a prompt that first says what each
  ref is ("Image 1 shows a red fox, image 2 …, image 3 …. Create a new
  photograph of these three together in a row on a pastel yellow seamless
  studio backdrop: … Keep each subject's look from its reference image.
  Natural colours …"): clean, natural colour, no blotches, all three
  subjects faithful to their refs. CFG 6 was the main cause of the washed-out
  look (prompt changed at the same time, so not fully separated).
- The same prompt written with `<image1>`/`<image2>`/`<image3>` tags (CFG 2,
  same seeds): near-identical images, same layout and poses per seed;
  seed 1's robot slightly crisper. With distinct subjects and an explicit
  order, the tags matter little; they remain the documented convention.
- Time per image unchanged by CFG (236–257 s): the cost is the three 1024²
  refs. Default CFG for runs with refs: 2 (base model), pending a wider
  A/B.

## Open questions

- EXIF from imported photos is kept verbatim, including GPS location if the
  camera recorded it. Strip GPS (or all EXIF) on import, keep as is, or make
  it a setting?
- `inbox_drop/imported/` keeps every imported original forever unless you
  clear it. Auto-delete after some days, or leave it to the user?

- A Settings screen: the resolution editor is only reachable from the
  Generate panel for now. A general Settings place (servers, PE, library
  root, resolutions) would be the natural second entry point once there is
  more to put in it.

- New idea form: which add button is the primary one (Add to inbox, Add &
  keep, Add & promote)? POC1 shows all three with equal weight.
- IDs use a separate counter per kind, so the same string can be an idea, a
  run and an image. Keep that, or use one counter across kinds so every ID is
  unique in the library?

- Ideas from outside: is a watched drop folder enough, or is a small HTTP
  endpoint wanted (e.g. a phone share target or browser bookmarklet)?
- Captioning image-only ideas: the i2i PE model or a Qwen3-VL chat server could
  describe an image to seed its text. Is that worth adding to the PE service?
- Base-model CFG: 1.0 (recipe default) vs ~6.0 (sd.cpp example). A/B on a fixed
  prompt set before choosing the preset.
- GGUF availability (and llama.cpp support) for the Qwen3.5-9B PE models.
- Upstream appetite for a disk-backed conditioning cache vs. carrying a local
  patch.

## To benchmark

- HIP vs Vulkan speed/memory on gfx1201 (both builds exist).
- Splitting one model across both GPUs (`diffusion=ROCm0&ROCm1`) vs one server
  per GPU: per-GPU is expected to win on throughput for batches.
- 2048² time per image, to decide whether it stays a preset or becomes a
  "final render" option.
- Conditioning cache size vs. hit rate with interleaved runs.
- TAEQI2.1 preview cost vs `proj`.
