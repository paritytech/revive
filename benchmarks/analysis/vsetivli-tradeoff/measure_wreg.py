"""Per-benchmark comparison of the dedicated-register-file prototype (XReviveW) against the
RVV/vtype variant (XReviveVec) and the scalar reference, on code size (.text object, PolkaVM blob)
and execution (interpreter, recompiler).

Arms:
  ref  -- no extension (scalar limb chains)
  vec  -- +xrevivevec (i256 in RVV groups, width from vtype)
  w    -- +xrevivew   (i256 in the dedicated W0-W15 file, no vtype; linked with POLKAVM_ASSUME_W256)

Uses the freshly built prototype llc and polkatool. Same measurement method as measure_per_bench.py
(best-of-3 compile, amortized runblob timing), comparing only modules that ran in all arms.
"""
import os, pathlib, re, subprocess, tempfile, time

HERE = pathlib.Path(__file__).resolve().parent
BIN = HERE / "bin"
IR = pathlib.Path(os.environ.get("VEC_IR") or HERE.parents[1] / "ir-corpus")
TIMEOUT = int(os.environ.get("TIMEOUT", "60"))
BEST_OF = 3

BASE = "+e,+m,+a,+c,+zbb,+auipc-addi-fusion,+ld-add-fusion,+lui-addi-fusion,+xtheadcondmov,+relax"
LLC = pathlib.Path("/home/ubuntu/workspaces/parity-llvm/build/bin/llc")
SIZE = BIN / "llvm-size"
LINK = pathlib.Path("/home/ubuntu/workspaces/polkavm/target/release/polkatool")
RUNBLOB = BIN / "runblob"
ATTR = re.compile(r'"target-features"="[^"]*"')
EXPORT = re.compile(r'^  (\S+): (.*?) gas=(-?\d+) steps=(\d+)(?: time_ns=(\d+))?$', re.M)
ITERS = int(os.environ.get("RUNBLOB_ITERS", "50"))

# ext: whether the linker treats it as revive_v2; feat: the target-feature suffix; w: needs the
# assume-256 link flag.
ARMS = {
    "ref": dict(ext=False, feat="", w=False),
    "vec": dict(ext=True, feat=",+xrevivevec", w=False),
    "w":   dict(ext=True, feat=",+xrevivew", w=True),
}


def ir_for(src, feat):
    fh = tempfile.NamedTemporaryFile("w", suffix=".ll", delete=False)
    fh.write(ATTR.sub(f'"target-features"="{BASE}{feat}"', src.read_text()))
    fh.close()
    return fh.name


def sizes(obj):
    out = subprocess.run([str(SIZE), "-A", obj], capture_output=True, text=True).stdout
    t = 0
    for row in out.splitlines():
        p = row.split()
        if len(p) >= 2 and p[1].isdigit() and p[0].startswith(".text"):
            t += int(p[1])
    return t


def best(fn):
    b = float("inf")
    for _ in range(BEST_OF):
        v = fn()
        if v is None:
            return None
        b = min(b, v)
    return b


def profile(src, arm):
    spec = ARMS[arm]
    r = dict(module=src.name, arm=arm)
    path = ir_for(src, spec["feat"])
    obj, blob = path + ".o", path + ".polkavm"
    llc_cmd = [str(LLC), "-O2", f"-mattr={BASE}{spec['feat']}", "-filetype=obj", "-o", obj, path]
    try:
        def do_compile():
            s = time.perf_counter()
            x = subprocess.run(llc_cmd, capture_output=True, timeout=TIMEOUT)
            return None if x.returncode else time.perf_counter() - s
        r["compile_s"] = best(do_compile)
        if r["compile_s"] is None:
            r["error"] = "compile"
            return r
        r["text"] = sizes(obj)

        isa = "revive_v2" if spec["ext"] else "revive_v1"
        env = {**os.environ}
        if spec["w"]:
            env["POLKAVM_ASSUME_W256"] = "1"
        x = subprocess.run([str(LINK), "link", "--isa", isa, "-o", blob, obj],
                           capture_output=True, timeout=TIMEOUT, env=env)
        if x.returncode:
            r["error"] = "link"
            return r
        r["blob"] = os.path.getsize(blob)

        for backend, key in (("interpreter", "interp_ns"), ("compiler", "recomp_ns")):
            x = subprocess.run([str(RUNBLOB), blob], capture_output=True, text=True, timeout=TIMEOUT,
                               env={**env, "RUNBLOB_BACKEND": backend, "RUNBLOB_ITERS": str(ITERS)})
            if x.returncode:
                r[key] = None
                continue
            found = EXPORT.findall(x.stdout)
            r[key] = sum(int(t) for *_, t in found if t) or None
            if backend == "interpreter":
                r["gas"] = sum(int(g) for _, _, g, _, _ in found)
                r["exports"] = len(found)
        r["ran"] = r.get("interp_ns") is not None and r.get("recomp_ns") is not None
    except subprocess.TimeoutExpired:
        r["error"] = "timeout"
    finally:
        for p in (path, obj, blob):
            try: os.unlink(p)
            except OSError: pass
    return r


def main():
    files = sorted(IR.rglob("*.optimized.ll"))
    print(f"corpus: {len(files)} modules; arms {list(ARMS)}; best-of-{BEST_OF}")
    rows = []
    for i, src in enumerate(files, 1):
        for arm in ARMS:
            rows.append(profile(src, arm))
        if i % 10 == 0:
            print(f"  ... {i}/{len(files)} modules", flush=True)

    keys = ["module", "arm", "compile_s", "text", "blob", "gas", "interp_ns", "recomp_ns", "exports", "error"]
    tsv = HERE / "per-bench-wreg.tsv"
    with tsv.open("w") as fh:
        fh.write("\t".join(keys) + "\n")
        for r in rows:
            fh.write("\t".join("" if r.get(k) is None else str(r.get(k, "")) for k in keys) + "\n")
    print(f"wrote {tsv}")

    by = {}
    for r in rows:
        by.setdefault(r["module"], {})[r["arm"]] = r

    # Compare vec vs w on the set where BOTH ran (that is the head-to-head we care about).
    both = [m for m, d in by.items() if d.get("vec", {}).get("ran") and d.get("w", {}).get("ran")]
    print(f"\n{len(both)} modules ran in BOTH vec and w\n")

    agg = {a: dict(text=0, blob=0, interp=0, recomp=0, gas=0) for a in ("ref", "vec", "w")}
    n_ref = 0
    for m in both:
        d = by[m]
        for a in ("vec", "w"):
            agg[a]["text"] += d[a]["text"]; agg[a]["blob"] += d[a]["blob"]
            agg[a]["interp"] += d[a]["interp_ns"]; agg[a]["recomp"] += d[a]["recomp_ns"]
            agg[a]["gas"] += d[a]["gas"]
        if d.get("ref", {}).get("ran"):
            n_ref += 1
            for k, kk in (("text", "text"), ("blob", "blob"), ("interp_ns", "interp"),
                          ("recomp_ns", "recomp"), ("gas", "gas")):
                agg["ref"][kk] += d["ref"][k]

    def line(label, a, base=None):
        d = agg[a]
        def rel(x, y):
            return f"{x/y:.3f}x" if base and y else ""
        b = agg[base] if base else None
        print(f"{label:<6} text={d['text']:>9}{rel(d['text'], b['text']) if b else '':>9}  "
              f"blob={d['blob']:>9}{rel(d['blob'], b['blob']) if b else '':>9}  "
              f"interp_us={d['interp']/1000:>10.1f}{rel(d['interp'], b['interp']) if b else '':>9}  "
              f"recomp_us={d['recomp']/1000:>10.1f}{rel(d['recomp'], b['recomp']) if b else '':>9}  "
              f"gas={d['gas']:>10}{rel(d['gas'], b['gas']) if b else '':>9}")

    print(f"aggregates over {len(both)} modules (vec vs w); ref over the {n_ref} it also compiled\n")
    line("vec", "vec")
    line("w  ", "w", base="vec")
    main.by = by
    main.both = both


main()
