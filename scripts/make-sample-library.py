#!/usr/bin/env python3
"""Write a small synthetic IncreArt library following docs/DESIGN.md (Filesystem
layout, Project folders, Input library folders), for poking at the format and
testing loaders before the real app exists.

    scripts/make-sample-library.py [out_dir]     # default: tmp/sample-library

Content mirrors the GUI prototype's mock data. Images are procedural 1024x1024
placeholders; model file hashes are sha256 of the file id, not of real weights.
Output is deterministic (fixed RNG seed). Refuses to write into a non-empty dir.
"""
import hashlib
import json
import math
import os
import random
import struct
import sys
import zlib
from datetime import datetime, timedelta, timezone

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
OUT = os.path.abspath(sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "tmp", "sample-library"))
SIZE = 1024
SDCPP_COMMIT = "fd21ecf"
rng = random.Random(42)
if os.path.isdir(OUT) and os.listdir(OUT):
    sys.exit(f"{OUT} is not empty; remove it or pass another dir")
T0 = datetime(2026, 10, 18, 9, 0, tzinfo=timezone.utc)

# ------------------------------------------------------------------ helpers

counters = {}  # (kind, minute) -> next number


def new_id(kind, t):
    """`YYYY.MM.DD-HH.MM-NNNN` in UTC, numbered from 0000 per kind within the
    minute (docs/DESIGN.md, IDs)."""
    minute = t.strftime("%Y.%m.%d-%H.%M")
    n = counters.get((kind, minute), 0)
    counters[(kind, minute)] = n + 1
    return f"{minute}-{n:04d}"


def ts(t):
    return t.strftime("%Y-%m-%dT%H:%M:%SZ")


def rsn_str(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n") + '"'


def rsn(v, ind=0):
    """Tiny RSN writer in the style increart-core writes (4-space indent, no
    trailing commas, `Option` values bare): tuple ("Name", dict|None) -> named
    struct or enum variant, Map -> map, list -> list, str -> string, None -> None."""
    pad, pad1 = "    " * ind, "    " * (ind + 1)
    if v is None:
        return "None"
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, int):
        return str(v)
    if isinstance(v, float):
        return repr(v).removesuffix(".0")  # like Rust's Display for floats
    if isinstance(v, str):
        return rsn_str(v)
    if isinstance(v, Some):
        return rsn(v.v, ind)
    if isinstance(v, Map):
        if not v.d:
            return "{}"
        return "{\n" + ",\n".join(f"{pad1}{rsn_str(k)}: {rsn(x, ind + 1)}" for k, x in v.d.items()) + "\n" + pad + "}"
    if isinstance(v, tuple):
        name, body = v
        if body is None:
            return name
        return name + " {\n" + ",\n".join(f"{pad1}{k}: {rsn(x, ind + 1)}" for k, x in body.items()) + "\n" + pad + "}"
    if isinstance(v, list):
        if not v:
            return "[]"
        return "[\n" + ",\n".join(f"{pad1}{rsn(x, ind + 1)}" for x in v) + "\n" + pad + "]"
    raise TypeError(type(v))


class Some:
    def __init__(self, v):
        self.v = v


class Map:
    def __init__(self, d):
        self.d = d


def write(path, data):
    path = os.path.join(OUT, path)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb" if isinstance(data, bytes) else "w") as f:
        f.write(data)


def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)


def picture(seed, alpha):
    """Two-colour gradient with a soft blob; with alpha, the blob on a transparent
    background. Rows use PNG filter 1 (Sub), which keeps smooth images small."""
    r = random.Random(seed)
    hue = r.random()
    c1 = [int(255 * (0.5 + 0.5 * math.cos(6.283 * (hue + k / 3)))) for k in range(3)]
    c2 = [int(255 * (0.5 + 0.5 * math.cos(6.283 * (hue + 0.4 + k / 3)))) for k in range(3)]
    blob = [int(255 * (0.5 + 0.5 * math.cos(6.283 * (hue + 0.7 + k / 3)))) for k in range(3)]
    cx, cy, rad = SIZE * (0.3 + 0.4 * r.random()), SIZE * (0.3 + 0.4 * r.random()), SIZE * (0.15 + 0.15 * r.random())
    ch = 4 if alpha else 3
    raw, pixels = bytearray(), bytearray()
    for y in range(SIZE):
        t = y / (SIZE - 1)
        base = [int(c1[k] + (c2[k] - c1[k]) * t) for k in range(3)]
        dy2 = (y - cy) ** 2
        row = bytearray()
        for x in range(SIZE):
            w = max(0.0, 1.0 - math.sqrt((x - cx) ** 2 + dy2) / rad)
            w = w * w * (3 - 2 * w)
            if alpha:
                row += bytes((blob[0], blob[1], blob[2], int(255 * w)))
            else:
                row += bytes(int(base[k] + (blob[k] - base[k]) * w) for k in range(3))
        sub = bytearray(row[:ch]) + bytearray((row[i] - row[i - ch]) & 0xFF for i in range(ch, len(row)))
        raw += b"\x01" + sub
        pixels += row
    return ch, bytes(raw), bytes(pixels)


def pixel_sha256(width, height, ch, pixels):
    """Content fingerprint per docs/DESIGN.md (Pixel hash): canonical encoding v1."""
    return sha(b"increart-pixels-v1" + struct.pack(">IIB", width, height, ch) + pixels)


def png(seed, alpha, text_chunks=(), image_id=None):
    """Returns (file bytes, pixel_sha256). Store files (with an `image_id`) also
    get `increart:id` and `increart:pixels`."""
    ch, raw, pixels = picture(seed, alpha)
    h = pixel_sha256(SIZE, SIZE, ch, pixels)
    out = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", SIZE, SIZE, 8, 6 if ch == 4 else 2, 0, 0, 0))
    if image_id:
        text_chunks = [("tEXt", "increart:id", image_id), ("tEXt", "increart:pixels", h), *text_chunks]
    for kind, key, text in text_chunks:
        if kind == "iTXt":
            out += chunk(b"iTXt", key.encode() + b"\x00\x00\x00\x00\x00" + text.encode())
        else:
            out += chunk(b"tEXt", key.encode("latin-1") + b"\x00" + text.encode("latin-1", "replace"))
    return out + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""), h


def sha(data):
    return hashlib.sha256(data).hexdigest()


def link(path, store, image_id):
    """Relative symlink at `path` (library-relative) to `<store>_images/<image_id>.png`."""
    at = os.path.join(OUT, path)
    os.makedirs(os.path.dirname(at), exist_ok=True)
    os.symlink(os.path.relpath(os.path.join(OUT, f"{store}_images/{image_id}.png"), os.path.dirname(at)), at)


def image_ref(store, image_id):
    return ("ImageRef", {"id": image_id, "store": (store.capitalize(), None)})


# ------------------------------------------------------------------ models + config

MODEL_FILES = {
    "diffusion_model": "diffusion_models/qwen_image_2.1_int8_convrot.safetensors",
    "llm": "text_encoders/qwen3vl_8b_int8_convrot.safetensors",
    "vae": "vae/qwen_image_2.1_vae_bf16.safetensors",
}
MODEL = ("Model", {
    "recipe": "qwen-image-21",
    "files": Map({slot: ("ModelFile", {"id": fid, "sha256": sha(fid.encode())}) for slot, fid in MODEL_FILES.items()}),
})

write("library.rsn", "// Library format and link kind (see docs/DESIGN.md, Links).\n" + rsn(("Library", {"format": 1, "link_mode": ("Symlink", None)})) + "\n")

write("config/config.rsn", "// Sample of ~/.config/increart/config.rsn (see docs/DESIGN.md, Config file).\n" + rsn(("Config", {
    "resolutions": [
        ("Resolution", {"name": "Square", "width": 1024, "height": 1024, "default": True}),
        ("Resolution", {"name": "Square L", "width": 1344, "height": 1344}),
        ("Resolution", {"name": "Square XL", "width": 2048, "height": 2048}),
        ("Resolution", {"name": "Portrait 2:3", "width": 896, "height": 1344}),
        ("Resolution", {"name": "Landscape 3:2", "width": 1344, "height": 896}),
        ("Resolution", {"name": "Wide 16:9", "width": 1664, "height": 928}),
    ],
})) + "\n")

# ------------------------------------------------------------------ input side

images = {}  # name -> (pixel_sha256, original file name)


def input_image(name, seed, original, alpha=False):
    """Imports a synthetic original: `original` is (file name or None, path or None,
    format, conversions). Returns the original bytes as dropped (a PNG here)."""
    fname, path, fmt, conversions = original
    dropped, h = png(seed, alpha)  # the pixel hash ignores chunks
    source = {
        "version": 1,
        "original_name": fname,
        "original_path": path,
        "original_format": fmt,
        "original_size": 150_000 + rng.randrange(3_000_000),
        "original_sha256": sha(f"synthetic original {name}".encode()),
        "original_mtime": ts(T0 - timedelta(days=rng.randrange(1, 300))),
        "original_width": SIZE,
        "original_height": SIZE,
        "imported": None,
        "conversions": conversions,
    }
    imports[name] = source
    images[name] = (h, fname or "clipboard.png")
    return dropped


imports = {}
dl, pics, phone = "/home/user/Downloads", "/home/user/Pictures", "/mnt/storage/sdcpp/increart_lib/input/inbox_drop"
ORIGINALS = {
    "lighthouse": (2, (None, None, "png", [])),
    "untitled-a": (5, ("Screenshot_20261020-101500.png", phone, "png", [])),
    "untitled-b": (6, ("Screenshot_20261020-101512.png", phone, "png", [])),
    "robot": (4, ("robot-mascot.webp", dl, "webp", ["webp -> png"])),
    "glaze": (9, ("glaze.jpg", dl, "jpeg", ["jpeg -> png"])),
    "hut": (11, ("hut.jpg", pics, "jpeg", ["exif-orientation: rotate 90", "jpeg -> png"])),
    "fish-a": (12, ("fish_01.jpg", pics, "jpeg", ["icc: Display P3 -> sRGB", "jpeg -> png"])),
    "fish-b": (13, ("fish_02.jpg", pics, "jpeg", ["icc: Display P3 -> sRGB", "jpeg -> png"])),
    "papercut": (14, ("papercut.png", pics, "png", [])),
    "rejected": (21, ("beach.jpg", dl, "jpeg", ["jpeg -> png"])),
}
dropped = {}
for name, (seed, orig) in ORIGINALS.items():
    dropped[name] = input_image(name, seed, orig, alpha=(name == "robot"))


input_ids = {}  # pixel_sha256 -> input image ID (the pixel index)


def store_input(name, imported):
    """Imports into the input store at `imported` (the idea's creation time), reusing
    an existing image with the same pixels. Returns the image ID."""
    h = images[name][0]
    if h in input_ids:
        return input_ids[h]
    seed, _ = ORIGINALS[name]
    src = {**imports[name], "imported": ts(imported)}
    if src["original_path"]:
        src["original_path"] = f"{src['original_path']}/{src['original_name']}"
    image_id = input_ids[h] = new_id("input", imported)
    data, _ = png(seed, name == "robot", [("iTXt", "increart:source", json.dumps(src, indent=2))], image_id)
    write(f"input_images/{image_id}.png", data)
    return image_id


ideas = {}  # key -> [id, dir, idea.rsn body]
PE_HASH = sha(b"sample PE system prompt (t2i)")


def idea(key, where, t, title, text, tags=(), imgs=(), source=("Manual", None), starred=False, variants=()):
    iid = new_id("idea", t)
    d = f"input/{where}/{iid}" if where else f"input/{iid}"
    image_ids = [store_input(i, t) for i in imgs]
    body = {
        "id": iid,
        "title": title,
        "text": text,
        "variants": [("PromptVariant", {"text": v, "origin": ("Rewritten", {"model": "qwen3.5-9b-qwen-image-2.1-pe-t2i", "system_prompt_hash": PE_HASH})}) for v in variants],
        "tags": list(tags),
        "source": source,
        "created": ts(t),
        "images": image_ids,
        "projects": [],
        "starred": starred,
    }
    ideas[key] = [iid, d, body]
    for n, (i, image_id) in enumerate(zip(imgs, image_ids), 1):
        link(f"{d}/{n}-{os.path.splitext(images[i][1])[0]}.png", "input", image_id)


day = timedelta(days=1)
idea("fox", "library", T0, "Fox in café", "a red fox reading a newspaper in a sunlit cafe, film photograph", ["animals", "photo"], starred=True)
idea("lighthouse", "unreviewed", T0 + 2 * day, "Moody lighthouse", "lighthouse on a cliff at dusk, storm rolling in, painterly", ["landscape"], ["lighthouse"], ("Clipboard", None))
idea("untitled", "unreviewed", T0 + 2 * day + timedelta(hours=3), "", "", [], ["untitled-a", "untitled-b"], ("InboxFolder", None))
idea("robot", "library", T0 + timedelta(hours=5), "Sticker set", "cute robot mascot stickers, flat vector, transparent background", ["alpha", "vector"], ["robot"], starred=True)
idea("glaze", "unreviewed", T0 + 3 * day, "Ceramic texture", "use this glaze texture as a style reference", ["style"], ["glaze"],
     ("DroppedFile", {"original_path": "/home/user/Downloads/glaze.jpg"}))
idea("jazz", "unreviewed", T0 + 3 * day + timedelta(hours=1), "Poster: jazz night", "1960s jazz club poster with bold type 'BLUE NOTE NIGHTS'", ["poster", "text"])
idea("hut", "library/places", T0 + timedelta(hours=1), "Kept: alpine hut", "wooden hut in alpine meadow, morning fog", ["landscape"], ["hut"], starred=True)
idea("fish", "library", T0 + timedelta(hours=2), "Kept: glass fish", "translucent glass fish sculpture, studio lighting", ["product"], ["fish-a", "fish-b"])
idea("alley", "library/places", T0 + timedelta(hours=3), "Kept: neon alley", "rainy neon alley, cinematic", ["city"], starred=True,
     variants=["A narrow alley at night in heavy rain, magenta and cyan neon signs reflected in puddles, shallow depth of field, anamorphic lens flare, cinematic colour grade."])
idea("papercut", "library/styles/paper", T0 + timedelta(hours=4), "Ref: paper cut style", "", ["style"], ["papercut"], ("DroppedFile", {"original_path": "/home/user/Pictures/papercut.png"}))
idea("rejected", "discarded", T0 + timedelta(hours=6), "Blurry beach", "beach at noon, harsh light", ["landscape"], ["rejected"])

# Dropped from outside the app: two files not yet picked up by the watcher, and
# the originals of the InboxFolder idea, moved aside after import.
write("input/inbox_drop/from-phone.txt", "otter floating on its back holding a teacup, watercolour\n")
write("input/inbox_drop/IMG_2041.png", png(31, False)[0])
for name in ["untitled-a", "untitled-b"]:
    write(f"input/inbox_drop/imported/{ORIGINALS[name][1][0]}", dropped[name])

# ------------------------------------------------------------------ projects

SAMPLE = {"method": "euler", "scheduler": "simple", "steps": 20, "cfg": 6.0, "flow_shift": 3.0}
RGBA_WRAP = "This is an RGBA image with transparency. {}. The image has alpha channel and the background is transparent."
projects = {}


def project(key, path, t, sources):
    pid = new_id("project", t)
    projects[key] = (pid, path)
    for s in sources:
        ideas[s][2]["projects"].append(pid)
    write(f"projects/{path}/project.rsn", rsn(("Project", {"id": pid, "created": ts(t), "sources": [ideas[s][0] for s in sources]})) + "\n")


def ref(name):
    return image_ref("input", input_ids[images[name][0]])


def run(proj, t, label, prompt, seeds, verdicts, alpha=False, mode=("Generate", None), refs=(), parent=None, source=None, sample=SAMPLE, first=0, sweep=None):
    """Writes run.rsn and one PNG per finished seed. `verdicts[i]` is the review
    folder for seed i ("good"/"neutral"/"bad"), None = unreviewed. `sweep` is
    (sweep label, task, preset) for one style of a style sweep."""
    pid, path = projects[proj]
    rid = new_id("run", t)
    sent = RGBA_WRAP.format(prompt.rstrip(".")) if alpha else prompt
    # each finished seed's timing (run.rsn `timings`, DESIGN.md Timing):
    # generating ~31-37 s, finished ~35 s apart on two GPUs
    elapsed = [round(31 + rng.random() * 6, 1) for _ in verdicts]
    timings = [("SeedTiming", {
        "rev": 0,
        "seed": first + i,
        "device": ["ROCm0", "ROCm1"][i % 2],
        "done": ts(t + timedelta(seconds=35 * (i + 1))),
        "elapsed_s": e,
        "queued_s": 1.0,
        "step_s": round((e - 2.5) / sample["steps"], 2),
    }) for i, e in enumerate(elapsed)]
    write(f"projects/{path}/runs/{rid}/run.rsn", rsn(("Run", {
        "id": rid,
        "label": label,
        "created": ts(t),
        "prompt": prompt,
        "negative_prompt": "",
        "alpha": alpha,
        "width": SIZE,
        "height": SIZE,
        "sample": ("Sample", sample),
        "mode": mode,
        "refs": list(refs),
        "model": MODEL,
        "source_idea": Some(ideas[source][0]) if source else None,
        "revisions": [("Revision", {"rev": 0, "created": ts(t), "seeds": ("Seeds", {"first": first, "count": seeds})})],
        **({"sweep": ("Sweep", {"name": sweep[0], "task": sweep[1], "preset": sweep[2]})} if sweep else {}),
        **({"timings": timings} if timings else {}),
    })) + "\n")
    out = []
    for i, verdict in enumerate(verdicts):
        seed = first + i
        recipe = {
            "prompt_original": prompt,
            "prompt_sent": sent,
            "negative_prompt": "",
            "width": SIZE,
            "height": SIZE,
            "seed": seed,
            "alpha": alpha,
            "sample": ("Sample", sample),
            "mode": mode,
            "refs": list(refs),
            "parent": Some(parent) if parent else None,
            "source_idea": Some(ideas[source][0]) if source else None,
            "rewrite": None,
            "model": MODEL,
            "sdcpp": SDCPP_COMMIT,
            "increart": "0.0.0-sample",
            "device": ["ROCm0", "ROCm1"][i % 2],
            "elapsed_s": elapsed[i],
            "queued_s": 1.0,
            "step_s": timings[i][1]["step_s"],
        }
        params = f"{sent}\nSteps: {sample['steps']}, Sampler: {sample['method']}, CFG scale: {sample['cfg']}, Seed: {seed}, Size: {SIZE}x{SIZE}"
        oid = new_id("output", t + timedelta(seconds=35 * (i + 1)))  # finished ~35 s apart
        data, _ = png(int(sha(f"{rid}-{seed}".encode())[:8], 16), alpha, [("iTXt", "increart:recipe", rsn(("Recipe", recipe))), ("tEXt", "parameters", params)], oid)
        folder = f"reviewed/{verdict}" if verdict else "unreviewed"
        write(f"output_images/{oid}.png", data)
        link(f"projects/{path}/{folder}/{rid}_r0_s{seed:04d}.png", "output", oid)
        out.append((oid, folder))
    return out


G, N, B = "good", "neutral", "bad"
project("fox", "clients/acme/fox-cafe", T0 + day, ["fox"])
r01 = run("fox", T0 + day, "r01", ideas["fox"][2]["text"], 8, [G, N, B, G, N, B, N, B], source="fox",
          sample={**SAMPLE, "cfg": 1.0})
run("fox", T0 + day + timedelta(hours=4), "r02 cfg6", ideas["fox"][2]["text"], 32, [None] * 6, source="fox")

project("fish", "experiments/glass-fish", T0 + 2 * day, ["fish"])
fish_init = ref("fish-a")
run("fish", T0 + 2 * day, "r01 edit", "make the fish amber glass", 16, [G, None, None, None],
    mode=("Edit", {"init": Some(fish_init), "strength": 0.75}), refs=[ref("fish-b")],
    parent=fish_init, source="fish")

project("stickers", "stickers", T0 + 2 * day + timedelta(hours=6), ["robot"])
run("stickers", T0 + 2 * day + timedelta(hours=6), "r01 rgba", "cute robot mascot sticker", 6, [G, G, N, N, None, None],
    alpha=True, refs=[ref("robot")], source="robot")

# A style sweep: one run per style, the same three seeds; the last style's
# third image is still to come.
for i, (style, words, verdicts) in enumerate([
    ("Claymation", "claymation, soft plasticine with fingerprints", [G, N, None]),
    ("Pixel art", "16-bit pixel art, crisp pixels, limited palette", [B, B, None]),
    ("Watercolour", "loose watercolour, soft washes, paper texture", [None, None, None]),
    ("Ukiyo-e woodblock", "ukiyo-e woodblock print, flat colour, bold outlines", [None, None]),
]):
    run("stickers", T0 + 3 * day + timedelta(minutes=i), f"r02 · {style}", f"cute robot mascot sticker, {words}", 3, verdicts,
        sweep=("r02", "Modify style", style))

# An idea made from an output (I in review), still in the Idea inbox.
good = next(h for h, folder in r01 if folder == "reviewed/good")
idea("from-fox", "unreviewed", T0 + 3 * day + timedelta(hours=2), "from fox-cafe", ideas["fox"][2]["text"], [], [],
     ("FromOutput", {"image": image_ref("output", good)}))

# ------------------------------------------------------------------ write ideas, empty dirs, README

for iid, d, body in ideas.values():
    write(f"{d}/idea.rsn", rsn(("Idea", body)) + "\n")
for d in ["thumbs", "cache/conditioning"]:
    os.makedirs(os.path.join(OUT, d), exist_ok=True)

write("README.md", """# Sample IncreArt library

Synthetic data in the layout of `docs/DESIGN.md` (Filesystem layout, Project
folders, Input library folders), generated by `scripts/make-sample-library.py`.
Mirrors the GUI prototype's mock data.

- `config/config.rsn` stands in for `~/.config/increart/config.rsn`;
  `library.rsn` holds the format version and link mode (symlinks here).
- IDs (ideas, projects, runs, images) are `YYYY.MM.DD-HH.MM-NNNN`: UTC
  creation minute plus a counter from 0000, separate per kind.
- `input_images/` and `output_images/` are the image stores: every image, as
  PNG, named by its ID. Every other image file in the library is a relative
  symlink into them.
- `input/unreviewed/` is the Idea inbox, `input/library/` the input library
  (a folder tree), `input/discarded/` discarded ideas. Each idea folder has
  `idea.rsn` plus links to its images.
- `input/inbox_drop/` has two files not yet picked up; `imported/` holds
  originals already imported.
- `projects/` is the project folder tree; a directory with `project.rsn` is a
  project. Its `unreviewed/` and `reviewed/*/` hold links.
- Store PNGs carry `increart:id` (their ID) and `increart:pixels` (hash of
  the decoded pixels, see Pixel hash in the design doc). Inputs also carry
  `increart:source` (JSON: the original file's name, path, format, size,
  hash, conversions); outputs carry `increart:recipe` (RSN) plus an
  sd-server-style `parameters` chunk.
- `thumbs/` and `cache/conditioning/` are empty caches.

Not real data: images are procedural placeholders; the "originals" in
`increart:source` (names, sizes, hashes, conversions) are invented, and none
carries EXIF; model `sha256` values are the hash of the file id; timings are
made up.
""")
print(OUT)
