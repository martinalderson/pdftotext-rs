#!/usr/bin/env python3
"""Speed + quality harness.  usage: run.py <label> [--bin PATH] [--src DIR] [--ref DIR] [--modes head,full]
Runs the binary once per PDF (like fetch.py does), records CPU ms, compares words to pdftotext reference
and, if a previous label is given with --prev, reports files whose output changed."""
import argparse, glob, json, os, re, resource, statistics as st, subprocess, sys, time
ap = argparse.ArgumentParser()
ap.add_argument("label"); ap.add_argument("--bin", default=os.path.dirname(__file__) + "/../target/x86_64-unknown-linux-gnu/dist/pdftotext-rs")
ap.add_argument("--src", default="/tmp/pdfbench/s"); ap.add_argument("--ref", default="/tmp/pdfbench/out")
ap.add_argument("--out", default="/tmp/pdfbench/runs"); ap.add_argument("--modes", default="head,full"); ap.add_argument("--prev")
a = ap.parse_args()
W = re.compile(r"[A-Za-z]{3,}|\d{2,}")
words = lambda s: set(w.lower() for w in W.findall(s))
def bigrams(s):
    w = [x.lower() for x in W.findall(s)]; return set(zip(w, w[1:]))
files = sorted(glob.glob(a.src + "/*.pdf"))
def cpu():
    r = resource.getrusage(resource.RUSAGE_CHILDREN); return r.ru_utime + r.ru_stime
for mode in a.modes.split(","):
    out = f"{a.out}/{a.label}-{mode}"; os.makedirs(out, exist_ok=True)
    rows = []; rec = []; prec = []; big = []; per = []; empty = 0; fails = 0; changed = []
    for f in files:
        name = os.path.basename(f); c0 = cpu(); t0 = time.perf_counter()
        p = subprocess.run([a.bin] + (["-l", "2"] if mode == "head" else []) + ["-layout", f, "-"], capture_output=True)
        ms = (cpu() - c0) * 1000; text = p.stdout.decode("utf-8", "replace")
        if p.returncode != 0: fails += 1
        rows.append((name, ms)); open(f"{out}/{name}.txt", "w").write(text)
        rd = lambda e: open(f"{a.ref}/{e}-{mode}/{name}.txt", errors="replace").read()
        reftext = rd("pdftotext"); w = words(text)
        # truth = words at least two of the three reference engines agree on
        e3 = [words(reftext), words(rd("pymupdf")), words(rd("pdfium"))]
        ref = (e3[0] & e3[1]) | (e3[0] & e3[2]) | (e3[1] & e3[2]); anyref = e3[0] | e3[1] | e3[2]
        if len(ref) >= 20:
            rec.append(len(w & ref) / len(ref)); empty += not w; per.append((rec[-1], name, sorted(ref - w)[:14], sorted(w - anyref)[:14]))
            if w: prec.append(len(w & anyref) / len(w))
            rb = bigrams(reftext)
            if rb: big.append(len(rb & bigrams(text)) / len(rb))
        if a.prev:
            try:
                if open(f"{a.out}/{a.prev}-{mode}/{name}.txt").read() != text: changed.append(name)
            except FileNotFoundError: pass
    json.dump(rows, open(out + ".json", "w"))
    c = sorted(m for _, m in rows); tot = sum(c); rec.sort()
    print(f"[{a.label} {mode}] cpu={tot/1000:6.2f}s  {tot/len(c):6.1f} ms/file  median={c[len(c)//2]:5.1f} p90={c[int(len(c)*.9)]:6.1f} max={c[-1]:7.1f} | "
          f"recall mean={st.mean(rec):.4f} p10={rec[len(rec)//10]:.3f} <0.9={sum(r<0.9 for r in rec)} empty={empty} precision={st.mean(prec):.4f} bigram={st.mean(big):.4f} fails={fails}"
          + (f" | changed vs {a.prev}: {len(changed)}" if a.prev else ""), flush=True)
    per.sort(); json.dump(per[:40], open(out + ".worst.json", "w"), indent=0)
    if a.prev and changed: open(f"{out}.changed", "w").write("\n".join(changed))
