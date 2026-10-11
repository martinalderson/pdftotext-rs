//! pdftotext-compatible CLI on top of pdf_oxide.
//! usage: pdftotext-rs [-f N] [-l N] [-layout] [-raw] [-q] [-enc X] [-nopgbrk] <file.pdf> [out.txt|-]
mod layout;
use std::io::Write;
use std::process::exit;

fn usage() -> ! {
    eprintln!("usage: pdftotext-rs [-f first] [-l last] [-layout] [-raw] [-nopgbrk] [-q] <PDF-file> [<text-file>|-]");
    exit(99)
}

fn main() {
    let mut first: usize = 1;
    let mut last: usize = usize::MAX;
    let mut nopgbrk = false;
    let mut layout = false;
    let mut pos: Vec<String> = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "-f" => first = args.next().and_then(|v| v.parse().ok()).unwrap_or_else(|| usage()),
            "-l" => last = args.next().and_then(|v| v.parse().ok()).unwrap_or_else(|| usage()),
            "-enc" | "-eol" | "-opw" | "-upw" | "-r" | "-x" | "-y" | "-W" | "-H" | "-fixed" => {
                args.next();
            },
            "-nopgbrk" => nopgbrk = true,
            "-layout" => layout = true,
            "-raw" | "-q" | "-nodiag" | "-htmlmeta" | "-bbox" | "-cropbox" => {},
            "-" => pos.push(a),
            s if s.starts_with('-') && s.len() > 1 => usage(),
            _ => pos.push(a),
        }
    }
    if pos.is_empty() || pos.len() > 2 {
        usage();
    }
    if std::env::var_os("PDFRS_FULL_PIPELINE").is_none() {
        pdf_oxide::extractors::text::LEAN_SPANS.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    match std::env::var("PDFRS_SCAN").as_deref() {
        Ok("prescan") => {},
        Ok("linear") => pdf_oxide::content::parser::NO_PRESCAN.store(true, std::sync::atomic::Ordering::Relaxed),
        _ => pdf_oxide::content::parser::LEAN_SCAN.store(true, std::sync::atomic::Ordering::Relaxed),
    }
    let input = &pos[0];
    let doc = match pdf_oxide::PdfDocument::open_mapped(input) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Error: couldn't open {}: {}", input, e);
            exit(1)
        },
    };
    let n = match doc.page_count() {
        Ok(n) => n,
        Err(e) => {
            eprintln!("Error: {}", e);
            exit(1)
        },
    };
    let first = first.max(1);
    let last = last.min(n);
    let mut out: Vec<u8> = Vec::new();
    for p in first..=last {
        let mode = std::env::var("PDFRS_MODE").unwrap_or_default();
        let r = match mode.as_str() {
            "oxide" => doc.extract_text(p - 1),
            "dump" => doc.extract_spans_raw(p - 1).map(|s| s.iter().map(|x| format!("{:7.1} {:7.1} w{:6.1} h{:5.1} fs{:5.1} rot{:4.0} seq{:4} cw{} {:?} font={}\n", x.bbox.x, x.bbox.y, x.bbox.width, x.bbox.height, x.font_size, x.rotation_degrees, x.sequence, x.char_widths.len(), x.text, x.font_name)).collect::<String>()),
            _ => doc.extract_spans_raw(p - 1).map(|mut spans| {
                // Filled-in form fields are part of what pdftotext prints.
                spans.extend(doc.extract_widget_spans(p - 1));
                let runs: Vec<layout::Run> = spans
                    .iter()
                    .map(|s| layout::Run {
                        text: &s.text,
                        x: s.bbox.x,
                        y: s.bbox.y,
                        width: s.bbox.width,
                        height: s.bbox.height,
                        font_size: s.font_size,
                        rotation: s.rotation_degrees,
                        seq: s.sequence,
                    })
                    .collect();
                let mut t = String::new();
                layout::assemble(&runs, layout, &mut t);
                t
            }),
        };
        match r {
            Ok(t) => out.extend_from_slice(t.as_bytes()),
            Err(e) => eprintln!("Error (page {}): {}", p, e),
        }
        if !nopgbrk {
            out.push(0x0c);
        }
    }
    let res = if pos.len() == 2 && pos[1] != "-" {
        std::fs::write(&pos[1], &out)
    } else if pos.len() == 2 {
        std::io::stdout().write_all(&out)
    } else {
        let mut p = std::path::PathBuf::from(input);
        p.set_extension("txt");
        std::fs::write(p, &out)
    };
    if res.is_err() {
        exit(2)
    }
}
