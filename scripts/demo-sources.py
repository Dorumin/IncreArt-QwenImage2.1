#!/usr/bin/env python3
"""Downloads the public-domain source images of the demo library, with a
credits file saying where each came from and why it may be redistributed.

    scripts/demo-sources.py [out_dir]     # default: tmp/demo-sources

Every source is checked against its collection's own rights field (Met
`isPublicDomain`, Art Institute `is_public_domain`, the Library of Congress
item's rights statement); one that no longer passes stops the script.
NASA imagery is not subject to US copyright (NASA media guidelines).
"""

import json
import os
import sys
import urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.abspath(sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "tmp", "demo-sources"))

# (file stem, collection, id)
SOURCES = [
    ("great-wave", "met", "45434"),
    ("wheat-field", "met", "436535"),
    ("grande-jatte", "aic", "27992"),
    ("moonwalk", "nasa", "as11-40-5903"),
    ("earthrise", "nasa", "as08-14-2383"),
    ("zion-poster", "loc", "2007676134"),
    ("grand-canyon-poster", "loc", "2007676131"),
    ("liberty-photochrom", "loc", "2008679689"),
    ("sydney-harbour", "loc", "2017657736"),
]


def get(url):
    req = urllib.request.Request(url, headers={"User-Agent": "increart-demo-sources/1.0"})
    with urllib.request.urlopen(req, timeout=60) as r:
        return r.read()


def met(oid):
    d = json.loads(get(f"https://collectionapi.metmuseum.org/public/collection/v1/objects/{oid}"))
    assert d["isPublicDomain"], f"met {oid} is not public domain"
    return {
        "title": d["title"],
        "creator": d["artistDisplayName"],
        "date": d["objectDate"],
        "collection": "The Metropolitan Museum of Art",
        "page": d["objectURL"],
        "rights": "Public domain (Met Open Access, CC0)",
        "image": d["primaryImage"],
    }


def aic(oid):
    d = json.loads(get(f"https://api.artic.edu/api/v1/artworks/{oid}?fields=id,title,artist_title,date_display,is_public_domain,image_id"))
    a = d["data"]
    assert a["is_public_domain"], f"aic {oid} is not public domain"
    return {
        "title": a["title"],
        "creator": a["artist_title"],
        "date": a["date_display"],
        "collection": "Art Institute of Chicago",
        "page": f"https://www.artic.edu/artworks/{oid}",
        "rights": "Public domain (Art Institute of Chicago, CC0)",
        "image": f"{d['config']['iiif_url']}/{a['image_id']}/full/1686,/0/default.jpg",
    }


def nasa(nid):
    d = json.loads(get(f"https://images-api.nasa.gov/search?nasa_id={nid}"))
    item = d["collection"]["items"][0]
    meta = item["data"][0]
    return {
        "title": meta["title"],
        "creator": "NASA",
        "date": meta.get("date_created", "")[:10],
        "collection": "NASA Image and Video Library",
        "page": f"https://images.nasa.gov/details/{nid}",
        "rights": "NASA imagery, not subject to US copyright",
        "image": f"https://images-assets.nasa.gov/image/{nid}/{nid}~medium.jpg",
    }


def loc(pk):
    d = json.loads(get(f"https://www.loc.gov/pictures/item/{pk}/?fo=json"))
    item = d["item"]
    rights = item.get("rights_information", "")
    assert "no known restrictions" in rights.lower(), f"loc {pk} rights: {rights[:120]}"
    return {
        "title": item["title"],
        "creator": item.get("creator", "") or "",
        "date": item.get("date", ""),
        "collection": "Library of Congress Prints and Photographs",
        "page": f"https://www.loc.gov/pictures/item/{pk}/",
        "rights": "No known restrictions on publication (Library of Congress)",
        "image": d["resources"][0]["large"],
    }


def main():
    os.makedirs(OUT, exist_ok=True)
    credits = []
    for stem, kind, oid in SOURCES:
        info = {"met": met, "aic": aic, "nasa": nasa, "loc": loc}[kind](oid)
        path = os.path.join(OUT, stem + ".jpg")
        if not os.path.exists(path):
            data = get(info["image"])
            with open(path + ".part", "wb") as f:
                f.write(data)
            os.replace(path + ".part", path)
        credits.append({"file": stem + ".jpg", **info})
        print(f"{stem:22} {info['rights']}")
    with open(os.path.join(OUT, "credits.json"), "w") as f:
        json.dump(credits, f, indent=2, ensure_ascii=False)
        f.write("\n")


if __name__ == "__main__":
    main()
