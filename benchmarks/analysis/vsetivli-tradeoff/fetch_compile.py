"""Fetch a verified mainnet contract's sources from Sourcify, compile with resolc (matching solc),
and emit its *.optimized.ll into ir/ for the benchmark harness.

Usage: python3 fetch_compile.py <label> <address> [<contract_name_to_keep>]
Records a one-line status to status.tsv. Robust to multi-file sources (reconstructs the tree and
compiles with the source root as base path). Skips pre-0.8 contracts (resolc supports 0.8.0-0.8.36).
"""
import json, os, subprocess, sys, urllib.request, pathlib, re

HERE = pathlib.Path(__file__).resolve().parent
SOLC_DIR = HERE / "solc"
SRC_DIR = HERE / "src"
IR_DIR = HERE / "ir"
for d in (SOLC_DIR, SRC_DIR, IR_DIR):
    d.mkdir(exist_ok=True)

SOLC_BASE = "https://binaries.soliditylang.org/linux-amd64/solc-linux-amd64-v"
SOURCIFY = "https://sourcify.dev/server/v2/contract/1/"
RESOLC = "resolc"
MIN, MAX = (0, 8, 0), (0, 8, 36)


def get(url):
    # The solc-binaries CDN 403s the default urllib UA; send a browser-like one.
    req = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0 (X11; Linux x86_64) revive-bench/1.0"})
    with urllib.request.urlopen(req, timeout=120) as r:
        return r.read()


def solc_for(version):  # version like "0.8.12+commit.f00d7308"
    path = SOLC_DIR / f"solc-{version.split('+')[0]}"
    if not path.exists():
        path.write_bytes(get(SOLC_BASE + version))
        path.chmod(0o755)
    return path


def main():
    label, addr = sys.argv[1], sys.argv[2]
    keep = sys.argv[3] if len(sys.argv) > 3 else None
    try:
        d = json.loads(get(SOURCIFY + addr + "?fields=sources,compilation,metadata"))
    except Exception as e:
        print(f"{label}\t{addr}\tFETCH_FAIL\t{e}")
        return
    comp = d.get("compilation", {})
    ver = comp.get("compilerVersion", "?")
    name = keep or comp.get("name", label)
    tup = tuple(int(x) for x in ver.split("+")[0].split("."))
    if not (MIN <= tup <= MAX):
        print(f"{label}\t{addr}\t{name}\t{ver}\tUNSUPPORTED_SOLC")
        return
    # Reconstruct the source tree under src/<label>/
    root = SRC_DIR / label
    root.mkdir(parents=True, exist_ok=True)
    srcs = d.get("sources", {})
    files = {}
    for spath, sc in srcs.items():
        content = sc["content"] if isinstance(sc, dict) else sc
        safe = spath.lstrip("/")
        fp = root / safe
        fp.parent.mkdir(parents=True, exist_ok=True)
        fp.write_text(content)
        files[safe] = fp
    # Remappings from metadata settings (e.g. @openzeppelin/=...).
    remaps = []
    meta = d.get("metadata") or {}
    if isinstance(meta, dict):
        remaps = (meta.get("settings", {}) or {}).get("remappings", []) or []
    solc = solc_for(ver)
    out = IR_DIR / label
    if out.exists():
        for f in out.glob("*"):
            f.unlink()
    out.mkdir(parents=True, exist_ok=True)
    cmd = [RESOLC, *remaps, *[str(p.relative_to(root)) for p in files.values()],
           "--solc", str(solc), "--base-path", ".", "-O2", "--debug-output-dir", str(out.resolve())]
    r = subprocess.run(cmd, cwd=root, capture_output=True, text=True, timeout=1800)
    produced = sorted(out.glob("*.optimized.ll"))
    if r.returncode != 0 and not produced:
        err = (r.stderr or r.stdout).strip().splitlines()
        print(f"{label}\t{addr}\t{name}\t{ver}\tCOMPILE_FAIL\t{err[-1] if err else ''}")
        return
    # Select the target contract's optimized.ll. The filename is
    # <source_path>.<ContractName>.optimized.ll, and many contracts share a source-file prefix that
    # may itself contain `name`, so match the exact contract-name suffix; fall back to the largest.
    exact = [f for f in produced if f.name.endswith(f".{name}.optimized.ll")]
    if exact:
        target = max(exact, key=lambda f: f.stat().st_size)
    else:
        target = max(produced, key=lambda f: f.stat().st_size) if produced else None
    if not target:
        print(f"{label}\t{addr}\t{name}\t{ver}\tNO_IR")
        return
    dest = IR_DIR / f"real_{label}.{name}.optimized.ll"
    dest.write_text(target.read_text())
    print(f"{label}\t{addr}\t{name}\t{ver}\tOK\t{dest.name}\t{dest.stat().st_size}B")


main()
