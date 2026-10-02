#!/usr/bin/env python3
"""Writes THIRD-PARTY-LICENSES.txt: the licenses of everything compiled or
bundled into Magpie (Rust crates on every platform, fonts and icons).

MIT, Apache, OFL and similar licenses ask that their notices travel with the
software, so this file ships in every release download and is built into the
app (`magpie --licenses`). Re-run after changing dependencies:

    python3 scripts/third-party-licenses.py
"""

import hashlib
import json
import pathlib
import re
import subprocess

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "THIRD-PARTY-LICENSES.txt"
OURS = {"magpie-finance", "magpie-core"}
LICENSE_FILE = re.compile(r"^(LICEN[CS]E|COPYING|NOTICE|UNLICENSE|COPYRIGHT)", re.I)


def shipped_packages(meta):
    """Packages reachable from the app through normal (non-dev, non-build)
    dependencies, on any target platform."""
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    pkgs = {p["id"]: p for p in meta["packages"]}
    root = next(p["id"] for p in meta["packages"] if p["name"] == "magpie-finance")
    seen, stack = set(), [root]
    while stack:
        pid = stack.pop()
        if pid in seen:
            continue
        seen.add(pid)
        for dep in nodes[pid]["deps"]:
            if any(k["kind"] is None for k in dep["dep_kinds"]):
                stack.append(dep["pkg"])
    return sorted((pkgs[i] for i in seen if pkgs[i]["name"] not in OURS), key=lambda p: (p["name"], p["version"]))


def license_texts(pkg):
    d = pathlib.Path(pkg["manifest_path"]).parent
    files = sorted(f for f in d.iterdir() if f.is_file() and LICENSE_FILE.match(f.name))
    return [f.read_text(errors="replace").strip() for f in files]


def main():
    meta = json.loads(
        subprocess.check_output(["cargo", "metadata", "--format-version", "1", "--locked"], cwd=ROOT)
    )
    groups = {}  # text hash -> (text, [package names])
    no_file = []
    for p in shipped_packages(meta):
        texts = license_texts(p)
        label = f"{p['name']} {p['version']}"
        if not texts:
            no_file.append(f"{label} ({p.get('license') or 'see crate'})")
            continue
        for text in texts:
            key = hashlib.sha256(" ".join(text.split()).encode()).hexdigest()
            groups.setdefault(key, (text, []))[1].append(label)

    out = [
        "Third-party licenses",
        "====================",
        "",
        "Magpie itself is MIT licensed (see LICENSE). It includes the following",
        "fonts, icons and Rust libraries, under their own licenses.",
        "",
    ]

    def section(title, text):
        out.extend(["", "-" * 78, title, "-" * 78, "", text.strip(), ""])

    fonts = ROOT / "crates" / "app" / "assets" / "fonts"
    section("Inter, Inter Display (SIL Open Font License 1.1)", (fonts / "Inter-LICENSE.txt").read_text())
    section(
        "Geist, Onest, Plus Jakarta Sans, DM Sans, Figtree, Outfit, IBM Plex Sans, JetBrains Mono (SIL Open Font License 1.1)",
        (fonts / "OFL-LICENSES.txt").read_text(),
    )
    reg = pathlib.Path(meta["packages"][0]["manifest_path"]).parents[1]
    egui_fonts = sorted(reg.glob("epaint_default_fonts-*/fonts"))
    if egui_fonts:
        f = egui_fonts[-1]
        section("Ubuntu Light (Ubuntu Font Licence 1.0), via egui", (f / "UFL.txt").read_text())
        section("Hack (MIT / Bitstream Vera License), via egui", (f / "Hack-Regular.txt").read_text())
        section("Noto Emoji (SIL Open Font License 1.1), via egui", (f / "OFL.txt").read_text())
        section("emoji-icon-font (MIT), via egui", (f / "emoji-icon-font-mit-license.txt").read_text())

    for text, names in sorted(groups.values(), key=lambda g: g[1][0]):
        section("Used by: " + ", ".join(names), text)
    if no_file:
        section(
            "Packages without a license file (license as declared on crates.io)",
            "\n".join(no_file),
        )
    OUT.write_text("\n".join(out).rstrip() + "\n")
    print(f"wrote {OUT.relative_to(ROOT)}: {len(groups)} license texts, {OUT.stat().st_size // 1024} KiB")


if __name__ == "__main__":
    main()
