"""Code size and compile time for the four ways of carrying a wide instruction's width.

  ref            no extension at all
  no-vsetvli     width encoded in funct7; no vtype dependency
  vsetvli-noMO   width from vtype, outliner barred from lifting vtype-dependent code
  vsetvli-MO     width from vtype, outlining allowed; the linker recovers width interprocedurally

The first three differ only in how llc is invoked, so they are measured the same way. The fourth
produces the same object code as an unrestricted build -- what separates it from the third is
linker work, measured elsewhere.
"""
import concurrent.futures, os, pathlib, re, statistics, subprocess, sys, tempfile, time

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / "vec-ext"))
import measure

HERE = pathlib.Path(__file__).resolve().parent
IR = pathlib.Path(os.environ.get("VEC_IR") or HERE.parent / "ir-intrinsics-oz")
LLC_VSETIVLI = HERE / "bin" / "llc-vsetivli"
LLC_FUNCT7 = HERE / "bin" / "llc-funct7"
SIZE = measure.TOOLS / "llvm-size"
REPEATS = int(os.environ.get("REPEATS", "3"))

# (llc, extra feature edits, extra flags)
ARMS = {
    "ref":          (LLC_VSETIVLI, [], []),
    "no-vsetvli":   (LLC_FUNCT7,   [("xrevivevec", True)], []),
    "vsetvli-noMO": (LLC_VSETIVLI, [("xrevivevec", True)], []),
    "vsetvli-MO":   (LLC_VSETIVLI, [("xrevivevec", True)],
                     ["-riscv-revive-outline-vtype=true"]),
}

def sizes(obj):
    """Sums every .text*/.rodata* section: the object is compiled with function sections, so the
    code is spread across one section per function rather than a single `.text`."""
    out = subprocess.run([str(SIZE), "-A", str(obj)], capture_output=True, text=True).stdout
    code = rodata = 0
    for row in out.splitlines():
        parts = row.split()
        if len(parts) < 2 or not parts[1].isdigit():
            continue
        if parts[0].startswith(".text"):
            code += int(parts[1])
        elif parts[0].startswith((".rodata", ".srodata")):
            rodata += int(parts[1])
    return code, rodata


def compile_one(args):
    name, source = args
    llc, edits, extra = ARMS[name]
    features = measure.BASE_FEATURES + "".join(
        ("," + ("+" if on else "-") + f) for f, on in edits)
    with tempfile.NamedTemporaryFile("w", suffix=".ll", delete=False) as fh:
        fh.write(measure.rewrite(source.read_text(), edits))
        path = fh.name
    obj = path + ".o"
    cmd = [str(llc), "-O2", f"-mattr={features}", *extra, "-filetype=obj", "-o", obj, path]
    best = None
    try:
        for _ in range(REPEATS):
            start = time.perf_counter()
            done = subprocess.run(cmd, capture_output=True, text=True, timeout=900)
            elapsed = time.perf_counter() - start
            if done.returncode:
                return name, source, None, None, None, done.stderr.strip().splitlines()[:1]
            # Fastest of N: the minimum is the one least polluted by other load on the machine.
            best = elapsed if best is None else min(best, elapsed)
    except subprocess.TimeoutExpired:
        return name, source, None, None, None, ["TIMEOUT"]
    text, rodata = sizes(obj)
    for p in (path, obj):
        try: os.unlink(p)
        except OSError: pass
    return name, source, text, rodata, best, None


def main():
    files = sorted(IR.rglob("*.optimized.ll"))
    jobs = [(name, f) for name in ARMS for f in files]
    results = {name: {} for name in ARMS}
    failed = {name: [] for name in ARMS}
    with concurrent.futures.ThreadPoolExecutor(max(1, os.cpu_count() - 2)) as ex:
        for name, source, text, rodata, secs, err in ex.map(compile_one, jobs):
            if err:
                failed[name].append((source, err))
            else:
                results[name][source] = (text, rodata, secs)

    # Only modules every arm compiled, so the columns describe the same programs.
    common = set(files)
    for name in ARMS:
        common &= set(results[name])
    common = sorted(common)
    print(f"corpus: {len(files)} modules, {len(common)} compiled by every arm")
    for name in ARMS:
        if failed[name]:
            print(f"  {name}: {len(failed[name])} failed, e.g. {failed[name][0][1]}")

    print(f"\n{'arm':<14}{'.text':>12}{'.rodata':>11}{'total':>12}"
          f"{'compile s':>11}{'vs ref':>10}{'vs no-vsetvli':>15}")
    base_text = sum(results["ref"][f][0] for f in common)
    ref_ext = sum(results["no-vsetvli"][f][0] for f in common)
    rows = {}
    for name in ARMS:
        text = sum(results[name][f][0] for f in common)
        rodata = sum(results[name][f][1] for f in common)
        secs = sum(results[name][f][2] for f in common)
        rows[name] = (text, rodata, secs)
        d_ref = f"{100*(text-base_text)/base_text:+.2f}%"
        d_ext = f"{100*(text-ref_ext)/ref_ext:+.2f}%" if name != "ref" else "-"
        print(f"{name:<14}{text:>12,}{rodata:>11,}{text+rodata:>12,}"
              f"{secs:>11.1f}{d_ref:>10}{d_ext:>15}")

    out = HERE / "codesize-compiletime.tsv"
    with out.open("w") as fh:
        fh.write("module\t" + "\t".join(f"{n}_text\t{n}_rodata\t{n}_secs" for n in ARMS) + "\n")
        for f in common:
            cells = []
            for n in ARMS:
                t, r, s = results[n][f]
                cells += [str(t), str(r), f"{s:.4f}"]
            fh.write(f.parent.name + "/" + f.name + "\t" + "\t".join(cells) + "\n")
    print(f"\nwrote {out}")


main()
