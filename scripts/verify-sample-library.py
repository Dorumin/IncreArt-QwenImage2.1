#!/usr/bin/env python3
"""Independent check of a sample library written by make-sample-library.py:
store files (name = increart:id, increart:pixels = real pixel hash, recipe or
source chunk present), every image link resolves into a store, every
ImageRef and idea image resolves, and idea/project/run IDs are well formed and
unique. Decodes PNGs itself (all five row filters), independent of the generator.

    scripts/verify-sample-library.py [library_dir]   # default: tmp/sample-library
"""
import glob, hashlib, json, os, re, struct, sys, zlib
os.chdir(sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "tmp", "sample-library"))

def chunks(d):
    i = 8
    while i < len(d):
        n, = struct.unpack(">I", d[i:i+4]); yield d[i+4:i+8], d[i+8:i+8+n]; i += 12 + n

def text(d, key):
    for k, body in chunks(d):
        if k == b"tEXt" and body.split(b"\0", 1)[0] == key.encode(): return body.split(b"\0", 1)[1].decode("latin-1")
        if k == b"iTXt" and body.split(b"\0", 1)[0] == key.encode(): return body.split(b"\0", 5)[-1].decode()

def phash(d):
    idat = b""
    for k, body in chunks(d):
        if k == b"IHDR": w, h, depth, ctype = struct.unpack(">IIBB", body[:10])
        if k == b"IDAT": idat += body
    ch = {2: 3, 6: 4}[ctype]; raw = zlib.decompress(idat); stride = w * ch
    out, prev, pos = bytearray(), bytearray(stride), 0
    for _ in range(h):
        f = raw[pos]; line = bytearray(raw[pos+1:pos+1+stride]); pos += 1 + stride
        for x in range(stride):
            a = line[x-ch] if x >= ch else 0; b = prev[x]; c = prev[x-ch] if x >= ch else 0
            if f == 1: line[x] = (line[x] + a) & 255
            elif f == 2: line[x] = (line[x] + b) & 255
            elif f == 3: line[x] = (line[x] + (a + b) // 2) & 255
            elif f == 4:
                p = a + b - c; pa, pb, pc = abs(p-a), abs(p-b), abs(p-c)
                line[x] = (line[x] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        out += line; prev = line
    return hashlib.sha256(b"increart-pixels-v1" + struct.pack(">IIB", w, h, ch) + bytes(out)).hexdigest()

problems = []
ID = re.compile(r"\d{4}\.\d{2}\.\d{2}-\d{2}\.\d{2}-\d{4,}")
store = {}
for p in glob.glob("*_images/*.png"):
    d = open(p, "rb").read(); h = phash(d); store[p] = d
    if text(d, "increart:id") != os.path.basename(p)[:-4]: problems.append(f"increart:id != name: {p}")
    if text(d, "increart:pixels") != h: problems.append(f"increart:pixels != pixel hash: {p}")
    if not ID.fullmatch(os.path.basename(p)[:-4]): problems.append(f"bad id format: {p}")
    if p.startswith("input_images/"):
        src = json.loads(text(d, "increart:source"))
        if src.get("version") != 1 or "original_format" not in src: problems.append(f"bad source json: {p}")
    elif text(d, "increart:recipe") is None: problems.append(f"output without recipe: {p}")
links = [p for p in glob.glob("**/*.png", recursive=True) if os.path.islink(p)]
for p in links:
    t = os.path.relpath(os.path.realpath(p))
    if os.path.isabs(os.readlink(p)): problems.append(f"absolute link: {p}")
    if t not in store: problems.append(f"link not into a store: {p} -> {t}")
plain = [p for p in glob.glob("**/*.png", recursive=True) if not os.path.islink(p) and not p.startswith(("input_images/", "output_images/", "input/inbox_drop/"))]
if plain: problems.append(f"image files outside stores that aren't links: {plain}")
blob = "".join(open(f).read() for f in glob.glob("**/*.rsn", recursive=True)) + "".join(text(d, "increart:recipe") or "" for d in store.values())
refs = re.findall(r'id: "([0-9.\-]+)",\s*store: (Input|Output)', blob)
for h, s in refs:
    if f"{s.lower()}_images/{h}.png" not in store: problems.append(f"dangling ref {s} {h}")
for f in glob.glob("input/*/**/idea.rsn", recursive=True):
    block = re.search(r'^    images: (\[\]|\[\n.*?^    \]),$', open(f).read(), re.M | re.S).group(1)
    for h in re.findall(r'"([^"]+)"', block):
        if f"input_images/{h}.png" not in store: problems.append(f"idea image missing: {f} {h}")
ids = {"idea": [os.path.basename(os.path.dirname(f)) for f in glob.glob("input/**/idea.rsn", recursive=True)],
       "project": re.findall(r'^    id: "(.*)",$', "".join(open(f).read() for f in glob.glob("projects/**/project.rsn", recursive=True)), re.M),
       "run": [os.path.basename(os.path.dirname(f)) for f in glob.glob("projects/**/run.rsn", recursive=True)]}
for kind, xs in ids.items():
    if len(set(xs)) != len(xs) or not all(ID.fullmatch(x) for x in xs): problems.append(f"{kind} ids bad or duplicated: {xs}")
for f in glob.glob("input/**/idea.rsn", recursive=True):
    if re.search(r'^    id: "(.*)",$', open(f).read(), re.M).group(1) != os.path.basename(os.path.dirname(f)): problems.append(f"idea dir != id: {f}")
print(f"{sum(map(len, ids.values()))} idea/project/run ids; {len(store)} store images, {len(links)} links, {len(refs)} refs; problems: {problems or 'none'}")
