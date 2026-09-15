"""ref + w only remeasure (NewYork corpus). Hung llc fails once at TIMEOUT (not best-of-3)."""
import os, pathlib, re, subprocess, tempfile, time
HERE = pathlib.Path(__file__).resolve().parent
BIN = HERE / "bin"
IR = pathlib.Path(os.environ.get("VEC_IR") or HERE.parents[1] / "ir-corpus")
TIMEOUT = int(os.environ.get("TIMEOUT", "90"))
BEST_OF = 3
BASE = "+e,+m,+a,+c,+zbb,+auipc-addi-fusion,+ld-add-fusion,+lui-addi-fusion,+xtheadcondmov,+relax"
LLC = pathlib.Path("/home/ubuntu/workspaces/parity-llvm/build/bin/llc")
SIZE = BIN / "llvm-size"
LINK = pathlib.Path("/home/ubuntu/workspaces/polkavm/target/release/polkatool")
RUNBLOB = BIN / "runblob"
ATTR = re.compile(r'"target-features"="[^"]*"')
EXPORT = re.compile(r'^  (\S+): (.*?) gas=(-?\d+) steps=(\d+)(?: time_ns=(\d+))?$', re.M)
ITERS = int(os.environ.get("RUNBLOB_ITERS", "50"))
ARMS = {"ref": dict(ext=False, feat="", w=False), "w": dict(ext=True, feat=",+xrevivew", w=True)}
def ir_for(src, feat):
    fh = tempfile.NamedTemporaryFile("w", suffix=".ll", delete=False)
    fh.write(ATTR.sub(f'"target-features"="{BASE}{feat}"', src.read_text())); fh.close(); return fh.name
def sizes(obj):
    out = subprocess.run([str(SIZE), "-A", obj], capture_output=True, text=True).stdout; t=0
    for row in out.splitlines():
        p = row.split()
        if len(p)>=2 and p[1].isdigit() and p[0].startswith(".text"): t+=int(p[1])
    return t
def best(fn):
    b=float("inf")
    for _ in range(BEST_OF):
        v=fn()
        if v is None: return None
        b=min(b,v)
    return b
def profile(src, arm):
    spec=ARMS[arm]; r=dict(module=src.name, arm=arm); path=ir_for(src, spec["feat"])
    obj, blob = path+".o", path+".polkavm"
    llc_cmd=[str(LLC),"-O2",f"-mattr={BASE}{spec['feat']}","-filetype=obj","-o",obj,path]
    try:
        def do_compile():
            s=time.perf_counter(); x=subprocess.run(llc_cmd,capture_output=True,timeout=TIMEOUT)
            return None if x.returncode else time.perf_counter()-s
        r["compile_s"]=best(do_compile)
        if r["compile_s"] is None: r["error"]="compile"; return r
        r["text"]=sizes(obj)
        isa="revive_v2" if spec["ext"] else "revive_v1"; env={**os.environ}
        if spec["w"]: env["POLKAVM_ASSUME_W256"]="1"
        x=subprocess.run([str(LINK),"link","--isa",isa,"-o",blob,obj],capture_output=True,timeout=TIMEOUT,env=env)
        if x.returncode: r["error"]="link"; return r
        r["blob"]=os.path.getsize(blob)
        for backend,key in (("interpreter","interp_ns"),("compiler","recomp_ns")):
            x=subprocess.run([str(RUNBLOB),blob],capture_output=True,text=True,timeout=TIMEOUT,
                             env={**env,"RUNBLOB_BACKEND":backend,"RUNBLOB_ITERS":str(ITERS)})
            if x.returncode: r[key]=None; continue
            found=EXPORT.findall(x.stdout)
            r[key]=sum(int(t) for *_,t in found if t) or None
            if backend=="interpreter":
                r["gas"]=sum(int(g) for _,_,g,_,_ in found); r["exports"]=len(found)
        r["ran"]=r.get("interp_ns") is not None and r.get("recomp_ns") is not None
    except subprocess.TimeoutExpired: r["error"]="timeout"
    finally:
        for p in (path,obj,blob):
            try: os.unlink(p)
            except OSError: pass
    return r
files=sorted(IR.rglob("*.optimized.ll"))
print(f"corpus: {len(files)} modules; arms {list(ARMS)}; timeout {TIMEOUT}s",flush=True)
rows=[]
for i,src in enumerate(files,1):
    for arm in ARMS: rows.append(profile(src,arm))
    if i%10==0: print(f"  ... {i}/{len(files)}",flush=True)
keys=["module","arm","compile_s","text","blob","gas","interp_ns","recomp_ns","exports","error"]
tsv=HERE/"per-bench-wreg-nyw.tsv"
with tsv.open("w") as fh:
    fh.write("\t".join(keys)+"\n")
    for r in rows: fh.write("\t".join("" if r.get(k) is None else str(r.get(k,"")) for k in keys)+"\n")
print(f"wrote {tsv}",flush=True)
by={}
for r in rows: by.setdefault(r["module"],{})[r["arm"]]=r
ran=[m for m,d in by.items() if d.get("w",{}).get("ran") and d.get("ref",{}).get("ran")]
gw=sum(by[m]["w"]["gas"] for m in ran); gr=sum(by[m]["ref"]["gas"] for m in ran)
print(f"\n{len(ran)} modules ran in BOTH ref and w; gas w={gw} ref={gr} w/ref={gw/gr:.3f}x",flush=True)
