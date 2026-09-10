"""Decompose the toy-corpus (ir-corpus) gas increase ext-vs-ref by wide-op class, using
runblob RUNBLOB_GASDECOMP. For each module: build ref (v1) and vec (v2) blobs, run the decomposition,
and aggregate. full - z_<class> = that class's executed gas; full - scalar_only = total wide gas.
"""
import os, re, subprocess, tempfile, pathlib, sys

BASE = "+e,+m,+a,+c,+zbb,+auipc-addi-fusion,+ld-add-fusion,+lui-addi-fusion,+xtheadcondmov,+relax"
LLC = "/home/ubuntu/workspaces/parity-llvm/build/bin/llc"
LINK = "/home/ubuntu/workspaces/polkavm/target/release/polkatool"
RUNBLOB = "/home/ubuntu/workspaces/polkavm/target/release/runblob"
IR = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "/home/ubuntu/workspaces/revive/benchmarks/ir-corpus")
ATTR = re.compile(r'"target-features"="[^"]*"')
MODELS = ["full", "scalar_only", "z_convert", "z_memory", "z_move", "z_linear", "z_mul", "z_divrem", "z_modexp"]


def gasdecomp(src, ext):
    feat = ",+xrevivevec" if ext else ""
    isa = "revive_v2" if ext else "revive_v1"
    fh = tempfile.NamedTemporaryFile("w", suffix=".ll", delete=False)
    fh.write(ATTR.sub(f'"target-features"="{BASE}{feat}"', src.read_text()))
    fh.close()
    p = fh.name
    obj, blob = p + ".o", p + ".polkavm"
    out = {}
    try:
        if subprocess.run([LLC, "-O2", f"-mattr={BASE}{feat}", "-filetype=obj", "-o", obj, p],
                          capture_output=True, timeout=120).returncode:
            return None
        if subprocess.run([LINK, "link", "--isa", isa, "-o", blob, obj], capture_output=True, timeout=120).returncode:
            return None
        r = subprocess.run([RUNBLOB, blob], capture_output=True, text=True, timeout=120,
                           env={**os.environ, "RUNBLOB_GASDECOMP": "1"})
        for line in r.stdout.splitlines():
            m = re.match(r"\s+(\w+)\s+(-?\d+)", line)
            if m and m.group(1) in MODELS:
                out[m.group(1)] = int(m.group(2))
    except subprocess.TimeoutExpired:
        return None
    finally:
        for x in (p, obj, blob):
            try: os.unlink(x)
            except OSError: pass
    return out if len(out) == len(MODELS) else None


files = sorted(IR.rglob("*.optimized.ll"))
agg = {"ref_full": 0, "vec_full": 0, "vec_scalar": 0,
       "convert": 0, "memory": 0, "move": 0, "linear": 0, "mul": 0, "divrem": 0, "modexp": 0}
n = 0
worst = []
for i, src in enumerate(files, 1):
    ref = gasdecomp(src, False)
    vec = gasdecomp(src, True)
    if not ref or not vec:
        continue
    n += 1
    agg["ref_full"] += ref["full"]
    agg["vec_full"] += vec["full"]
    agg["vec_scalar"] += vec["scalar_only"]
    for cls, key in (("z_convert", "convert"), ("z_memory", "memory"), ("z_move", "move"),
                     ("z_linear", "linear"), ("z_mul", "mul"), ("z_divrem", "divrem"), ("z_modexp", "modexp")):
        agg[key] += vec["full"] - vec[cls]
    delta = vec["full"] - ref["full"]
    worst.append((delta, src.name.replace(".optimized.ll", "").replace("crates_integration_contracts_", ""),
                  ref["full"], vec["full"]))
    if i % 20 == 0:
        print(f"  ... {i}/{len(files)}", flush=True)

print(f"\n=== toy corpus gas decomposition: {n} modules ran in both ref and vec ===")
print(f"ref  total gas: {agg['ref_full']:,}")
print(f"vec  total gas: {agg['vec_full']:,}  ({agg['vec_full']/agg['ref_full']:.3f}x ref)")
print(f"  vec scalar-only gas:     {agg['vec_scalar']:,}  (scalar delta vs ref: {agg['vec_scalar']-agg['ref_full']:+,})")
print(f"  total wide-op gas:       {agg['vec_full']-agg['vec_scalar']:,}")
for key in ("memory", "convert", "linear", "move", "mul", "divrem", "modexp"):
    print(f"    {key:8}: {agg[key]:+,}")
print(f"\nnet increase (vec-ref): {agg['vec_full']-agg['ref_full']:+,}")
print("\ntop 10 modules by absolute gas increase (ref -> vec):")
for d, name, rf, vf in sorted(worst, reverse=True)[:10]:
    print(f"  {name:40} {rf:>6} -> {vf:>6}  ({d:+}, {vf/rf:.2f}x)" if rf else f"  {name:40} {rf} -> {vf}")
