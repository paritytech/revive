"""3-way per-benchmark table: ref vs set_width-DISABLED (flat) vs set_width-ENABLED (width-aware).
Inputs: per-bench-wreg-recalibrated.tsv (flat) and per-bench-wreg-setwidth.tsv (width-proportional)."""
import csv, sys
def load(p):
    by={}
    for r in csv.DictReader(open(p),delimiter="\t"): by.setdefault(r["module"],{})[r["arm"]]=r
    return by
def num(x):
    try: return float(x)
    except: return None
short=lambda m: m.replace("crates_integration_contracts_","").replace(".sol","").replace(".optimized.ll","")
flat=load(sys.argv[1]); sw=load(sys.argv[2])
mods=[m for m in flat if m in sw
      and num(flat[m].get("ref",{}).get("gas")) is not None
      and num(flat[m].get("vec",{}).get("gas")) is not None
      and num(sw[m].get("vec",{}).get("gas")) is not None]
mods.sort(key=lambda m: num(flat[m]["vec"]["gas"])/num(flat[m]["ref"]["gas"]), reverse=True)
print("| benchmark | gas ref | gas sw-off | gas sw-on | sw-on/ref | sw-on/sw-off | blob off | blob on | recomp | interp |")
print("|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|")
T={"rg":0,"og":0,"ng":0,"ob":0,"nb":0}
for m in mods:
    rg=num(flat[m]["ref"]["gas"]); og=num(flat[m]["vec"]["gas"]); ng=num(sw[m]["vec"]["gas"])
    ob=num(flat[m]["vec"]["blob"]); nb=num(sw[m]["vec"]["blob"])
    rb=num(flat[m]["ref"]["blob"])
    rc=num(sw[m]["vec"]["recomp_ns"]); rrc=num(flat[m]["ref"]["recomp_ns"])
    it=num(sw[m]["vec"]["interp_ns"]); rit=num(flat[m]["ref"]["interp_ns"])
    T["rg"]+=rg;T["og"]+=og;T["ng"]+=ng;T["ob"]+=ob;T["nb"]+=nb
    print(f"| {short(m)} | {rg:.0f} | {og:.0f} | {ng:.0f} | {ng/rg:.3f}× | {ng/og:.3f}× "
          f"| {ob:.0f} | {nb:.0f} | {rc/rrc:.2f}× | {it/rit:.2f}× |")
print(f"| **TOTAL ({len(mods)})** | **{T['rg']:.0f}** | **{T['og']:.0f}** | **{T['ng']:.0f}** "
      f"| **{T['ng']/T['rg']:.3f}×** | **{T['ng']/T['og']:.3f}×** | **{T['ob']:.0f}** | **{T['nb']:.0f}** | | |")
