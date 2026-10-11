//! pdftotext-style physical line assembly from positioned text spans.
//!
//! Spans are bucketed by text rotation, mapped into an upright reading frame,
//! clustered into lines by vertical overlap and emitted left to right. In
//! layout mode horizontal gaps become column padding, as `pdftotext -layout`.

/// One positioned run of text, already in page space.
pub struct Run<'a> {
    pub text: &'a str,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub font_size: f32,
    pub rotation: f32,
    pub seq: usize,
}

struct Item<'a> {
    text: &'a str,
    u0: f32,
    u1: f32,
    vc: f32,
    h: f32,
    seq: usize,
    /// Whitespace-only run: contributes a word break, no text.
    ws: bool,
    /// The font gave no advance, so `u1` is an estimate.
    est: bool,
}

/// Gap (as a fraction of font size) at which two runs stop being one word.
const WORD_BREAK: f32 = 0.1;

pub fn assemble(runs: &[Run], layout: bool, out: &mut String) {
    // Bucket by rotation rounded to the degree; the largest bucket reads first.
    let mut buckets: Vec<(i32, Vec<Item>)> = Vec::new();
    for r in runs {
        if r.text.is_empty() {
            continue;
        }
        let ws = r.text.trim().is_empty();
        let deg = normalise(r.rotation);
        let (s, c) = (deg as f32).to_radians().sin_cos();
        let u = r.x * c + r.y * s;
        let v = -r.x * s + r.y * c;
        let h = if r.height > 0.0 { r.height } else { r.font_size.max(1.0) };
        // A font with no usable widths (missing resource, broken /Widths)
        // reports a zero advance; assume half an em per character.
        let est = r.width <= 0.0;
        let w = if est { 0.5 * h * r.text.chars().count() as f32 } else { r.width };
        let item = Item { text: r.text, u0: u, u1: u + w, vc: v + h * 0.5, h, seq: r.seq, ws, est };
        match buckets.iter_mut().find(|(d, _)| *d == deg) {
            Some((_, v)) => v.push(item),
            None => buckets.push((deg, vec![item])),
        }
    }
    buckets.retain(|(_, v)| v.iter().any(|i| !i.ws));
    buckets.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(&b.0)));
    let mut first = true;
    for (_, items) in buckets {
        if !first {
            out.push('\n');
        }
        first = false;
        assemble_bucket(items, layout, out);
    }
}

fn normalise(deg: f32) -> i32 {
    let d = (deg.round() as i32).rem_euclid(360);
    // Snap near-quadrant angles so a slightly skewed scan stays one bucket.
    for q in [0, 90, 180, 270, 360] {
        if (d - q).abs() <= 2 {
            return q % 360;
        }
    }
    d
}

fn assemble_bucket(mut items: Vec<Item>, layout: bool, out: &mut String) {
    if items.is_empty() {
        return;
    }
    items.sort_by(|a, b| b.vc.total_cmp(&a.vc).then(a.u0.total_cmp(&b.u0)));

    // Cluster into lines: a run joins the current line while its centre stays
    // within half a (smaller) glyph height of the line's running centre.
    let mut lines: Vec<Vec<Item>> = Vec::new();
    let (mut line_vc, mut line_h, mut n) = (0.0f32, 0.0f32, 0usize);
    for it in items {
        let same = n > 0 && (line_vc - it.vc).abs() < 0.5 * line_h.min(it.h).max(0.5);
        if same {
            line_vc = (line_vc * n as f32 + it.vc) / (n + 1) as f32;
            line_h = line_h.max(it.h);
            n += 1;
            lines.last_mut().unwrap().push(it);
        } else {
            line_vc = it.vc;
            line_h = it.h;
            n = 1;
            lines.push(vec![it]);
        }
    }

    // Column pitch for layout mode: median advance per character.
    let (mut pitch, mut left) = (0.0f32, f32::MAX);
    if layout {
        let mut ws: Vec<f32> = Vec::new();
        for l in &lines {
            for it in l {
                if it.ws {
                    continue;
                }
                let c = it.text.chars().count();
                if c > 0 && it.u1 > it.u0 && !it.est {
                    ws.push((it.u1 - it.u0) / c as f32);
                }
                left = left.min(it.u0);
            }
        }
        if !ws.is_empty() {
            let mid = ws.len() / 2;
            ws.select_nth_unstable_by(mid, |a, b| a.total_cmp(b));
            pitch = ws[mid];
        }
    }

    let mut prev: Option<(f32, f32)> = None; // (centre, height) of the previous line
    let mut buf = String::new();
    for mut l in lines {
        l.sort_by(|a, b| a.u0.total_cmp(&b.u0).then(a.seq.cmp(&b.seq)));
        let vc = l.iter().map(|i| i.vc).sum::<f32>() / l.len() as f32;
        let h = l.iter().map(|i| i.h).fold(0.0f32, f32::max);
        if let Some((pvc, ph)) = prev {
            // Keep vertical whitespace: one blank line per skipped line pitch.
            let pitch_v = ph.max(h) * 1.2;
            let skipped = (((pvc - vc) / pitch_v).round() as i32 - 1).clamp(0, if layout { 3 } else { 1 });
            for _ in 0..skipped {
                out.push('\n');
            }
        }
        prev = Some((vc, h));

        buf.clear();
        let mut cols = 0usize; // chars in buf
        let mut end = f32::MIN; // right edge of what has been emitted
        let mut last: Option<&Item> = None;
        let mut forced = false; // an explicit space glyph sits before the next run
        for it in &l {
            if it.ws {
                forced = last.is_some();
                end = end.max(it.u1);
                continue;
            }
            if let Some(p) = last {
                // Same text drawn again on top of itself (fake bold / shadow).
                if p.text == it.text && (it.u0 - p.u0).abs() < 0.1 * it.h {
                    continue;
                }
            }
            let gap = it.u0 - end;
            // With estimated advances only a gap of half an em is trustworthy.
            let limit = if last.is_some_and(|p| p.est) { 0.5 } else { WORD_BREAK };
            let word_start = last.is_none() || forced || gap > limit * it.h || gap < -0.5 * it.h;
            forced = false;
            let starts_ws = it.text.starts_with(char::is_whitespace);
            let ends_ws = buf.ends_with(char::is_whitespace);
            if last.is_some() && word_start && !starts_ws && !ends_ws {
                buf.push(' ');
                cols += 1;
            }
            // Column padding only at word starts: glyph advances inside a word
            // never match the page pitch and must not split it.
            if layout && pitch > 0.0 && word_start {
                let target = ((it.u0 - left) / pitch).round().max(0.0) as usize;
                // Cap padding so a degenerate pitch cannot explode the output.
                let target = target.min(cols + 400);
                while cols < target {
                    buf.push(' ');
                    cols += 1;
                }
            }
            buf.push_str(it.text);
            cols += it.text.chars().count();
            end = end.max(it.u1);
            last = Some(it);
        }
        out.push_str(buf.trim_end());
        out.push('\n');
    }
}
