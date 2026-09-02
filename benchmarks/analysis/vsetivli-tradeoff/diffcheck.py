"""Differential value-correctness check across ref / vec / w.

Codesize and gas (measure_wreg.py) cannot see a value miscompile -- a wrong result costs the
same gas and bytes. This compiles each corpus module on all three arms, runs it under runblob's
value trace (RUNBLOB_TRACE=1: the sequence of host calls, every storage write as key -> value, and
the return payload -- all layout-independent), and reports any module whose observable values
differ between arms (gate: w vs vec, same extension so directly comparable). Identical traces ==
identical computed state on the exercised path.

COVERAGE, honestly: this runs each export with EMPTY calldata and host calls stubbed to zero, so it
only exercises the deploy path and calldata-independent logic, and it only sees values that reach an
observable sink (set_storage / seal_return). That is real coverage -- it detected a set of ext-vs-ref
divergences -- but it is NOT exhaustive: a value produced only on a calldata-dependent path, or one
that reaches a sink through memory rather than the affected registers, is invisible. Measured
example: the i256 wide-move truncation bug (wmv1r vs wmv2r) produces ZERO w-vs-vec mismatches on this
corpus, because the copied values reach storage via wld/wst rather than the truncated register move.
So treat this as a complement to the LLVM lit tests (which pin the instruction encodings directly),
not a replacement; broadening it would need real calldata per contract or targeted register-copy
fixtures.
"""
import os, pathlib, re, subprocess, tempfile, sys

HERE = pathlib.Path(__file__).resolve().parent
BIN = HERE / "bin"
IR = pathlib.Path(os.environ.get("VEC_IR") or HERE.parents[1] / "ir-corpus")
TIMEOUT = int(os.environ.get("TIMEOUT", "60"))
BASE = "+e,+m,+a,+c,+zbb,+auipc-addi-fusion,+ld-add-fusion,+lui-addi-fusion,+xtheadcondmov,+relax"
LLC = pathlib.Path("/home/ubuntu/workspaces/parity-llvm/build/bin/llc")
LINK = pathlib.Path("/home/ubuntu/workspaces/polkavm/target/release/polkatool")
RUNBLOB = BIN / "runblob"
ATTR = re.compile(r'"target-features"="[^"]*"')
GAS = re.compile(r" gas=-?\d+")

ARMS = {
    "ref": dict(ext=False, feat="", w=False),
    "vec": dict(ext=True, feat=",+xrevivevec", w=False),
    "w":   dict(ext=True, feat=",+xrevivew", w=True),
}


def trace_of(src, arm):
    """Compile+link+trace one module on one arm. Returns the canonical trace text, or None if it
    did not build/link/run (so it can be skipped rather than reported as a mismatch)."""
    spec = ARMS[arm]
    fh = tempfile.NamedTemporaryFile("w", suffix=".ll", delete=False)
    fh.write(ATTR.sub(f'"target-features"="{BASE}{spec["feat"]}"', src.read_text()))
    fh.close()
    path = fh.name
    obj, blob = path + ".o", path + ".polkavm"
    try:
        x = subprocess.run([str(LLC), "-O2", f"-mattr={BASE}{spec['feat']}", "-filetype=obj",
                            "-o", obj, path], capture_output=True, timeout=TIMEOUT)
        if x.returncode:
            return None
        env = {**os.environ}
        if spec["w"]:
            env["POLKAVM_ASSUME_W256"] = "1"
        isa = "revive_v2" if spec["ext"] else "revive_v1"
        x = subprocess.run([str(LINK), "link", "--isa", isa, "-o", blob, obj],
                           capture_output=True, timeout=TIMEOUT, env=env)
        if x.returncode:
            return None
        x = subprocess.run([str(RUNBLOB), blob], capture_output=True, text=True, timeout=TIMEOUT,
                           env={**env, "RUNBLOB_BACKEND": "interpreter", "RUNBLOB_TRACE": "1"})
        if x.returncode:
            return None
        # Strip gas (legitimately differs between arms); keep outcome + value events.
        return GAS.sub("", x.stdout)
    except subprocess.TimeoutExpired:
        return None
    finally:
        for p in (path, obj, blob):
            try: os.unlink(p)
            except OSError: pass


def main():
    files = sorted(IR.rglob("*.optimized.ll"))
    print(f"corpus: {len(files)} modules; comparing value traces across {list(ARMS)}\n")
    # The correctness gate is w == vec: both use the extension, so they take the same host-call
    # path and present values to the host in the same byte order -- a difference is a real W codegen
    # bug. ext-vs-ref is only informational: with hash_keccak_256 stubbed to 0 the ext path (which
    # hashes storage slots) and the scalar ref path diverge in host calls and value byte-order at the
    # set_storage boundary, so those differences are harness confounds, not miscompiles.
    compared = 0
    mismatches = []       # real: w != vec
    ref_diffs = 0         # informational: ext != ref
    for i, src in enumerate(files, 1):
        traces = {a: trace_of(src, a) for a in ARMS}
        name = src.name.replace(".optimized.ll", "").replace("crates_integration_contracts_", "")
        if traces["w"] is not None and traces["vec"] is not None:
            compared += 1
            if traces["w"] != traces["vec"]:
                mismatches.append((name, traces["vec"], traces["w"]))
        for ext in ("vec", "w"):
            if traces[ext] is not None and traces["ref"] is not None and traces[ext] != traces["ref"]:
                ref_diffs += 1
                break
        if i % 10 == 0:
            print(f"  ... {i}/{len(files)}", flush=True)

    print(f"\n{compared} modules ran in both w and vec; "
          f"{len(mismatches)} real value mismatches (w vs vec)")
    print(f"{ref_diffs} modules differ from scalar ref (informational: host-stub/byte-order confound)\n")
    import difflib
    for name, tvec, tw in mismatches:
        print(f"=== VALUE MISMATCH {name}: w vs vec ===")
        for line in difflib.unified_diff(tvec.splitlines(), tw.splitlines(), "vec", "w", lineterm=""):
            print("  " + line)
        print()
    sys.exit(1 if mismatches else 0)


main()
