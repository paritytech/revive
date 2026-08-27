"""End-to-end comparison of the four ways of carrying a wide instruction's width.

For every module in the corpus and every arm: compile the IR to an object, link it to a PolkaVM
blob, and run every export on the interpreter. Records object size, blob size, link time and the
gas each export consumes.

Gas rather than wall-clock: it is deterministic, identical between the interpreter and the
recompiler, and so measures the work the program does rather than the noise of the machine
measuring it.

  ref            no extension
  no-vsetvli     width in funct7; funct7 llc, funct7 linker
  vsetvli-noMO   width from vtype, outliner barred; vtype llc, vtype linker (dataflow)
  vsetvli-MO     width from vtype, outlining allowed; same linker, which must then attribute a
                 width to code whose vtype comes from the call site
"""
import concurrent.futures, os, pathlib, re, subprocess, sys, tempfile, time

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / "vec-ext"))
import measure

HERE = pathlib.Path(__file__).resolve().parent
BIN = HERE / "bin"
IR = pathlib.Path(os.environ.get("VEC_IR") or HERE.parent / "ir-intrinsics-oz")
TIMEOUT = int(os.environ.get("TIMEOUT", "120"))

ARMS = {
    "ref":          dict(llc=BIN/"llc-vsetivli", ext=False, flags=[], link=None),
    "no-vsetvli":   dict(llc=BIN/"llc-funct7",   ext=True,  flags=[], link=BIN/"polkatool-funct7"),
    "vsetvli-noMO": dict(llc=BIN/"llc-vsetivli", ext=True,  flags=[], link=BIN/"polkatool-vsetivli"),
    "vsetvli-MO":   dict(llc=BIN/"llc-vsetivli", ext=True,
                         flags=["-riscv-revive-outline-vtype=true"], link=BIN/"polkatool-vsetivli"),
}
RUNBLOB = BIN / "runblob"
EXPORT = re.compile(r'^  (\S+): (.*?) gas=(-?\d+) steps=(\d+)$', re.M)


def one(job):
    arm, source = job
    spec = ARMS[arm]
    edits = [("xrevivevec", True)] if spec["ext"] else []
    features = measure.BASE_FEATURES + (",+xrevivevec" if spec["ext"] else "")
    with tempfile.NamedTemporaryFile("w", suffix=".ll", delete=False) as fh:
        fh.write(measure.rewrite(source.read_text(), edits))
        path = fh.name
    obj, blob = path + ".o", path + ".polkavm"
    out = dict(arm=arm, module=source.parent.name + "/" + source.name)
    try:
        done = subprocess.run([str(spec["llc"]), "-O2", f"-mattr={features}", *spec["flags"],
                               "-filetype=obj", "-o", obj, path],
                              capture_output=True, text=True, timeout=TIMEOUT)
        if done.returncode:
            out["error"] = "compile: " + (done.stderr.strip().splitlines() or [""])[0][:120]
            return out
        out["obj_bytes"] = os.path.getsize(obj)
        # `.text` as well as the whole file: the file carries relocations and symbols, which say
        # nothing about code size, and the object-level and blob-level figures diverge enough that
        # both belong in the same table.
        size_out = subprocess.run([str(measure.TOOLS / "llvm-size"), "-A", obj],
                                  capture_output=True, text=True).stdout
        text = rodata = 0
        for row in size_out.splitlines():
            parts = row.split()
            if len(parts) < 2 or not parts[1].isdigit():
                continue
            if parts[0].startswith(".text"):
                text += int(parts[1])
            elif parts[0].startswith((".rodata", ".srodata")):
                rodata += int(parts[1])
        out["text_bytes"], out["rodata_bytes"] = text, rodata

        # `ref` has no extension, so the funct7 linker (which decodes no wide instruction it
        # would meet) links it just as well; use it for a like-for-like blob size.
        linker = spec["link"] or BIN / "polkatool-funct7"
        isa = "revive_v2" if spec["ext"] else "revive_v1"
        start = time.perf_counter()
        done = subprocess.run([str(linker), "link", "--isa", isa, "-o", blob, obj],
                              capture_output=True, text=True, timeout=TIMEOUT)
        out["link_secs"] = time.perf_counter() - start
        if done.returncode:
            err = (done.stderr.strip() or done.stdout.strip()).splitlines()
            out["error"] = "link: " + (err[0][:160] if err else "?")
            return out
        out["blob_bytes"] = os.path.getsize(blob)

        done = subprocess.run([str(RUNBLOB), blob], capture_output=True, text=True,
                              timeout=TIMEOUT, env={**os.environ, "RUNBLOB_BACKEND": "interpreter"})
        if done.returncode:
            out["error"] = "run: " + (done.stderr.strip().splitlines() or [""])[0][:160]
            return out
        # Keep each export's outcome, not just its gas: an arm that faults early burns little
        # gas while doing none of the work, and averaging that in would read as a speed-up.
        found = EXPORT.findall(done.stdout)
        out["per_export"] = {name: (outcome, int(gas), int(steps))
                             for name, outcome, gas, steps in found}
        out["gas"] = sum(g for _, g, _ in out["per_export"].values())
        out["exports"] = len(found)
        out["ran"] = True
    except subprocess.TimeoutExpired:
        out["error"] = f"timeout after {TIMEOUT}s"
    finally:
        for p in (path, obj, blob):
            try: os.unlink(p)
            except OSError: pass
    return out


def main():
    files = sorted(IR.rglob("*.optimized.ll"))
    jobs = [(arm, f) for arm in ARMS for f in files]
    rows = []
    with concurrent.futures.ThreadPoolExecutor(max(1, os.cpu_count() - 2)) as ex:
        for i, row in enumerate(ex.map(one, jobs), 1):
            rows.append(row)
            if i % 50 == 0:
                print(f"  ... {i}/{len(jobs)}", flush=True)

    by = {a: {r["module"]: r for r in rows if r["arm"] == a} for a in ARMS}
    modules = sorted(by["ref"])
    common = [m for m in modules if all(by[a].get(m, {}).get("ran") for a in ARMS)]
    print(f"\ncorpus {len(modules)} modules; {len(common)} compiled, linked and ran in every arm")
    for a in ARMS:
        errs = [r for r in by[a].values() if "error" in r]
        if errs:
            kinds = {}
            for r in errs:
                kinds.setdefault(r["error"].split(":")[0], []).append(r)
            print(f"  {a}: {len(errs)} failed " +
                  ", ".join(f"{k}={len(v)}" for k, v in kinds.items()))
            print(f"      e.g. {errs[0]['module']}: {errs[0]['error'][:130]}")

    if common:
        # Gas is only comparable where every arm ran the same export to the same outcome, having
        # taken the same number of host calls. Anything else is measuring different work.
        matched = {}
        for m in common:
            names = set(by["ref"][m]["per_export"])
            for a in ARMS:
                names &= set(by[a][m]["per_export"])
            for name in names:
                shape = {(by[a][m]["per_export"][name][0], by[a][m]["per_export"][name][2])
                         for a in ARMS}
                if len(shape) == 1:
                    matched[(m, name)] = {a: by[a][m]["per_export"][name][1] for a in ARMS}
        total_exports = sum(len(by["ref"][m]["per_export"]) for m in common)
        print(f"\ngas compared over {len(matched)} of {total_exports} exports that every arm ran "
              f"identically")

        print(f"\n{'arm':<14}{'.text':>11}{'.rodata':>10}{'blob':>10}{'link s':>8}{'gas':>11}"
              f"{'text/ref':>10}{'blob/ref':>10}{'gas/ref':>9}")
        base_blob = sum(by["ref"][m]["blob_bytes"] for m in common)
        base_gas = sum(v["ref"] for v in matched.values()) or 1
        base_text = sum(by["ref"][m]["text_bytes"] for m in common)
        for a in ARMS:
            tx = sum(by[a][m]["text_bytes"] for m in common)
            ro = sum(by[a][m]["rodata_bytes"] for m in common)
            bl = sum(by[a][m]["blob_bytes"] for m in common)
            lk = sum(by[a][m]["link_secs"] for m in common)
            ga = sum(v[a] for v in matched.values())
            print(f"{a:<14}{tx:>11,}{ro:>10,}{bl:>10,}{lk:>8.1f}{ga:>11,}"
                  f"{100*(tx-base_text)/base_text:>9.2f}%{100*(bl-base_blob)/base_blob:>9.2f}%"
                  f"{100*(ga-base_gas)/base_gas:>8.2f}%")

    out = HERE / "sweep.tsv"
    keys = ["arm", "module", "obj_bytes", "text_bytes", "rodata_bytes", "blob_bytes",
            "link_secs", "gas", "exports", "error", "per_export"]
    with out.open("w") as fh:
        fh.write("\t".join(keys) + "\n")
        for r in rows:
            fh.write("\t".join(str(r.get(k, "")) for k in keys) + "\n")
    print(f"\nwrote {out}")


main()
