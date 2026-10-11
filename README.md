# pdftotext-rs

A drop-in replacement for `pdftotext -layout` that uses roughly a tenth of the
CPU. It is a single static Rust binary built on a patched copy of
[pdf_oxide](https://github.com/yfedoseev/pdf_oxide).

It was written for a pipeline that extracts text from tens of thousands of
PDFs, one process per file, on a small server where CPU was the bottleneck.

```
pdftotext-rs -l 2 -layout file.pdf -
```

## Results

CPU time per file (user + system), one process per PDF, best of three runs,
on 403 PDFs from UK planning applications: reports, forms and a lot of
vector-heavy drawings.

| Step | `pdftotext` 26.01 | `pdftotext-rs` | Ratio |
|---|---|---|---|
| First 2 pages (`-l 2 -layout`) | 49.9 ms | 4.5 ms | 11x |
| Full text (`-layout`) | 81.3 ms | 10.6 ms | 8x |

Quality was scored as word recall against the words that at least two of
`pdftotext`, MuPDF and PDFium agree on, because no single engine is a clean
ground truth (each one garbles some files).

| Sample | `pdftotext-rs` | `pdftotext` | MuPDF | PDFium |
|---|---|---|---|---|
| 403 files used while developing | 99.7% | n/a | n/a | n/a |
| 700 unseen files | 99.86% | 99.83% | 99.88% | 99.88% |

On the unseen sample each reference engine was scored against the consensus
of the other three, so the four numbers are comparable. There were no
crashes, hangs or errors across the 700 files.

These numbers come from one corpus on one machine. Expect the ratio to move
on other kinds of document; text-only PDFs gain less than drawings do.

## Where the time went

Stock `pdf_oxide` was no faster than PDFium on this corpus and recovered
about 92% of the words `pdftotext` did. Profiling showed why, and each fix
below was checked against the harness before the next one.

- **Skip the post-processing.** `pdf_oxide`'s `extract_text` runs the content
  stream several times and applies reading-order, de-duplication and
  artefact heuristics. Those cost most of the CPU and were also where text
  went missing (rotated text had its glyph positions snapped together).
  `pdftotext-rs` takes the raw positioned runs and assembles lines itself
  (`src/layout.rs`), the way `pdftotext -layout` does.
- **One linear pass over the content stream.** A new scanner
  (`parse_and_execute_lean`) locates operands without parsing them and only
  builds an operator when it can affect text. Path, clip, colour and
  line-style operators cost a table lookup per byte, and `q … Q` groups that
  draw only paths are dropped whole. Its output was byte-identical to the
  previous path on all 403 files.
- **Memory-map the input.** The file used to be read into the heap and then
  cloned. Two pages of a 40 MB file now touch only the pages they need.
- **Decode each font program once.** Type 0 fonts had their embedded
  TrueType program inflated twice.
- **No regexes at start-up.** ToUnicode CMap parsing compiled six regexes per
  process. They are replaced by hand-written matchers, pinned to the original
  patterns by a differential test over 20,000 generated strings.
- **Static, non-relocatable binary.** Dynamic loading and relocation were a
  visible share of a 5 ms process.
- **A quadratic slowdown on tagged documents.** Resolving marked-content
  properties cloned the page's whole resource dictionary each time; one file
  went from 7.0 s to 0.6 s.

Two correctness changes came out of the same work: form-field values are
included (as `pdftotext` does), and runs are never joined across a text
repositioning operator, so word breaks are decided from geometry.

## Limitations

- **Not byte-identical to `pdftotext`.** Words and line order match; column
  padding differs. Anything that depends on exact whitespace will see
  different input.
- **Only `-layout` is emulated properly.** Without it you get the same lines
  with single spaces, not `pdftotext`'s reading-order mode.
- **Supported options:** `-f`, `-l`, `-layout`, `-nopgbrk`. `-raw`, `-q`,
  `-enc`, `-eol` and a few others are accepted and ignored. Passwords,
  cropping and `-bbox` are not implemented.
- **Glyphs with no usable Unicode mapping are dropped** where `pdftotext`
  prints U+FFFD.
- **Memory can be higher on very large files.** Median peak RSS was 5 MB
  against 19 MB for `pdftotext`, but the largest document peaked at 142 MB
  against 33 MB.
- **Span colours are not tracked**, since nothing here uses them.

## Building

Needs Rust 1.88 or newer and, for the static link, `glibc-static`.

```
cargo build --profile dist
target/x86_64-unknown-linux-gnu/dist/pdftotext-rs -layout file.pdf out.txt
```

`cargo build --release` gives a faster build with debug info for profiling.
The binary has no runtime dependencies, so it can be copied to any x86-64
Linux machine.

## The pdf_oxide patch

`vendor/pdf_oxide` is pdf_oxide 0.3.78 (commit `ad49c4c`) with local changes.
The full diff is in `patches/pdf_oxide-0.3.78.patch`. The new behaviour is
opt-in through two statics, `LEAN_SPANS` and `LEAN_SCAN`, so the library's
existing API behaves as before unless a caller sets them. `Cargo.lock` is
pinned to upstream's, because a newer `office_oxide` breaks the build.

## Benchmarking

The harness expects a directory of PDFs and reference outputs from the three
other engines.

```
export PDFBENCH=/tmp/pdfbench         # PDFs go in $PDFBENCH/s
for e in pdftotext pymupdf pdfium; do
  for m in head full; do python3 bench/reference.py $e $m $PDFBENCH/s $PDFBENCH/out; done
done
python3 bench/run.py mylabel          # CPU per file, recall, precision, word-order score
bench/stat.sh target/x86_64-unknown-linux-gnu/dist/pdftotext-rs   # instruction counts, best-of-3 CPU
```

`bench/reference.py` needs `pymupdf` and `pypdfium2`. Pass `--prev <label>`
to `run.py` to list the files whose output changed since an earlier run.

Debug switches: `PDFRS_MODE=dump` prints raw runs with their geometry,
`PDFRS_MODE=oxide` uses pdf_oxide's own `extract_text`, and
`PDFRS_SCAN=linear|prescan` selects pdf_oxide's original scanners.

## Licence

MIT or Apache-2.0, at your option, the same as pdf_oxide. The vendored
library keeps its own licence files and notices under `vendor/pdf_oxide`.
