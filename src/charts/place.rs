use super::*;

/// A placement-over-course line, optionally with percentile bands (batch mode).
#[derive(Clone, Debug, PartialEq)]
pub struct PlaceSeries {
    pub label: String,
    pub color: String,
    /// 101 points, x = 0..=100 % of the course, y = placement (1 = first).
    pub line: Vec<f32>,
    /// Inner band (p25, p75) and outer band (p10, p90), each 101 points.
    pub inner: Option<(Vec<f32>, Vec<f32>)>,
    pub outer: Option<(Vec<f32>, Vec<f32>)>,
    /// Dashed line (e.g. the baseline when comparing against an edited run).
    pub dashed: bool,
    /// Drawn on top / thicker.
    pub highlight: bool,
}

/// Placement (y, 1 at the top .. `field_size` at the bottom) vs % of course (x). Vertical markers at
/// `markers` (x in %, label), e.g. the final-leg start. Hover shows the value under the cursor.
/// A legend is drawn under the chart for 2–4 series (more = single-run mode; the UI's name chips cover it).
#[component]
pub fn PlaceChart(series: Vec<PlaceSeries>, field_size: usize, markers: Vec<(f32, String)>) -> Element {
    let (mut size, cw) = use_width();
    const H: f32 = 320.0;
    const L: f32 = 30.0;
    const R: f32 = 16.0;
    const T: f32 = 22.0;
    const B: f32 = 26.0;
    let (pw, ph) = (cw - L - R, H - T - B);
    let n = field_size.max(2) as f32;
    let x = move |pct: f32| L + pct.clamp(0.0, 100.0) / 100.0 * pw;
    let y = move |p: f32| T + (p.clamp(1.0, n) - 1.0) / (n - 1.0) * ph;
    // Index of `pct` in a series of `len` points (normally 101 → identity).
    let at = |v: &[f32], pct: usize| v.get(pct * (v.len().max(1) - 1) / 100).copied().unwrap_or(n);
    let pts = |v: &[f32]| {
        let k = 100.0 / (v.len().max(2) - 1) as f32;
        v.iter().enumerate().map(|(i, &p)| (x(i as f32 * k), y(p))).collect::<Vec<_>>()
    };
    // Upper edge left to right, lower edge back.
    let band = |(lo, hi): &(Vec<f32>, Vec<f32>)| pts_str(&[pts(hi), pts(lo).into_iter().rev().collect()].concat());

    let mut hover = use_signal(|| None::<usize>);
    let any_hl = series.iter().any(|s| s.highlight);
    let mut order: Vec<&PlaceSeries> = series.iter().collect();
    order.sort_by_key(|s| s.highlight); // highlighted last = on top
    let dim = |s: &PlaceSeries| if any_hl && !s.highlight { 0.45 } else { 1.0 };
    // (color, inner, outer, line, width, dash, opacity)
    let drawn: Vec<_> = order.iter().map(|s| (
        s.color.clone(),
        s.inner.as_ref().map(band),
        s.outer.as_ref().map(band),
        pts_str(&pts(&s.line)),
        if s.highlight { 3.0 } else { 2.0 },
        if s.dashed { "6 4" } else { "none" },
        dim(s),
    )).collect();

    let row_labels: Vec<f32> = (1..=n as usize).filter(|p| n <= 12.0 || (p - 1) % 2 == 0).map(|p| p as f32).collect();

    // Tooltip rows at the hovered x, best placement first: (value, label, color, dashed, p25–p75).
    let tip = hover().map(|h| {
        let mut rows: Vec<_> = series.iter().map(|s| (
            at(&s.line, h), s.label.clone(), s.color.clone(), s.dashed,
            s.inner.as_ref().map(|(lo, hi)| format!("{} to {}", fmt_place(at(lo, h)), fmt_place(at(hi, h)))),
        )).collect();
        rows.sort_by(|a, b| a.0.total_cmp(&b.0));
        let longest = rows.iter().map(|r| r.1.chars().count().min(24)).max().unwrap_or(0) as f32;
        let has_band = rows.iter().any(|r| r.4.is_some());
        let tw = 62.0 + longest * 6.2 + if has_band { 52.0 } else { 0.0 };
        let gx = x(h as f32);
        let tx = if gx + 10.0 + tw < cw - R { gx + 10.0 } else { gx - 10.0 - tw };
        (h, gx, tx, tw, rows)
    });

    rsx! {
        div {
            svg { view_box: "0 0 {cw} {H}", style: SVG_STYLE, onresize: move |e| track(&mut size, e),
                {y_grid(&row_labels, y, L, cw - R, |p| format!("{p}"))}
                for p in [0.0f32, 25.0, 50.0, 75.0, 100.0] {
                    text {
                        x: x(p), y: H - 8.0, fill: "var(--muted)", font_size: 11,
                        text_anchor: if p == 0.0 { "start" } else if p == 100.0 { "end" } else { "middle" },
                        "{p}%"
                    }
                }
                for (m, label) in markers.iter() {
                    line { x1: x(*m), x2: x(*m), y1: T - 4.0, y2: T + ph, stroke: "var(--muted)", stroke_width: 1, stroke_opacity: 0.7 }
                    text {
                        x: if *m > 80.0 { x(*m) - 4.0 } else { x(*m) + 4.0 }, y: T - 8.0, fill: "var(--muted)", font_size: 11,
                        text_anchor: if *m > 80.0 { "end" } else { "start" }, "{label}"
                    }
                }
                for (c, inner, outer, _, _, _, o) in drawn.iter() {
                    if let Some(b) = outer { polygon { points: "{b}", fill: "{c}", fill_opacity: 0.10 * o } }
                    if let Some(b) = inner { polygon { points: "{b}", fill: "{c}", fill_opacity: 0.20 * o } }
                }
                for (c, _, _, l, w, dash, o) in drawn.iter() {
                    polyline {
                        points: "{l}", fill: "none", stroke: "{c}", stroke_width: *w, stroke_opacity: *o,
                        stroke_dasharray: *dash, stroke_linejoin: "round", stroke_linecap: "round"
                    }
                }
                if let Some((h, gx, tx, tw, rows)) = &tip {
                    line { x1: *gx, x2: *gx, y1: T, y2: T + ph, stroke: "var(--ink)", stroke_width: 1, stroke_opacity: 0.5 }
                    for (v, _, c, _, _) in rows.iter() {
                        circle { cx: *gx, cy: y(*v), r: 4, fill: "{c}", stroke: "var(--surface)", stroke_width: 2 }
                    }
                    g { pointer_events: "none",
                        rect {
                            x: *tx, y: T, width: *tw, height: 24.0 + rows.len() as f32 * 14.0, rx: 6,
                            fill: "var(--surface)", fill_opacity: 0.96, stroke: "var(--grid)"
                        }
                        text { x: tx + 8.0, y: T + 15.0, fill: "var(--muted)", font_size: 11, "{h}% of course" }
                        for (k, (v, label, c, dashed, range)) in rows.iter().enumerate() {
                            line {
                                x1: tx + 8.0, x2: tx + 20.0, y1: T + 29.0 + k as f32 * 14.0, y2: T + 29.0 + k as f32 * 14.0,
                                stroke: "{c}", stroke_width: 2.5, stroke_dasharray: if *dashed { "3 2" } else { "none" }
                            }
                            text { x: tx + 26.0, y: T + 33.0 + k as f32 * 14.0, fill: "var(--ink)", font_size: 11, font_weight: 600, "{fmt_place(*v)}" }
                            text { x: tx + 56.0, y: T + 33.0 + k as f32 * 14.0, fill: "var(--ink)", font_size: 11, "{truncate(label, 24)}" }
                            if let Some(r) = range {
                                text { x: tx + tw - 8.0, y: T + 33.0 + k as f32 * 14.0, fill: "var(--muted)", font_size: 11, text_anchor: "end", "{r}" }
                            }
                        }
                    }
                }
                // Hover hit strips, one per % (a crosshair that snaps; no pixel→viewBox math needed).
                g { onmouseleave: move |_| hover.set(None),
                    for i in 0..=100usize {
                        rect {
                            x: x(i as f32 - 0.5).max(L), y: T, width: pw / 100.0, height: ph, fill: "transparent",
                            onmouseenter: move |_| hover.set(Some(i))
                        }
                    }
                }
            }
            if (2..=4).contains(&series.len()) {
                Legend { items: series.iter().map(|s| (s.label.clone(), s.color.clone(), s.dashed)).collect::<Vec<_>>() }
            }
        }
    }
}
