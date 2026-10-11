# usage: bench.py <engine> <mode: head|full> <srcdir> <outroot>
import sys, os, time, glob, json, resource, subprocess, site
sys.path.append(site.getusersitepackages())
engine, mode, src, outroot = sys.argv[1:5]
out = f"{outroot}/{engine}-{mode}"; os.makedirs(out, exist_ok=True)
files = sorted(glob.glob(src + "/*.pdf"))
def cpu():
    a = resource.getrusage(resource.RUSAGE_SELF); b = resource.getrusage(resource.RUSAGE_CHILDREN)
    return a.ru_utime + a.ru_stime + b.ru_utime + b.ru_stime
if engine == "pdftotext":
    def run(f):
        a = ["pdftotext"] + (["-l", "2"] if mode == "head" else []) + ["-layout", f, "-"]
        return subprocess.run(a, capture_output=True, timeout=300).stdout.decode("utf-8", "replace")
elif engine == "pymupdf":
    import pymupdf
    pymupdf.TOOLS.mupdf_display_errors(False)
    def run(f):
        with pymupdf.open(f) as d:
            n = min(2, len(d)) if mode == "head" else len(d)
            return "\n".join(d[i].get_text() for i in range(n))
elif engine == "pdfium":
    import pypdfium2
    def run(f):
        d = pypdfium2.PdfDocument(f); o = []
        n = min(2, len(d)) if mode == "head" else len(d)
        for i in range(n):
            p = d[i]; tp = p.get_textpage(); o.append(tp.get_text_range()); tp.close(); p.close()
        d.close(); return "\n".join(o)
elif engine == "pdf_oxide":
    import pdf_oxide
    try: pdf_oxide.disable_logging()
    except Exception: pass
    def run(f):
        d = pdf_oxide.PdfDocument(f); pc = int(d.page_count)
        n = min(2, pc) if mode == "head" else pc
        return "\n".join(d.extract_text(i) for i in range(n))
rows = []
for f in files:
    name = os.path.basename(f); c0 = cpu(); t0 = time.perf_counter(); status = "ok"
    try: text = run(f)
    except BaseException as ex:
        if isinstance(ex, KeyboardInterrupt): raise
        text = ""; status = type(ex).__name__
    rows.append({"name": name, "ms": (time.perf_counter() - t0) * 1000, "cpu_ms": (cpu() - c0) * 1000, "chars": len(text), "status": status})
    open(f"{out}/{name}.txt", "w").write(text)
json.dump(rows, open(f"{out}.json", "w"))
tot = sum(r["cpu_ms"] for r in rows); wall = sum(r["ms"] for r in rows)
print(f"{engine:10s} {mode:4s} files={len(rows)} cpu={tot/1000:7.2f}s wall={wall/1000:7.2f}s cpu/file={tot/len(rows):6.1f}ms fails={sum(r['status']!='ok' for r in rows)} peakRSS={resource.getrusage(resource.RUSAGE_SELF).ru_maxrss//1024}MB", flush=True)
