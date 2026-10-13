//! Pure SVG chart components (no engine types, only plain data).
//!
//! Theme colours come from CSS custom properties: --surface, --ink, --muted, --grid, --accent.
//! Charts measure their container (onresize) and draw 1:1 at its width with a fixed height.

use dioxus::prelude::*;

mod course;
mod dist;
mod place;
mod race;

pub use course::*;
pub use dist::*;
pub use place::*;
pub use race::*;

// 18 hues (OKLCH L 0.55–0.66) validated with dataviz validate_palette.js on both the light cream (#fffaf3) and dark
// plum (#1e1a2e) surfaces: adjacent-pair CVD ΔE >= 9.2, normal-vision ΔE >= 15.9. Gate order = slot order.
// ponytail: 18 hues can't all be pairwise distinct; highlight + tooltip carry identity past adjacent gates.
const PALETTE: [&str; 18] = [
    "#2a78d6", "#e76530", "#14ac77", "#ca8000", "#d56a93", "#0e880c", "#6756dc", "#e34948", "#3d6ebf",
    "#a0680c", "#02a3c0", "#a540a3", "#7a9a1e", "#c71f5f", "#de6c48", "#5166ca", "#00897b", "#b5651d",
];

/// Stable per-horse colour (gate index), readable on the app background. Also used by the UI for name chips.
pub fn horse_color(i: usize) -> &'static str {
    PALETTE[i % PALETTE.len()]
}

/// Rendered width in px (ResizeObserver), so charts draw 1:1 and text stays 11px at any container width.
fn use_width() -> (Signal<f32>, f32) {
    let size = use_signal(|| 640.0f32);
    let w = size().max(240.0);
    (size, w)
}

fn track(size: &mut Signal<f32>, e: Event<ResizeData>) {
    if let Ok(s) = e.get_content_box_size() && (s.width as f32 - size()).abs() > 0.5 {
        size.set(s.width as f32)
    }
}

const SVG_STYLE: &str = "display:block;width:100%;height:auto;font-variant-numeric:tabular-nums;user-select:none";

/// 1/2/5 × 10^k step giving about `n` intervals over `range`.
fn nice_step(range: f32, n: usize) -> f32 {
    // At least one interval: a chart in a hidden pane measures 0 px wide and asks for none.
    let raw = (range / n.max(1) as f32).max(1e-6);
    let mag = 10f32.powf(raw.log10().floor());
    mag * [1.0, 2.0, 5.0, 10.0].into_iter().find(|&f| raw / mag <= f + 1e-4).unwrap()
}

/// Domain widened to whole steps, plus the ticks inside it.
fn nice_domain(lo: f32, hi: f32, n: usize) -> (f32, f32, f32, Vec<f32>) {
    let hi = if hi - lo < 1e-6 { lo + 1.0 } else { hi };
    let step = nice_step(hi - lo, n);
    let (lo, hi) = ((lo / step + 1e-4).floor() * step, (hi / step - 1e-4).ceil() * step);
    let i0 = (lo / step).round() as i32;
    let ticks = (i0..=(hi / step).round() as i32).map(|k| k as f32 * step).collect();
    (lo, hi, step, ticks)
}

/// Tick label with just enough decimals for `step`; `pct` treats values as fractions.
fn fmt_tick(v: f32, step: f32, pct: bool) -> String {
    let (v, step) = if pct { (v * 100.0, step * 100.0) } else { (v, step) };
    let d = (-step.log10().floor()).clamp(0.0, 3.0) as usize;
    let s = format!("{:.*}", d, if v.abs() < step * 1e-3 { 0.0 } else { v });
    if pct { s + "%" } else { s }
}

fn fmt_pct(v: f32) -> String {
    format!("{:.0}%", v * 100.0)
}

/// Integer when whole, else one decimal (placements).
fn fmt_place(v: f32) -> String {
    if (v - v.round()).abs() < 0.05 { format!("{}", v.round()) } else { format!("{v:.1}") }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max { s.into() } else { s.chars().take(max.saturating_sub(1)).collect::<String>() + "…" }
}

/// Column from baseline `y0` to `y1` (either direction): 4px rounded data end, square at the baseline.
fn bar_path(x: f32, w: f32, y0: f32, y1: f32) -> String {
    let r = 4f32.min(w / 2.0).min((y1 - y0).abs());
    let s = if y1 < y0 { 1.0 } else { -1.0 }; // rounding bends back toward the baseline
    format!(
        "M{x},{y0}V{a}Q{x},{y1} {b},{y1}H{c}Q{d},{y1} {d},{a}V{y0}Z",
        a = y1 + s * r, b = x + r, c = x + w - r, d = x + w
    )
}

/// Horizontal gridlines + left labels for a value axis.
fn y_grid(ticks: &[f32], y: impl Fn(f32) -> f32, x0: f32, x1: f32, label: impl Fn(f32) -> String) -> Element {
    rsx! {
        for &t in ticks {
            line { x1: x0, x2: x1, y1: y(t), y2: y(t), stroke: "var(--grid)", stroke_width: 1 }
            text { x: x0 - 6.0, y: y(t), fill: "var(--muted)", font_size: 11, text_anchor: "end", dominant_baseline: "middle", "{label(t)}" }
        }
    }
}

/// Bottom axis labels at `ticks`; right-aligned past `right` so the last one stays inside.
fn x_labels(ticks: &[f32], x: impl Fn(f32) -> f32, y: f32, right: f32, label: impl Fn(f32) -> String) -> Element {
    rsx! {
        for &t in ticks {
            text { x: x(t), y, fill: "var(--muted)", font_size: 11, text_anchor: if x(t) > right { "end" } else { "middle" }, "{label(t)}" }
        }
    }
}

/// Count axis for histograms: (top, step, whole-number ticks).
fn count_axis(max: usize) -> (f32, f32, Vec<f32>) {
    let (_, top, step, ticks) = nice_domain(0.0, (max as f32).max(1.0), 4);
    (top, step.max(1.0), ticks.into_iter().filter(|t| t.fract() == 0.0).collect())
}

/// `nb` bins of width `bw` from `start`; values outside land in the end bins.
fn bin_counts(vals: impl IntoIterator<Item = f32>, start: f32, bw: f32, nb: usize) -> Vec<usize> {
    let mut counts = vec![0; nb];
    for v in vals {
        counts[(((v - start) / bw).floor() as usize).min(nb - 1)] += 1; // `as` saturates negatives to 0
    }
    counts
}


/// Top-right readout line used for hover values.
fn readout(x: f32, y: f32, s: String) -> Element {
    // Right-aligned at `x`: keep it inside the chart (~6.3 px per char at 11 px).
    let s = truncate(&s, ((x - 8.0) / 6.3).max(8.0) as usize);
    rsx! {
        text {
            x, y, fill: "var(--ink)", font_size: 11, font_weight: 600, text_anchor: "end",
            stroke: "var(--surface)", stroke_width: 3, paint_order: "stroke", "{s}"
        }
    }
}

/// Line-key legend under a chart: (label, colour, dashed).
#[component]
fn Legend(items: Vec<(String, String, bool)>) -> Element {
    rsx! {
        div { style: "display:flex;flex-wrap:wrap;gap:4px 16px;padding:4px 0 0 30px;font-size:12px;color:var(--ink)",
            for (label, c, dashed) in items {
                span { style: "display:inline-flex;align-items:center;gap:6px",
                    svg { width: 18, height: 8,
                        line { x1: 1, x2: 17, y1: 4, y2: 4, stroke: "{c}", stroke_width: 2.5, stroke_linecap: "round", stroke_dasharray: if dashed { "4 3" } else { "none" } }
                    }
                    "{label}"
                }
            }
        }
    }
}

/// Smooth SVG path through `p` (x monotonic, either direction): monotone cubic (Fritsch-Carlson), so it never
/// overshoots past the points (HP stays >= 0, place >= 1). `join` continues a path (L) instead of starting one (M).
fn curve(p: &[(f32, f32)], join: bool) -> String {
    let Some(&(x0, y0)) = p.first() else { return String::new() };
    let mut s = format!("{}{x0:.1},{y0:.1}", if join { 'L' } else { 'M' });
    if p.len() < 2 {
        return s;
    }
    let m: Vec<f32> = p.windows(2).map(|w| (w[1].1 - w[0].1) / (w[1].0 - w[0].0)).collect();
    let mut t: Vec<f32> = (0..p.len())
        .map(|i| match i {
            0 => m[0],
            i if i == m.len() => m[i - 1],
            i if m[i - 1] * m[i] <= 0.0 => 0.0,
            i => (m[i - 1] + m[i]) / 2.0,
        })
        .collect();
    for (i, &mi) in m.iter().enumerate() {
        if mi == 0.0 {
            (t[i], t[i + 1]) = (0.0, 0.0);
        } else {
            let (a, b) = (t[i] / mi, t[i + 1] / mi);
            if a * a + b * b > 9.0 {
                let k = 3.0 / (a * a + b * b).sqrt();
                (t[i], t[i + 1]) = (k * a * mi, k * b * mi);
            }
        }
    }
    for (i, w) in p.windows(2).enumerate() {
        let ((xa, ya), (xb, yb)) = (w[0], w[1]);
        let h = (xb - xa) / 3.0;
        s += &format!("C{:.1},{:.1} {:.1},{:.1} {xb:.1},{yb:.1}", xa + h, ya + t[i] * h, xb - h, yb - t[i + 1] * h);
    }
    s
}

fn pts_str(v: &[(f32, f32)]) -> String {
    v.iter().map(|(x, y)| format!("{x:.1},{y:.1}")).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scales() {
        assert_eq!(nice_step(2400.0, 6), 500.0);
        assert!((nice_domain(0.0, 0.37, 4).3.last().unwrap() - 0.4).abs() < 1e-6);
        assert_eq!(nice_domain(0.0, 0.37, 4).3[0], 0.0); // no tick below the baseline
        assert_eq!(fmt_tick(0.25, 0.05, true), "25%");
        assert_eq!(fmt_tick(-0.5, 0.5, false), "-0.5");
        assert_eq!(fmt_place(3.0), "3");
        assert!(bar_path(0.0, 10.0, 100.0, 50.0).starts_with("M0,100V54"));
        assert_eq!(bin_counts([-1.0, 0.0, 0.5, 1.0, 9.0], 0.0, 1.0, 2), [3, 2]); // out of range clamps to the end bins
        assert_eq!(phase_bounds(&[0.0, 400.0, 2600.0], 2400.0), [0.0, 400.0, 2400.0]);
        assert_eq!(dist_ticks(1700.0, 2400.0, 700.0)[0], 1700.0);
        let e = elevation_or_slopes(&[], &[(100.0, 200.0, 2.0)], 300.0);
        assert!((e.last().unwrap() - 2.0).abs() < 1e-3); // 100 m at +2% = +2 m
        // Flat runs stay flat: a step's control points don't bulge past either level.
        assert_eq!(curve(&[(0.0, 5.0), (10.0, 5.0), (20.0, 0.0)], false), "M0.0,5.0C3.3,5.0 6.7,5.0 10.0,5.0C13.3,5.0 16.7,1.7 20.0,0.0");
    }
}
