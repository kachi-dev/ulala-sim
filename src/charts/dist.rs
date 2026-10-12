use super::*;

/// Vertical bars. `bars` = (x label, fraction), labelled as percentages (0.37 → 37%).
/// `compare` = optional second series drawn as an outline over the first (baseline vs edited).
#[component]
pub fn BarChart(bars: Vec<(String, f32)>, compare: Option<Vec<f32>>, color: String) -> Element {
    let (mut size, cw) = use_width();
    const H: f32 = 220.0;
    const L: f32 = 40.0;
    const R: f32 = 8.0;
    const T: f32 = 22.0;
    const B: f32 = 24.0;
    let mut hover = use_signal(|| None::<usize>);
    let (pw, ph) = (cw - L - R, H - T - B);
    let cmp = |i: usize| compare.as_ref().and_then(|c| c.get(i).copied());
    let all = bars.iter().map(|b| b.1).chain(compare.iter().flatten().copied());
    let (lo, hi) = all.fold((0f32, 0f32), |(a, b), v| (a.min(v), b.max(v)));
    let (lo, hi, step, ticks) = nice_domain(lo, hi, 4);
    let y = move |v: f32| T + (hi - v) / (hi - lo) * ph;
    let slot = pw / bars.len().max(1) as f32;
    let bw = (slot * 0.6).min(28.0);
    let label_every = bars.len().div_ceil(12).max(1);
    let read = hover().and_then(|i| bars.get(i).map(|(l, v)| {
        let c = cmp(i).map(|c| format!("  (vs {})", fmt_pct(c))).unwrap_or_default();
        format!("{l}: {}{c}", fmt_pct(*v))
    }));

    rsx! {
        svg { view_box: "0 0 {cw} {H}", style: SVG_STYLE, onresize: move |e| track(&mut size, e),
            {y_grid(&ticks, y, L, cw - R, |t| fmt_tick(t, step, true))}
            if let Some(i) = hover() {
                rect { x: L + i as f32 * slot, y: T, width: slot, height: ph, fill: "var(--grid)", fill_opacity: 0.5 }
            }
            for (i, (label, v)) in bars.iter().enumerate() {
                path { d: bar_path(L + (i as f32 + 0.5) * slot - bw / 2.0, bw, y(0.0), y(*v)), fill: "{color}" }
                if let Some(c) = cmp(i) {
                    path {
                        d: bar_path(L + (i as f32 + 0.5) * slot - bw / 2.0, bw, y(0.0), y(c)),
                        fill: "none", stroke: "var(--ink)", stroke_width: 1.5, stroke_dasharray: "3 2"
                    }
                }
                if bars.len() <= 12 {
                    text {
                        x: L + (i as f32 + 0.5) * slot, y: y(v.max(cmp(i).unwrap_or(*v)).max(0.0)) - 5.0,
                        fill: "var(--ink)", font_size: 11, text_anchor: "middle", "{fmt_pct(*v)}"
                    }
                }
                if i % label_every == 0 {
                    text {
                        x: L + (i as f32 + 0.5) * slot, y: H - 8.0, fill: "var(--muted)", font_size: 11, text_anchor: "middle",
                        "{truncate(label, (slot * label_every as f32 / 6.5) as usize + 1)}"
                    }
                }
            }
            line { x1: L, x2: cw - R, y1: y(0.0), y2: y(0.0), stroke: "var(--muted)", stroke_width: 1 }
            if let Some(s) = read { {readout(cw - R, 12.0, s)} }
            g { onmouseleave: move |_| hover.set(None),
                for i in 0..bars.len() {
                    rect { x: L + i as f32 * slot, y: T, width: slot, height: ph, fill: "transparent", onmouseenter: move |_| hover.set(Some(i)) }
                }
            }
        }
    }
}

/// Histogram of raw values (auto bins), e.g. per-seed finish-time deltas. A zero line is drawn when 0 is in range.
/// `unit` labels the x axis ("s", "m", "HP").
#[component]
pub fn Histogram(values: Vec<f32>, color: String, unit: String) -> Element {
    let (mut size, cw) = use_width();
    const H: f32 = 200.0;
    const L: f32 = 36.0;
    const R: f32 = 12.0;
    const T: f32 = 22.0;
    const B: f32 = 24.0;
    let mut hover = use_signal(|| None::<usize>);
    let vals: Vec<f32> = values.iter().copied().filter(|v| v.is_finite()).collect();
    if vals.is_empty() {
        return rsx! { svg { view_box: "0 0 {cw} 40", style: SVG_STYLE, onresize: move |e| track(&mut size, e), text { x: cw / 2.0, y: 24, fill: "var(--muted)", font_size: 12, text_anchor: "middle", "No data" } } };
    }
    let (min, max) = vals.iter().fold((f32::MAX, f32::MIN), |(a, b), &v| (a.min(v), b.max(v)));
    let (min, max) = if max - min < 1e-6 { (min - 0.5, max + 0.5) } else { (min, max) };
    // ~sqrt(n) bins (5..=30) snapped to a nice width, edges on multiples of it so 0 lands on an edge.
    let bw = nice_step(max - min, (vals.len() as f32).sqrt().clamp(5.0, 30.0) as usize);
    let start = (min / bw).floor() * bw;
    let nb = (((max - start) / bw).floor() as usize + 1).max(1);
    let counts = bin_counts(vals.iter().copied(), start, bw, nb);
    let (lo, hi) = (start, start + nb as f32 * bw);
    let (pw, ph) = (cw - L - R, H - T - B);
    let x = move |v: f32| L + (v - lo) / (hi - lo) * pw;
    let (cmax, cstep, cticks) = count_axis(*counts.iter().max().unwrap_or(&0));
    let y = move |c: f32| T + (1.0 - c / cmax) * ph;
    let (_, _, xstep, xticks) = nice_domain(lo, hi, (pw / 80.0) as usize);
    let xticks: Vec<f32> = xticks.into_iter().filter(|t| (lo - 1e-4..=hi + 1e-4).contains(t)).collect();
    let read = hover().filter(|&i| i < nb).map(|i| {
        let a = start + i as f32 * bw;
        format!(
            "{} to {} {unit}: {} ({:.0}%)",
            fmt_tick(a, bw, false), fmt_tick(a + bw, bw, false), counts[i], 100.0 * counts[i] as f32 / vals.len() as f32
        )
    });

    rsx! {
        svg { view_box: "0 0 {cw} {H}", style: SVG_STYLE, onresize: move |e| track(&mut size, e),
            {y_grid(&cticks, y, L, cw - R, |t| fmt_tick(t, cstep, false))}
            {x_labels(&xticks, x, H - 8.0, cw - R - 30.0, |t| format!("{} {unit}", fmt_tick(t, xstep, false)))}
            for (i, &c) in counts.iter().enumerate() {
                if c > 0 {
                    path {
                        d: bar_path(x(start + i as f32 * bw) + 1.0, (pw / nb as f32 - 2.0).max(1.0), y(0.0), y(c as f32)),
                        fill: "{color}", fill_opacity: if hover() == Some(i) { 0.75 } else { 1.0 }
                    }
                }
            }
            line { x1: L, x2: cw - R, y1: y(0.0), y2: y(0.0), stroke: "var(--muted)", stroke_width: 1 }
            if lo < 0.0 && hi > 0.0 {
                line { x1: x(0.0), x2: x(0.0), y1: T - 4.0, y2: T + ph, stroke: "var(--ink)", stroke_width: 1.5, stroke_dasharray: "4 3" }
            }
            if let Some(s) = read { {readout(cw - R, 12.0, s)} }
            g { onmouseleave: move |_| hover.set(None),
                for i in 0..nb {
                    rect { x: x(start + i as f32 * bw), y: T, width: pw / nb as f32, height: ph, fill: "transparent", onmouseenter: move |_| hover.set(Some(i)) }
                }
            }
        }
    }
}

/// Last spurt delay (m after the final-leg start): the first bin is exactly the on-time window [0, `on_time`] in
/// green, later bins (>= 2 m wide, past `on_time`) in `color`. Legend above the plot, x axis at least 0..30 m.
#[component]
pub fn DelayHist(values: Vec<f32>, on_time: f32, color: String) -> Element {
    let (mut size, cw) = use_width();
    let mut hover = use_signal(|| None::<usize>);
    const H: f32 = 200.0;
    const L: f32 = 36.0;
    const R: f32 = 14.0;
    const T: f32 = 30.0;
    const B: f32 = 24.0;
    let vals: Vec<f32> = values.iter().copied().filter(|v| v.is_finite()).collect();
    let hi_v = vals.iter().copied().fold(30.0f32, f32::max);
    let bw = nice_step(hi_v - on_time, 20).max(2.0);
    let nl = ((hi_v - on_time) / bw).floor() as usize + 1;
    // Bin 0 = on time (the engine counts only > `on_time` as late), bin k >= 1 = the k-th `bw` past it.
    let mut counts = vec![0usize; nl + 1];
    for &v in &vals {
        let k = if v <= on_time { 0 } else { (((v - on_time) / bw).floor() as usize + 1).min(nl) };
        counts[k] += 1;
    }
    let hi = on_time + nl as f32 * bw;
    let edge = move |k: usize| if k == 0 { 0.0 } else { on_time + (k - 1) as f32 * bw };
    let (pw, ph) = (cw - L - R, H - T - B);
    let x = move |v: f32| L + v.clamp(0.0, hi) / hi * pw;
    let (cmax, cstep, cticks) = count_axis(*counts.iter().max().unwrap_or(&0));
    let y = move |c: f32| T + (1.0 - c / cmax) * ph;
    let (_, _, xstep, xticks) = nice_domain(0.0, hi, (pw / 80.0) as usize);
    let xticks: Vec<f32> = xticks.into_iter().filter(|t| (0.0..=hi + 1e-4).contains(t)).collect();
    let total = vals.len().max(1);
    let read = hover().filter(|&k| k <= nl).map(|k| {
        let what = if k == 0 { "on time".to_string() } else { format!("{:.1} to {:.1} m late", edge(k), edge(k + 1)) };
        format!("{what}: {} ({:.1}%)", counts[k], 100.0 * counts[k] as f32 / total as f32)
    });
    rsx! {
        svg { view_box: "0 0 {cw} {H}", style: SVG_STYLE, onresize: move |e| track(&mut size, e),
            // Legend above the plot (not over the bars).
            rect { x: L, y: 4, width: 10, height: 10, rx: 2, fill: "var(--good)" }
            text { x: L + 14.0, y: 13, fill: "var(--ink)", font_size: 11, "On time (up to {on_time} m, decided at the first step of the final leg)" }
            {y_grid(&cticks, y, L, cw - R, |t| fmt_tick(t, cstep, false))}
            {x_labels(&xticks, x, H - 8.0, cw - R - 30.0, |t| format!("{} m", fmt_tick(t, xstep, false)))}
            for (k, &c) in counts.iter().enumerate() {
                if c > 0 {
                    path {
                        d: bar_path(x(edge(k)) + 0.5, (x(edge(k + 1)) - x(edge(k)) - 1.0).max(1.0), y(0.0), y(c as f32)),
                        fill: if k == 0 { "var(--good)".to_string() } else { color.clone() },
                        fill_opacity: if hover() == Some(k) { 0.75 } else { 1.0 }
                    }
                }
            }
            line { x1: L, x2: cw - R, y1: y(0.0), y2: y(0.0), stroke: "var(--muted)", stroke_width: 1 }
            if let Some(s) = read { {readout(cw - R, 13.0, s)} }
            g { onmouseleave: move |_| hover.set(None),
                for k in 0..=nl {
                    rect { x: x(edge(k)), y: T, width: (x(edge(k + 1)) - x(edge(k))).max(1.0), height: ph, fill: "transparent", onmouseenter: move |_| hover.set(Some(k)) }
                }
            }
        }
    }
}

/// Compact horizontal box plot of a distribution: whiskers p10–p90, box p25–p75, median tick, over [lo, hi].
/// Fills its container's width at 18 px tall; a faint zero tick is drawn when 0 is inside [lo, hi].
#[component]
pub fn BoxRow(p10: f32, p25: f32, p50: f32, p75: f32, p90: f32, lo: f32, hi: f32, color: String) -> Element {
    let x = |v: f32| 2.0 + 96.0 * ((v - lo) / (hi - lo).max(1e-6)).clamp(0.0, 1.0);
    rsx! {
        svg { view_box: "0 0 100 18", preserve_aspect_ratio: "none", style: "display:block;width:100%;height:18px",
            if lo < 0.0 && hi > 0.0 {
                line { x1: x(0.0), x2: x(0.0), y1: 1, y2: 17, stroke: "var(--muted)", stroke_width: 1, vector_effect: "non-scaling-stroke" }
            }
            line { x1: x(p10), x2: x(p90), y1: 9, y2: 9, stroke: "var(--muted)", stroke_width: 1.5, vector_effect: "non-scaling-stroke" }
            for v in [p10, p90] {
                line { x1: x(v), x2: x(v), y1: 6, y2: 12, stroke: "var(--muted)", stroke_width: 1.5, vector_effect: "non-scaling-stroke" }
            }
            rect {
                x: x(p25), y: 4, width: (x(p75) - x(p25)).max(0.5), height: 10, fill: "{color}", fill_opacity: 0.35,
                stroke: "{color}", stroke_width: 1.5, vector_effect: "non-scaling-stroke"
            }
            line { x1: x(p50), x2: x(p50), y1: 2, y2: 16, stroke: "var(--ink)", stroke_width: 2.5, vector_effect: "non-scaling-stroke" }
        }
    }
}

/// Skill length gains in horse lengths, one row per skill in the given order: (label, colour,
/// [p10, p25, p50, p75, p90, mean], sample count); a row with no samples shows "n/a".
/// Box p25..p75, whiskers p10..p90, median tick, mean dot; shared axis with a zero line; mean column on the right.
#[component]
pub fn GainChart(rows: Vec<(String, String, [f32; 6], usize)>) -> Element {
    let (mut size, cw) = use_width();
    let mut hover = use_signal(|| None::<usize>);
    let stats: Vec<Option<[f32; 6]>> = rows.iter().map(|r| (r.3 > 0).then_some(r.2)).collect();
    const L: f32 = 180.0;
    const R: f32 = 60.0;
    const T: f32 = 22.0;
    const ROW: f32 = 22.0;
    let pw = cw - L - R;
    let h = T + rows.len() as f32 * ROW + 22.0;
    let (lo, hi) = stats.iter().flatten().fold((0f32, 0f32), |(a, b), s| (a.min(s[0]), b.max(s[4])));
    let (lo, hi, step, ticks) = nice_domain(lo, hi, (pw / 70.0).max(2.0) as usize);
    let x = move |v: f32| L + (v.clamp(lo, hi) - lo) / (hi - lo) * pw;
    let cy = |i: usize| T + (i as f32 + 0.5) * ROW;
    // Tooltip below the row, or above it near the bottom.
    let tip = hover().and_then(|i| stats.get(i).copied().flatten().map(|s| {
        let y = if i + 2 >= rows.len() && i > 0 { cy(i) - 40.0 } else { cy(i) + 12.0 };
        (y, s, rows[i].3)
    }));

    rsx! {
        svg { view_box: "0 0 {cw} {h}", style: SVG_STYLE, onresize: move |e| track(&mut size, e),
            text { x: 8, y: 12, fill: "var(--muted)", font_size: 11, "Gain (lengths)" }
            text { x: cw - 6.0, y: 12, fill: "var(--muted)", font_size: 11, text_anchor: "end", "Mean" }
            if let Some(i) = hover() {
                rect { x: 0, y: T + i as f32 * ROW, width: cw, height: ROW, fill: "var(--grid)", fill_opacity: 0.5 }
            }
            for t in ticks.iter() {
                line { x1: x(*t), x2: x(*t), y1: T, y2: h - 20.0, stroke: "var(--grid)", stroke_width: 1 }
                if x(*t) < cw - R - 20.0 {
                    text { x: x(*t), y: 12, fill: "var(--muted)", font_size: 11, text_anchor: "middle", "{fmt_tick(*t, step, false)}" }
                }
                text { x: x(*t), y: h - 6.0, fill: "var(--muted)", font_size: 11, text_anchor: "middle", "{fmt_tick(*t, step, false)}" }
            }
            line { x1: x(0.0), x2: x(0.0), y1: T - 4.0, y2: h - 20.0, stroke: "var(--ink)", stroke_width: 1, stroke_opacity: 0.6 }
            for (i, ((label, c, ..), st)) in rows.iter().zip(stats.iter()).enumerate() {
                text { x: L - 8.0, y: cy(i), fill: "var(--ink)", font_size: 11, text_anchor: "end", dominant_baseline: "central", "{truncate(label, 28)}" }
                if let Some([p10, p25, p50, p75, p90, mean]) = st {
                    line { x1: x(*p10), x2: x(*p90), y1: cy(i), y2: cy(i), stroke: "var(--muted)", stroke_width: 1.5 }
                    rect { x: x(*p25), y: cy(i) - 6.0, width: (x(*p75) - x(*p25)).max(1.0), height: 12, rx: 2, fill: "{c}", fill_opacity: 0.35, stroke: "{c}", stroke_width: 1.5 }
                    line { x1: x(*p50), x2: x(*p50), y1: cy(i) - 8.0, y2: cy(i) + 8.0, stroke: "var(--ink)", stroke_width: 2 }
                    circle { cx: x(*mean), cy: cy(i), r: 3, fill: "var(--surface)", stroke: "var(--ink)", stroke_width: 1.5 }
                    text { x: cw - 6.0, y: cy(i), fill: "var(--ink)", font_size: 11, font_weight: 600, text_anchor: "end", dominant_baseline: "central", "{mean:+.2}" }
                } else {
                    text { x: cw - 6.0, y: cy(i), fill: "var(--muted)", font_size: 11, text_anchor: "end", dominant_baseline: "central", "n/a" }
                }
            }
            if let Some((y, [p10, p25, p50, p75, p90, mean], n)) = tip {
                g { pointer_events: "none",
                    rect { x: L.max(cw - R - 420.0), y, width: 420, height: 26, rx: 6, fill: "var(--surface)", fill_opacity: 0.96, stroke: "var(--grid)" }
                    text {
                        x: L.max(cw - R - 420.0) + 8.0, y: y + 13.0, fill: "var(--ink)", font_size: 11, dominant_baseline: "central",
                        "mean {mean:+.2} · p10 {p10:+.2} · p25 {p25:+.2} · median {p50:+.2} · p75 {p75:+.2} · p90 {p90:+.2} · n {n}"
                    }
                }
            }
            g { onmouseleave: move |_| hover.set(None),
                for i in 0..rows.len() {
                    rect { x: 0, y: T + i as f32 * ROW, width: cw, height: ROW, fill: "transparent", onmouseenter: move |_| hover.set(Some(i)) }
                }
            }
        }
    }
}
