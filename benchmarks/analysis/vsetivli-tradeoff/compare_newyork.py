"""Compare the ref/vec/w benchmark measurements on the Yul-path IR corpus vs the NewYork IR corpus.

The two inputs are the per-module TSVs written by measure_wreg.py against the two corpora
(per-bench-wreg-yul.tsv and per-bench-wreg-newyork.tsv). NewYork does type inference that narrows
i256 down to i64/i128 where provable, so its blobs should carry fewer wide ops -- this quantifies the
whole-corpus effect on code size, gas, and execution, per arm, plus coverage (NewYork compiles a
smaller slice; see doc §9).
"""
import csv, statistics as st, sys

ARMS = ("ref", "vec", "w")


def load(path):
    by = {}
    for r in csv.DictReader(open(path), delimiter="\t"):
        by.setdefault(r["module"], {})[r["arm"]] = r
    return by


def num(x):
    try:
        return float(x)
    except (TypeError, ValueError):
        return None


def ran(row):
    return row is not None and num(row.get("interp_ns")) is not None and num(row.get("recomp_ns")) is not None


def coverage(by, label):
    comp = {a: 0 for a in ARMS}
    linked = {a: 0 for a in ARMS}
    for m, d in by.items():
        for a in ARMS:
            row = d.get(a)
            if row and num(row.get("text")) is not None:
                comp[a] += 1
            if ran(row):
                linked[a] += 1
    print(f"[{label}] modules={len(by)}  llc-compiled(text) {comp}  linked+ran {linked}")
    return comp, linked


def totals(by, metric, arms):
    """Sum `metric` over modules where every arm in `arms` has it."""
    tot = {a: 0.0 for a in arms}
    n = 0
    for m, d in by.items():
        if all(a in d and num(d[a].get(metric)) is not None for a in arms):
            for a in arms:
                tot[a] += num(d[a][metric])
            n += 1
    return tot, n


def med_ratio(by, metric, a, b):
    rs = []
    for m, d in by.items():
        if a in d and b in d:
            x, y = num(d[a].get(metric)), num(d[b].get(metric))
            if x and y and x > 0 and y > 0:
                rs.append(x / y)
    return (st.median(rs), len(rs)) if rs else (None, 0)


def main():
    yul = load("per-bench-wreg-yul.tsv")
    ny = load("per-bench-wreg-newyork.tsv")

    print("=== coverage ===")
    coverage(yul, "yul")
    coverage(ny, "newyork")

    for label, by in (("yul", yul), ("newyork", ny)):
        print(f"\n=== {label}: deterministic totals ===")
        for metric in ("text", "blob", "gas"):
            tot, n = totals(by, metric, ARMS)
            base = tot["ref"] or 1
            print(f"  {metric:5} n={n:3}  " + "  ".join(f"{a}={tot[a]:.0f}({tot[a]/base:.3f}x)" for a in ARMS))
        print(f"  wall-time median ratios:")
        for metric in ("interp_ns", "recomp_ns"):
            parts = []
            for pair in (("vec", "ref"), ("w", "ref"), ("w", "vec")):
                r, k = med_ratio(by, metric, *pair)
                parts.append(f"{pair[0]}/{pair[1]}={r:.3f}(n={k})" if r else f"{pair[0]}/{pair[1]}=NA")
            print(f"    {metric:10} " + "  ".join(parts))

    # Head-to-head on the SAME modules that both corpora compiled+ran (vec arm), to isolate the IR change.
    common = [m for m in yul if m in ny and ran(yul[m].get("vec")) and ran(ny[m].get("vec"))]
    print(f"\n=== vec arm: NewYork vs Yul on the {len(common)} modules both compiled+ran ===")
    for metric in ("text", "blob", "gas", "interp_ns", "recomp_ns"):
        sy = sum(num(yul[m]["vec"][metric]) for m in common if num(yul[m]["vec"].get(metric)) is not None)
        sn = sum(num(ny[m]["vec"][metric]) for m in common if num(ny[m]["vec"].get(metric)) is not None)
        print(f"  {metric:10} yul={sy:.0f}  newyork={sn:.0f}  ny/yul={sn/sy:.3f}x" if sy else f"  {metric}: no data")


main()
