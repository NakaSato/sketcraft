#!/usr/bin/env python3
"""Build the browser app (apps/sketcraft-web) into a static site, without Trunk.

    python3 scripts/build_web.py                 # release build into dist/web
    python3 scripts/build_web.py --debug         # faster to build, slower to run
    python3 scripts/build_web.py --serve         # build, then serve on http://127.0.0.1:8080
    python3 scripts/build_web.py --dist dist/site/play

Steps: `cargo build` for wasm32-unknown-unknown, `wasm-bindgen --target web`, `wasm-opt -Oz`
when it is installed (release only), then the page: apps/sketcraft-web/index.html with each
`<link data-trunk ...>` replaced by what it asks for (copy-file, css, the app's module script).
Release builds get the offline service worker from `cargo xtask web-offline`; debug builds get
the template, which retires a release worker left on the same origin.

Needs the wasm32-unknown-unknown target and `wasm-bindgen` at the version in Cargo.lock:
    rustup target add wasm32-unknown-unknown
    cargo install wasm-bindgen-cli --version <Cargo.lock's wasm-bindgen> --locked
"""
from __future__ import annotations

import argparse
import hashlib
import html
import http.server
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
APP = ROOT / "apps" / "sketcraft-web"
BIN = "sketcraft-web"
TARGET = "wasm32-unknown-unknown"
# The wasm features Rust enables by default for this target; wasm-opt must accept them.
WASM_OPT_FEATURES = ["--enable-bulk-memory", "--enable-nontrapping-float-to-int", "--enable-sign-ext",
                     "--enable-mutable-globals", "--enable-reference-types", "--enable-multivalue"]
LINK = re.compile(r"[ \t]*<link\s+data-trunk\s+([^>]*?)/?>[ \t]*\n?")
ATTR = re.compile(r'([\w-]+)(?:="([^"]*)")?')


def run(*cmd: str, **kw) -> None:
    print("+", " ".join(cmd), flush=True)
    subprocess.run(cmd, check=True, **kw)


def locked_version(name: str) -> str:
    lock = (ROOT / "Cargo.lock").read_text(encoding="utf-8")
    found = re.search(rf'name = "{re.escape(name)}"\nversion = "([^"]+)"', lock)
    if not found:
        sys.exit(f"build_web: {name} is not in Cargo.lock")
    return found.group(1)


def find_tool(name: str, version: str | None = None) -> str | None:
    """`name` on PATH or in ~/.cargo/bin, if it reports `version` (when given)."""
    for candidate in (shutil.which(name), str(Path.home() / ".cargo" / "bin" / name)):
        if not candidate or not Path(candidate).is_file():
            continue
        if version is None:
            return candidate
        out = subprocess.run([candidate, "--version"], capture_output=True, text=True).stdout
        if out.split()[-1:] == [version]:
            return candidate
    return None


def short_hash(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()[:16]


def build(dist: Path, release: bool) -> None:
    bindgen_version = locked_version("wasm-bindgen")
    bindgen = find_tool("wasm-bindgen", bindgen_version)
    if not bindgen:
        sys.exit(f"build_web: needs wasm-bindgen {bindgen_version} (the version in Cargo.lock):\n"
                 f"    cargo install wasm-bindgen-cli --version {bindgen_version} --locked")

    profile = ["--release"] if release else []
    run("cargo", "build", "-p", BIN, "--bin", BIN, "--target", TARGET, "--locked", *profile, cwd=ROOT)
    wasm = ROOT / "target" / TARGET / ("release" if release else "debug") / f"{BIN}.wasm"

    template = (APP / "index.html").read_text(encoding="utf-8")
    links = [dict((k, v) for k, v in ATTR.findall(m.group(1))) for m in LINK.finditer(template)]
    rust = next((link for link in links if link.get("rel") == "rust"), None)
    if rust is None:
        sys.exit("build_web: apps/sketcraft-web/index.html has no <link data-trunk rel=\"rust\">")

    if dist.exists():
        shutil.rmtree(dist)
    dist.mkdir(parents=True)

    # wasm-bindgen writes the JS glue, the wasm and any snippets/; name both after the wasm's contents.
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        weak_refs = ["--weak-refs"] if "data-weak-refs" in rust else []
        run(bindgen, "--target", "web", "--no-typescript", *weak_refs, "--out-dir", str(tmp), "--out-name", BIN, str(wasm))
        bg = tmp / f"{BIN}_bg.wasm"
        level = rust.get("data-wasm-opt")
        wasm_opt = find_tool("wasm-opt") or next(iter(sorted((Path.home() / "Library/Caches/.wasm-pack").glob("wasm-opt-*/bin/wasm-opt"))), None)
        if release and level and level != "0":
            if wasm_opt:
                try:
                    run(str(wasm_opt), f"-O{level}", *WASM_OPT_FEATURES, str(bg), "-o", str(bg))
                except subprocess.CalledProcessError:
                    print("build_web: wasm-opt failed; keeping the unoptimized wasm")
            else:
                print("build_web: wasm-opt not found; the wasm is not size-optimized (install binaryen)")
        stem = f"{BIN}-{short_hash(bg.read_bytes())}"
        shutil.move(str(tmp / f"{BIN}.js"), dist / f"{stem}.js")
        shutil.move(str(bg), dist / f"{stem}_bg.wasm")
        if (tmp / "snippets").exists():
            shutil.move(str(tmp / "snippets"), dist / "snippets")

    def replace(match: re.Match) -> str:
        attrs = dict(ATTR.findall(match.group(1)))
        rel, href = attrs.get("rel"), attrs.get("href")
        indent = re.match(r"[ \t]*", match.group(0)).group(0)
        if rel == "rust":
            return (f'{indent}<link rel="modulepreload" href="./{stem}.js" crossorigin="anonymous">\n'
                    f'{indent}<link rel="preload" href="./{stem}_bg.wasm" as="fetch" type="application/wasm" crossorigin="anonymous">\n'
                    f'{indent}<script type="module">\n'
                    f'{indent}  import init, * as bindings from "./{stem}.js";\n'
                    f'{indent}  await init({{ module_or_path: "./{stem}_bg.wasm" }});\n'
                    f'{indent}  window.wasmBindings = bindings;\n'
                    f'{indent}</script>\n')
        source = (APP / href).resolve()
        if rel == "copy-file":
            out = dist / attrs.get("data-target-path", "") / source.name
            out.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, out)
            return ""
        if rel == "css":
            data = source.read_bytes()
            name = f"{source.stem}-{short_hash(data)}{source.suffix}"
            (dist / name).write_bytes(data)
            return f'{indent}<link rel="stylesheet" href="./{html.escape(name)}">\n'
        sys.exit(f"build_web: unsupported <link data-trunk rel=\"{rel}\">")

    (dist / "index.html").write_text(LINK.sub(replace, template), encoding="utf-8")

    if release:
        run("cargo", "run", "-q", "-p", "xtask", "--", "web-offline", str(dist), cwd=ROOT)
    else:
        shutil.copyfile(APP / "sw.js", dist / "sw.js")
    print(f"build_web: {dist}")


class Handler(http.server.SimpleHTTPRequestHandler):
    extensions_map = {**http.server.SimpleHTTPRequestHandler.extensions_map,
                      ".wasm": "application/wasm", ".js": "text/javascript", ".webmanifest": "application/manifest+json"}

    def end_headers(self) -> None:
        self.send_header("Cache-Control", "no-cache")
        super().end_headers()


def serve(dist: Path, port: int) -> None:
    os.chdir(dist)
    with http.server.ThreadingHTTPServer(("127.0.0.1", port), Handler) as server:
        print(f"build_web: serving {dist} on http://127.0.0.1:{port}/ (Ctrl+C stops)", flush=True)
        try:
            server.serve_forever()
        except KeyboardInterrupt:
            pass


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--dist", default="dist/web", help="output directory (default: dist/web)")
    parser.add_argument("--debug", action="store_true", help="debug build: faster to build, slower to run")
    parser.add_argument("--serve", nargs="?", type=int, const=8080, metavar="PORT", help="serve the site afterwards (default port 8080)")
    parser.add_argument("--no-build", action="store_true", help="only serve what --dist already holds")
    args = parser.parse_args()
    dist = (ROOT / args.dist).resolve()
    if not args.no_build:
        build(dist, release=not args.debug)
    if args.serve is not None:
        serve(dist, args.serve)


if __name__ == "__main__":
    main()
