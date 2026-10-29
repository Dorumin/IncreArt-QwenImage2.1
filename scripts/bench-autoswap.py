#!/usr/bin/env python3
"""KoboldCpp autoswap on one card (docs/vram-tiers.md): the chat model and
the image model in one server, only one resident at a time. Times each
image and PE ask, swaps included, and the card's VRAM peak in each.

Usage: scripts/bench-autoswap.py ID CARD OUT_DIR -- KOBOLDCPP_ARGS...
(the model, image and device flags, or a --config template; this adds
the port, and admin, router and autoswap unless a template is given)."""
import base64, json, os, shutil, signal, subprocess, sys, tempfile, threading, time, urllib.request

KOBOLDCPP = "/workspace/local/koboldcpp/koboldcpp"
REF = "/mnt/storage/increart_library/input_images/2026.10.24-22.01-0000.png"
PROMPT = ("Turn this into a winter scene: the same three mime women, same poses and outfits, "
          "now wearing red scarves and standing in falling snow in a quiet Paris street at dusk, warm lamp light.")
REWRITE = ("You rewrite image-editing prompts. Given the user's prompt and the reference image, "
           "write one detailed, concrete prompt for an image model. Answer with the prompt only.")

ident, card, out = sys.argv[1], int(sys.argv[2]), sys.argv[3]
extra = sys.argv[sys.argv.index("--") + 1:]
port = 5480 + card
used = f"/sys/class/drm/card{card}/device/mem_info_vram_used"
gib = lambda b: round(b / 2**30, 2)

def ref_png(size):
    tmp = tempfile.mktemp(suffix=".png")
    subprocess.run(["magick", REF, "-resize", f"{size}x{size}!", tmp], check=True)
    data = open(tmp, "rb").read()
    os.remove(tmp)
    return base64.b64encode(data).decode()

ref1024, ref768 = ref_png(1024), ref_png(768)
base = int(open(used).read())
peak = [0]
stop = False

def sample():
    while not stop:
        peak[0] = max(peak[0], int(open(used).read()))
        time.sleep(0.2)

def post(path, body, timeout=3600):
    req = urllib.request.Request(f"http://127.0.0.1:{port}{path}", json.dumps(body).encode(),
                                 {"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return json.load(r)

def image(seed):
    r = post("/sdapi/v1/txt2img", {"prompt": PROMPT, "width": 1024, "height": 1024, "seed": seed, "steps": 20,
                                   "cfg_scale": 2.0, "sampler_name": "euler", "extra_images": [ref1024],
                                   "add_sd_step_limit": 80, "add_sd_cfg_limit": 25, "remove_limits": 1})
    if not r.get("images") or not r["images"][0]:
        raise RuntimeError("no image")
    open(f"{out}/{ident}-{seed}.png", "wb").write(base64.b64decode(r["images"][0]))

def ask(_):
    r = post("/v1/chat/completions", {"max_tokens": 700, "temperature": 0.7, "messages": [
        {"role": "system", "content": REWRITE},
        {"role": "user", "content": [{"type": "text", "text": PROMPT},
                                     {"type": "image_url", "image_url": {"url": "data:image/png;base64," + ref768}}]}]})
    return r["usage"]["completion_tokens"]

admin = tempfile.mkdtemp(prefix="kcpp-admin-")
log = open(f"{out}/{ident}.log", "w")
cmd = [KOBOLDCPP, "--port", str(port), "--skiplauncher", "--quiet"]
# a template sets its own admin, router and autoswap modes
if "--config" not in extra:
    cmd += ["--admin", "--admindir", admin, "--routermode", "--autoswapmode"]
cmd += extra
proc = subprocess.Popen(cmd, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
threading.Thread(target=sample, daemon=True).start()
steps, result = [], {"id": ident, "card": card, "args": extra}
try:
    t = time.time()
    while True:
        try:
            urllib.request.urlopen(f"http://127.0.0.1:{port}/api/v1/model", timeout=5)
            break
        except Exception:
            if proc.poll() is not None or time.time() - t > 900:
                raise RuntimeError("KoboldCpp didn't start")
            time.sleep(1)
    result["launch_s"] = round(time.time() - t, 1)
    # the order the app asks in: enhance, generate, enhance again, generate twice
    for name, fn, arg in [("ask", ask, 0), ("image", image, 1), ("ask", ask, 0), ("ask", ask, 0),
                          ("image", image, 2), ("image", image, 3)]:
        peak[0] = 0
        t = time.time()
        extra_info = fn(arg)
        step = {"step": name, "s": round(time.time() - t, 1), "peak_rise_gib": gib(peak[0] - base)}
        if name == "ask":
            step["tokens"] = extra_info
        print(json.dumps(step), flush=True)
        steps.append(step)
except Exception as e:
    result["error"] = str(e)
finally:
    stop = True
    result["steps"] = steps
    os.killpg(proc.pid, signal.SIGTERM)
    try:
        proc.wait(30)
    except subprocess.TimeoutExpired:
        os.killpg(proc.pid, signal.SIGKILL)
    shutil.rmtree(admin, ignore_errors=True)
    json.dump(result, open(f"{out}/{ident}.json", "w"), indent=2)
