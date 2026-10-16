use super::*;

/// How often a skill activates along a stretch: (from m, to m, 0..1 of the busiest bin).
pub type Bins = Vec<(f32, f32, f32)>;
/// A `CourseProfile` region: (start m, end m, label, colour, bins).
pub type Region = (f32, f32, String, String, Bins);
/// A skill effect on `CourseProfile`: from activation to where it ended, its name in `color` underlined by a bar that
/// long, tinted down to the speed line (or the ridge). `tip` = the hover text.
#[derive(Clone, Debug, PartialEq)]
pub struct SkillMark {
    pub start: f32,
    pub end: f32,
    pub name: String,
    pub color: String,
    pub tip: String,
}

/// Longest skill name on the course before "…" (the grade mark ◎/○ is kept); the hover has the full name.
const MARK_NAME_MAX: usize = 16;

fn mark_name(name: &str) -> String {
    let (base, grade) = match name.trim_end().char_indices().last() {
        Some((i, c)) if matches!(c, '◎' | '○') => (name[..i].trim_end(), &name[i..]),
        _ => (name, ""),
    };
    if base.chars().count() <= MARK_NAME_MAX {
        return name.to_string();
    }
    let cut: String = base.chars().take(MARK_NAME_MAX).collect();
    format!("{}…{}{grade}", cut.trim_end_matches([' ', ',', ':']), if grade.is_empty() { "" } else { " " })
}
/// One race state of one uma along the course (e.g. rushed). `CourseProfile` draws a row each, `RaceChart` tinted
/// regions; both with the icon.
#[derive(Clone, Debug, PartialEq)]
pub struct Band {
    pub label: String,
    pub color: String,
    /// 24x24 stroke path.
    pub icon: String,
    /// Stretches (from m, to m).
    pub stretches: Vec<(f32, f32)>,
    /// Seconds in it.
    pub secs: f32,
}

/// Icon positions for `bands`: (band, x px), one per stretch's middle, sorted by x; one that would overlap the
/// previous icon of its own band (closer than `gap` px) is dropped, so a run of short stretches gets one.
pub(super) fn band_markers(bands: &[Band], x: impl Fn(f32) -> f32, gap: f32) -> Vec<(usize, f32)> {
    let x = &x;
    let mut v: Vec<(usize, f32)> =
        bands.iter().enumerate().flat_map(|(k, b)| b.stretches.iter().map(move |&(a, e)| (k, (x(a) + x(e)) / 2.0))).collect();
    v.sort_by(|a, b| a.1.total_cmp(&b.1));
    let mut last = vec![f32::MIN; bands.len()];
    v.retain(|&(k, cx)| {
        let keep = cx - last[k] >= gap;
        if keep {
            last[k] = cx;
        }
        keep
    });
    v
}

/// A band's icon at (`cx`, `cy`): its glyph on a disc of its colour, radius `r`; a faded dashed ring when `ghost`
/// (the baseline's events).
pub(super) fn band_icon(b: &Band, cx: f32, cy: f32, r: f32, ghost: bool) -> Element {
    let g = r * 1.27; // glyph size
    rsx! {
        g { opacity: if ghost { 0.7 } else { 1.0 },
            title { "{b.label} {b.secs:.1} s" }
            circle {
                cx, cy, r, fill: if ghost { "var(--surface)" } else { b.color.as_str() },
                stroke: if ghost { b.color.as_str() } else { "var(--surface)" }, stroke_width: 1.5,
                stroke_dasharray: if ghost { "3 2" } else { "none" },
            }
            path {
                d: "{b.icon}", transform: "translate({cx - g / 2.0} {cy - g / 2.0}) scale({g / 24.0})", fill: "none",
                stroke: if ghost { b.color.as_str() } else { "#1b1420" }, stroke_width: 2.2 + (11.0 - r) * 0.1,
                stroke_linecap: "round", stroke_linejoin: "round"
            }
        }
    }
}

/// Diagonal hatch patterns `{prefix}-{k}`, one per band in its colour (the baseline's events).
pub(super) fn band_hatches(prefix: &str, bands: &[Band]) -> Element {
    rsx! {
        defs {
            for (k, b) in bands.iter().enumerate() {
                pattern { id: "{prefix}-{k}", width: 6, height: 6, pattern_units: "userSpaceOnUse", pattern_transform: "rotate(45)",
                    rect { width: 2, height: 6, fill: "{b.color}", fill_opacity: 0.55 }
                }
            }
        }
    }
}

/// Phase boundaries: 0, the phase starts inside the course, `d`.
pub(super) fn phase_bounds(phases: &[f32], d: f32) -> Vec<f32> {
    std::iter::once(0.0).chain(phases.iter().copied().filter(|&p| p > 0.0 && p < d)).chain([d]).collect()
}

/// Activation distances binned finely (`bin` m) over the whole course, with the course's phases (late race and last
/// spurt tinted), corners (grey) and straights (light blue) marked under the axis, so the bars line up with the track.
/// Hover shows the bin and count. `start` = left end of the x axis (m), to zoom in on the end of the course (the axis
/// then reads e.g. "1700 m").
#[component]
pub fn CourseHist(
    distances: Vec<f32>, course_distance: f32, phases: Vec<f32>, corners: Vec<(f32, f32)>, bin: f32, color: String,
    #[props(default)] start: f32, #[props(default)] straights: Vec<(f32, f32)>,
) -> Element {
    let (mut size, cw) = use_width();
    let mut hover = use_signal(|| None::<usize>);
    const H: f32 = 190.0;
    const L: f32 = 36.0;
    const R: f32 = 14.0;
    const T: f32 = 22.0;
    const B: f32 = 40.0;
    let d = course_distance.max(1.0);
    let s0 = start.clamp(0.0, d - 1.0);
    let bw = bin.max(1.0);
    let nb = ((d - s0) / bw).ceil().max(1.0) as usize;
    let counts = bin_counts(distances.iter().copied().filter(|v| v.is_finite() && *v >= s0), s0, bw, nb);
    let total = distances.len().max(1);
    let (pw, ph) = (cw - L - R, H - T - B);
    let x = move |m: f32| L + (m.clamp(s0, d) - s0) / (d - s0) * pw;
    let (cmax, cstep, cticks) = count_axis(*counts.iter().max().unwrap_or(&0));
    let y = move |c: f32| T + (1.0 - c / cmax) * ph;
    let read = hover().filter(|&i| i < nb).map(|i| {
        let a = s0 + i as f32 * bw;
        format!("{a:.0} to {:.0} m: {} ({:.1}%)", a + bw, counts[i], 100.0 * counts[i] as f32 / total as f32)
    });
    rsx! {
        svg { view_box: "0 0 {cw} {H}", style: SVG_STYLE, onresize: move |e| track(&mut size, e),
            {course_axis(x, &phase_bounds(&phases, d), s0, T, T + ph, H, cw - R, &corners, &straights, &dist_ticks(s0, d, pw), true)}
            if s0 > 0.0 {
                text { x: cw - R, y: T - 8.0, fill: "var(--ink)", font_size: 11, font_weight: 700, text_anchor: "end", "Goal" }
                line { x1: cw - R, x2: cw - R, y1: T - 4.0, y2: T + ph, stroke: "var(--ink)", stroke_width: 1.5 }
            }
            {y_grid(&cticks, y, L, cw - R, |t| fmt_tick(t, cstep, false))}
            for (i, &c) in counts.iter().enumerate() {
                if c > 0 {
                    rect { x: x(s0 + i as f32 * bw), y: y(c as f32), width: (pw / nb as f32).max(1.0), height: y(0.0) - y(c as f32), fill: "{color}", fill_opacity: if hover() == Some(i) { 0.7 } else { 1.0 } }
                }
            }
            line { x1: L, x2: cw - R, y1: y(0.0), y2: y(0.0), stroke: "var(--muted)", stroke_width: 1 }
            if let Some(s) = read { {readout(cw - R, 12.0, s)} }
            g { onmouseleave: move |_| hover.set(None),
                for i in 0..nb {
                    rect { x: x(s0 + i as f32 * bw), y: T, width: (pw / nb as f32).max(1.0), height: ph, fill: "transparent", onmouseenter: move |_| hover.set(Some(i)) }
                }
            }
        }
    }
}

/// Fewer activations than this in a bin: its gain bar is hollow (mostly noise), and it doesn't set the scale.
const GAIN_MIN_N: usize = 5;

/// Lengths gained by where a skill activated: per `bin` m, the average gain of the seeds that activated there (green
/// ahead, red behind; hollow under `GAIN_MIN_N`), how often it activates there as a strip below, and the course's
/// phases, corners and straights along the bottom. `by_distance` = (activation m, lengths) per seed.
#[component]
pub fn GainBars(
    by_distance: Vec<(f32, f32)>, course_distance: f32, phases: Vec<f32>, corners: Vec<(f32, f32)>,
    straights: Vec<(f32, f32)>, bin: f32,
) -> Element {
    let (mut size, cw) = use_width();
    let mut hover = use_signal(|| None::<usize>);
    const L: f32 = 52.0;
    const R: f32 = 14.0;
    const T: f32 = 22.0;
    const PH: f32 = 130.0;
    const STRIP: f32 = 16.0;
    let strip_b = T + PH + 6.0 + STRIP;
    let (axis_y, h) = (strip_b + 4.0, strip_b + 56.0);
    let d = course_distance.max(1.0);
    let bw = bin.max(1.0);
    let nb = (d / bw).ceil().max(1.0) as usize;
    // Per bin: activations, sum of gains, sorted gains (for the median).
    let mut sums = vec![(0usize, 0f32, Vec::new()); nb];
    for &(m, g) in by_distance.iter().filter(|p| p.0.is_finite() && p.1.is_finite()) {
        let b = &mut sums[((m / bw).floor() as usize).min(nb - 1)];
        b.0 += 1;
        b.1 += g;
        b.2.push(g);
    }
    for b in &mut sums {
        b.2.sort_by(f32::total_cmp);
    }
    let mean = |i: usize| sums[i].1 / sums[i].0.max(1) as f32;
    let total = by_distance.len().max(1);
    let max_n = sums.iter().map(|b| b.0).max().unwrap_or(0).max(1);
    // Scale from the trustworthy bins (all of them when none is); the others are clamped to it.
    let solid: Vec<f32> = (0..nb).filter(|&i| sums[i].0 >= GAIN_MIN_N).map(mean).collect();
    let scale: Vec<f32> = if solid.is_empty() { (0..nb).filter(|&i| sums[i].0 > 0).map(mean).collect() } else { solid };
    let (lo, hi, step, ticks) = nice_domain(scale.iter().copied().fold(0.0, f32::min), scale.iter().copied().fold(0.0, f32::max), 4);
    let pw = cw - L - R;
    let x = move |m: f32| L + m.clamp(0.0, d) / d * pw;
    let y = move |v: f32| T + (1.0 - (v.clamp(lo, hi) - lo) / (hi - lo)) * PH;
    let read = hover().filter(|&i| i < nb).map(|i| {
        let (a, n) = (i as f32 * bw, sums[i].0);
        match n {
            0 => format!("{a:.0} to {:.0} m: no activations", a + bw),
            _ => format!(
                "{a:.0} to {:.0} m: {n} activation{} ({:.1}%), average {:+.2} L, median {:+.2} L",
                a + bw,
                if n == 1 { "" } else { "s" },
                100.0 * n as f32 / total as f32,
                mean(i),
                sums[i].2[n / 2]
            ),
        }
    });
    let bars: Vec<(usize, f32, usize)> = (0..nb).filter(|&i| sums[i].0 > 0).map(|i| (i, mean(i), sums[i].0)).collect();
    rsx! {
        svg { view_box: "0 0 {cw} {h}", style: SVG_STYLE, onresize: move |e| track(&mut size, e),
            {course_axis(x, &phase_bounds(&phases, d), 0.0, T, axis_y, h, cw - R, &corners, &straights, &dist_ticks(0.0, d, pw), true)}
            {y_grid(&ticks, y, L, cw - R, |t| format!("{} L", fmt_tick(t, step, false)))}
            line { x1: L, x2: cw - R, y1: y(0.0), y2: y(0.0), stroke: "var(--muted)", stroke_width: 1 }
            for &(i, m, n) in bars.iter() {
                {
                    let c = if m >= 0.0 { "var(--good)" } else { "var(--bad)" };
                    let (bx, bwid) = (x(i as f32 * bw) + 1.0, (pw / nb as f32 - 2.0).max(1.0));
                    let (top, bh) = (y(m).min(y(0.0)), (y(m) - y(0.0)).abs().max(1.0));
                    let sh = 2.0 + (STRIP - 2.0) * n as f32 / max_n as f32;
                    rsx! {
                        if n >= GAIN_MIN_N {
                            rect { x: bx, y: top, width: bwid, height: bh, rx: 2, fill: c, fill_opacity: if hover() == Some(i) { 0.75 } else { 1.0 } }
                        } else {
                            rect { x: bx + 0.6, y: top + 0.6, width: (bwid - 1.2).max(1.0), height: (bh - 1.2).max(1.0), rx: 2, fill: "none", stroke: c, stroke_width: 1.2 }
                        }
                        rect { x: bx, y: strip_b - sh, width: bwid, height: sh, rx: 1.5, fill: "var(--muted)", fill_opacity: 0.7 }
                    }
                }
            }
            text { x: L - 6.0, y: strip_b - 3.0, fill: "var(--muted)", font_size: 10, text_anchor: "end", "how often" }
            if let Some(s) = read { {readout(cw - R, 12.0, s)} }
            g { onmouseleave: move |_| hover.set(None),
                for i in 0..nb {
                    rect { x: x(i as f32 * bw), y: T, width: (pw / nb as f32).max(1.0), height: strip_b - T, fill: "transparent", onmouseenter: move |_| hover.set(Some(i)) }
                }
            }
        }
    }
}

/// The course under a distance chart from `s0` m: late race / last spurt tinted from `top` down to the corner and
/// straight bands at `axis_y` (untinted unless `tint`), phase lines, distance labels below them and phase names along the bottom (`h`).
#[allow(clippy::too_many_arguments)]
pub(super) fn course_axis(
    x: impl Fn(f32) -> f32, bounds: &[f32], s0: f32, top: f32, axis_y: f32, h: f32, right: f32, corners: &[(f32, f32)],
    straights: &[(f32, f32)], ticks: &[f32], tint: bool,
) -> Element {
    rsx! {
        for (i, w) in bounds.windows(2).enumerate().filter(|(_, w)| w[1] > s0) {
            rect { x: x(w[0]), y: top, width: (x(w[1]) - x(w[0])).max(0.0), height: axis_y - top, fill: if tint && i >= 2 { "var(--accent)" } else { "transparent" }, fill_opacity: if i == 2 { 0.08 } else { 0.14 } }
            if x(w[1]) - x(w[0]) > PHASES[i.min(3)].len() as f32 * 6.5 {
                text { x: (x(w[0]) + x(w[1])) / 2.0, y: h - 6.0, fill: "var(--muted)", font_size: 10.5, text_anchor: "middle", "{PHASES[i.min(3)]}" }
            }
            if i > 0 && w[0] > s0 {
                line { x1: x(w[0]), x2: x(w[0]), y1: top, y2: axis_y + 16.0, stroke: "var(--muted)", stroke_width: 1, stroke_dasharray: "3 3" }
            }
        }
        for &(a, b) in corners.iter().filter(|c| c.1 > s0) {
            rect { x: x(a), y: axis_y + 2.0, width: (x(b) - x(a)).max(1.0), height: 5, rx: 2, fill: "var(--muted)", fill_opacity: 0.5 }
        }
        for &(a, b) in straights.iter().filter(|c| c.1 > s0) {
            rect { x: x(a), y: axis_y + 2.0, width: (x(b) - x(a)).max(1.0), height: 5, rx: 2, fill: "var(--accent-2)", fill_opacity: 0.6 }
        }
        {x_labels(ticks, &x, axis_y + 18.0, right - 24.0, |t| format!("{t} m"))}
    }
}

/// One row per skill along the course: dots/ticks at activation distances (0..`course_distance` m), shaded final
/// leg from `final_leg_start`. Row = (label, colour, distances).
#[component]
pub fn CourseStrip(rows: Vec<(String, String, Vec<f32>)>, course_distance: f32, final_leg_start: f32) -> Element {
    let (mut size, cw) = use_width();
    const L: f32 = 150.0;
    const R: f32 = 14.0;
    const T: f32 = 22.0;
    const B: f32 = 22.0;
    const ROW: f32 = 22.0;
    let mut hover = use_signal(|| None::<usize>);
    let h = T + rows.len() as f32 * ROW + B;
    let pw = cw - L - R;
    let course = course_distance.max(1.0);
    let x = move |d: f32| L + d.clamp(0.0, course) / course * pw;
    let (_, _, _, ticks) = nice_domain(0.0, course, 6);
    let ticks: Vec<f32> = ticks.into_iter().filter(|&t| t <= course).collect();
    let read = hover().and_then(|i| rows.get(i)).map(|(label, _, d)| {
        let mut s = d.clone();
        s.sort_by(f32::total_cmp);
        match s.len() {
            0 => format!("{label}: no activations"),
            n => format!("{label}: {n}× · median {:.0} m", s[n / 2]),
        }
    });

    rsx! {
        svg { view_box: "0 0 {cw} {h}", style: SVG_STYLE, onresize: move |e| track(&mut size, e),
            rect { x: x(final_leg_start), y: T - 4.0, width: x(course) - x(final_leg_start), height: rows.len() as f32 * ROW + 4.0, fill: "var(--accent)", fill_opacity: 0.12 }
            text { x: x(final_leg_start) + 4.0, y: T - 8.0, fill: "var(--muted)", font_size: 11, "Late race" }
            for &t in ticks.iter() {
                line { x1: x(t), x2: x(t), y1: T - 4.0, y2: h - B, stroke: "var(--grid)", stroke_width: 1 }
            }
            {x_labels(&ticks, x, h - 7.0, cw - R - 24.0, |t| format!("{t} m"))}
            for (i, (label, c, ds)) in rows.iter().enumerate() {
                if hover() == Some(i) {
                    rect { x: 0, y: T + i as f32 * ROW, width: cw, height: ROW, fill: "var(--grid)", fill_opacity: 0.5 }
                }
                text { x: L - 10.0, y: T + (i as f32 + 0.5) * ROW, fill: "var(--ink)", font_size: 11, text_anchor: "end", dominant_baseline: "middle", "{truncate(label, 24)}" }
                line { x1: L, x2: cw - R, y1: T + (i as f32 + 0.5) * ROW, y2: T + (i as f32 + 0.5) * ROW, stroke: "var(--grid)", stroke_width: 1.5 }
                if ds.len() <= 40 {
                    for &d in ds.iter() {
                        circle { cx: x(d), cy: T + (i as f32 + 0.5) * ROW, fill: "{c}", r: 4.0, stroke: "var(--surface)", stroke_width: 1.5 }
                    }
                } else {
                    // Many activations (batch): one bar per ~10 m bucket, opacity by count. One node per sample
                    // would be tens of thousands of nodes for big batches.
                    for (b0, b1, k) in density(ds, course, pw) {
                        rect {
                            x: x(b0), y: T + (i as f32 + 0.5) * ROW - 5.0, width: (x(b1) - x(b0)).max(2.0), height: 10, rx: 1.5,
                            fill: "{c}", fill_opacity: 0.35 + 0.65 * k
                        }
                    }
                }
            }
            if let Some(s) = read { {readout(cw - R, 12.0, s)} }
            g { onmouseleave: move |_| hover.set(None),
                for i in 0..rows.len() {
                    rect { x: 0, y: T + i as f32 * ROW, width: cw, height: ROW, fill: "transparent", onmouseenter: move |_| hover.set(Some(i)) }
                }
            }
        }
    }
}

/// Activation density buckets for CourseStrip: (start m, end m, count relative to the busiest bucket).
fn density(ds: &[f32], course: f32, pw: f32) -> Vec<(f32, f32, f32)> {
    // ~10 m, but at least 2 px wide.
    let bw = 10f32.max(course * 2.0 / pw.max(1.0));
    let nb = (course / bw).ceil().max(1.0) as usize;
    let counts = bin_counts(ds.iter().copied(), 0.0, bw, nb);
    let max = *counts.iter().max().unwrap_or(&1) as f32;
    counts.iter().enumerate().filter(|c| *c.1 > 0).map(|(i, &c)| (i as f32 * bw, (i as f32 + 1.0) * bw, c as f32 / max)).collect()
}

const PHASES: [&str; 4] = ["Early race", "Mid race", "Late race", "Last spurt"];

/// Elevation profile sampled evenly over 0..=`dist`; when none is given, integrate the slope grades on a 5 m grid.
pub(super) fn elevation_or_slopes(elevation: &[f32], slopes: &[(f32, f32, f32)], dist: f32) -> Vec<f32> {
    if elevation.len() >= 2 {
        return elevation.to_vec();
    }
    let mut e = 0.0;
    (0..=(dist / 5.0).ceil() as usize)
        .map(|i| {
            let d = i as f32 * 5.0;
            let v = e;
            e += slopes.iter().find(|s| (s.0..s.1).contains(&d)).map_or(0.0, |s| s.2) / 100.0 * 5.0;
            v
        })
        .collect()
}

/// Side-view course preview (umalator-style course chart). x = 0..`distance` m.
/// - `elevation`: height in m, evenly spaced start..goal (~5 m apart); drawn as a filled silhouette with the vertical
///   scale exaggerated. Empty = derived from `slopes`.
/// - `corners`, `straights`: (start m, end m); corners numbered, the last straight labelled as the final straight.
/// - `slopes`: (start m, end m, grade %), + = uphill; tinted on the silhouette ridge and labelled.
/// - `phases`: start m of Early race, Mid race, Late race, Last spurt (a leading 0 is optional).
/// - `regions`: (start m, end m, label, colour, bins) packed into lanes above the track: a bar, or with `bins`
///   ((from m, to m, 0..1) along it) a little histogram of where it activates.
/// - `marks`: skill effects in the elevation band: each name in the band's free space (above or below the lines,
///   left or right of its bar), a bar from activation to end under it and a tint down to the speed line or the ridge.
/// - `speed`: (m, m/s) of one uma, drawn across the elevation band on its own scale (ticks on the right); a skill's
///   stretch of it takes the skill's colour.
/// - `bands`: one row each under the layout band, a bar per stretch (e.g. when an uma was rushed); the chart grows.
/// Final leg (from `final_leg_start`) is pink. Hover shows metres, phase, corner/straight, grade, speed and what ran
/// there.
#[component]
pub fn CourseProfile(
    distance: f32, elevation: Vec<f32>, corners: Vec<(f32, f32)>, straights: Vec<(f32, f32)>,
    slopes: Vec<(f32, f32, f32)>, phases: Vec<f32>, final_leg_start: f32, regions: Vec<Region>,
    marks: Vec<SkillMark>, bands: Vec<Band>, #[props(default)] speed: Vec<(f32, f32)>,
) -> Element {
    let (mut size, cw) = use_width();
    let mut hover = use_signal(|| None::<f32>);
    const L: f32 = 72.0;
    const R: f32 = 14.0;
    const LANE: f32 = 20.0;
    const EH: f32 = 96.0;
    let pw = cw - L - R;
    let dist = distance.max(1.0);
    let x = move |d: f32| L + d.clamp(0.0, dist) / dist * pw;
    let fits = |a: f32, b: f32, s: &str| x(b) - x(a) > s.chars().count() as f32 * 6.2 + 6.0;

    // Greedy lane packing by start; a region's footprint includes its label.
    let mut order: Vec<usize> = (0..regions.len()).collect();
    order.sort_by(|&a, &b| regions[a].0.total_cmp(&regions[b].0));
    let mut lane_end: Vec<f32> = vec![];
    let mut lanes = vec![(0usize, 0f32); regions.len()]; // (lane, label x)
    for &i in &order {
        let (a, b, label, ..) = &regions[i];
        let lw = label.chars().count() as f32 * 6.8;
        let lx = x(*a).min(cw - R - lw);
        let k = lane_end.iter().position(|&e| e <= lx.min(x(*a))).unwrap_or_else(|| { lane_end.push(0.0); lane_end.len() - 1 });
        lane_end[k] = x(*b).max(lx + lw) + 10.0;
        lanes[i] = (k, lx);
    }
    let rg_y = 22.0;
    let ph_y = rg_y + lane_end.len() as f32 * LANE + 4.0;
    let (el_y, el_b) = (ph_y + 24.0, ph_y + 24.0 + EH);
    let lay_y = el_b + 4.0;
    const BAND: f32 = 18.0;
    let band_y = lay_y + 22.0;
    // Bottom of the plotted rows: the layout band, or the last band row.
    let bot = if bands.is_empty() { lay_y + 18.0 } else { band_y + bands.len() as f32 * BAND - 4.0 };
    let h = bot + 24.0;

    let elev = elevation_or_slopes(&elevation, &slopes, dist);
    let (emin, emax) = elev.iter().fold((f32::MAX, f32::MIN), |(a, b), &e| (a.min(e), b.max(e)));
    let vs = EH * 0.7 / (emax - emin).max(1.0); // px per metre of height; >= 1 m range so flat courses stay flat
    let ey = move |e: f32| el_b - 8.0 - (e - emin) * vs;
    let ne = elev.len() - 1;
    let e_at = |d: f32| {
        let f = (d / dist).clamp(0.0, 1.0) * ne as f32;
        let i = (f.floor() as usize).min(ne - 1);
        elev[i] + (elev[i + 1] - elev[i]) * (f - i as f32)
    };
    let ridge = |a: f32, b: f32| {
        let mut v = vec![(x(a), ey(e_at(a)))];
        v.extend((0..=ne).map(|i| i as f32 * dist / ne as f32).filter(|&d| d > a && d < b).map(|d| (x(d), ey(e_at(d)))));
        v.push((x(b), ey(e_at(b))));
        v
    };
    let area = |a: f32, b: f32| {
        let mut v = vec![(x(a), el_b)];
        v.extend(ridge(a, b));
        v.push((x(b), el_b));
        pts_str(&v)
    };
    let (full_area, final_area, full_ridge) = (area(0.0, dist), area(final_leg_start, dist), pts_str(&ridge(0.0, dist)));
    let slope_ridges: Vec<(String, f32, f32, f32)> = slopes.iter().map(|&(a, b, g)| (pts_str(&ridge(a, b)), a, b, g)).collect();
    let slope_label_y = |a: f32, b: f32| ey(e_at((a + b) / 2.0).max(e_at(a)).max(e_at(b))) - 7.0;

    // Speed on its own scale over the band: from the slowest after the start dash (slower clamps to the bottom) to
    // the fastest, whole m/s.
    let sp_lo = speed.iter().filter(|p| p.0 > dist * 0.1).map(|p| p.1).fold(f32::MAX, f32::min);
    let sp_hi = speed.iter().map(|p| p.1).fold(f32::MIN, f32::max);
    let (sp_lo, sp_hi) = ((sp_lo - 0.5).floor(), (sp_hi + 0.5).ceil().max(sp_lo.floor() + 2.0));
    let sy = move |v: f32| el_b - 4.0 - (v.clamp(sp_lo, sp_hi) - sp_lo) / (sp_hi - sp_lo) * (EH - 8.0);
    let s_at = |d: f32| {
        let k = speed.partition_point(|p| p.0 <= d);
        match (k.checked_sub(1).map(|i| speed[i]), speed.get(k).copied()) {
            (Some(a), Some(b)) if b.0 > a.0 => a.1 + (b.1 - a.1) * (d - a.0) / (b.0 - a.0),
            (Some(p), _) | (None, Some(p)) => p.1,
            _ => 0.0,
        }
    };
    let s_line = |a: f32, b: f32| {
        let mut v = vec![(x(a), sy(s_at(a)))];
        v.extend(speed.iter().filter(|p| p.0 > a && p.0 < b).map(|p| (x(p.0), sy(p.1))));
        v.push((x(b), sy(s_at(b))));
        v
    };
    let has_speed = !speed.is_empty();
    let sp_ticks: Vec<f32> = if has_speed { (sp_lo as i32..=sp_hi as i32).filter(|v| v % 2 == 0).map(|v| v as f32).collect() } else { vec![] };
    // What each mark's tint drops to.
    let anchor = |d: f32| if has_speed { sy(s_at(d)) } else { ey(e_at(d)) };

    // Mark names: lanes LH apart over the band. A name fits where it clears the other names, the slope labels and
    // (first pass) the ridge and speed lines along its width; then across a line, never over a name.
    const LH: f32 = 16.0;
    let mut boxes: Vec<(f32, f32, f32, f32)> = slopes
        .iter()
        .filter(|s| x(s.1) - x(s.0) > 14.0)
        .map(|&(a, b, g)| {
            let (c, w, y) = ((x(a) + x(b)) / 2.0, format!("{g:+.1}%").len() as f32 * 3.4, slope_label_y(a, b));
            (c - w, y - 10.0, c + w, y + 3.0)
        })
        .collect();
    let crosses = |x0: f32, x1: f32, y0: f32, y1: f32| {
        let mut px = x0;
        while px <= x1 {
            let d = (px - L) / pw * dist;
            let hit = |y: f32| y >= y0 - 2.0 && y <= y1 + 2.0;
            if hit(ey(e_at(d))) || (has_speed && hit(sy(s_at(d)))) {
                return true;
            }
            px += 4.0;
        }
        false
    };
    // (label, label x, anchor, lane y) per mark.
    let mut placed: Vec<(String, f32, &str, f32)> = vec![];
    for m in marks.iter() {
        let label = mark_name(&m.name);
        let (a, z, w) = (x(m.start), x(m.end).max(x(m.start) + 3.0), label.chars().count() as f32 * 6.4);
        let mut cands: Vec<(f32, f32, &str, f32)> =
            [(a, (a + w).max(z), "start", a), ((z - w).min(a), z, "end", z)].into_iter().filter(|c| c.0 >= L - 4.0 && c.1 <= cw - R + 2.0).collect();
        if cands.is_empty() {
            let s = a.min(cw - R - w).max(L);
            cands.push((s, s + w, "start", s));
        }
        let free = |c: &(f32, f32, &str, f32), y: f32| boxes.iter().all(|&(p, q, r, s)| c.1 + 8.0 <= p || c.0 >= r + 8.0 || y + LH <= q || y >= s);
        let lanes = || (0..).map(|k| el_y + 2.0 + k as f32 * LH).take_while(|y| y + LH <= el_b);
        let spot = lanes()
            .find_map(|y| cands.iter().find(|c| free(c, y) && !crosses(c.0, c.1, y, y + LH)).map(|c| (y, *c)))
            .or_else(|| lanes().find_map(|y| cands.iter().find(|c| free(c, y)).map(|c| (y, *c))))
            .unwrap_or((el_y + 2.0, cands[0]));
        let (y, c) = spot;
        boxes.push((c.0, y, c.1, y + LH));
        placed.push((label, c.3, c.2, y));
    }

    let bounds = phase_bounds(&phases, dist);
    let final_straight = straights.iter().enumerate().max_by(|a, b| a.1.1.total_cmp(&b.1.1)).map(|(i, _)| i);
    let ticks = dist_ticks(0.0, dist, pw);
    let read = hover().map(|d| {
        let mut parts = vec![format!("{d:.0} m"), PHASES[bounds.windows(2).position(|w| d < w[1]).unwrap_or(0).min(3)].to_string()];
        if let Some(i) = corners.iter().position(|c| (c.0..c.1).contains(&d)) {
            parts.push(format!("Corner {}", i + 1));
        } else if let Some(i) = straights.iter().position(|s| (s.0..s.1).contains(&d)) {
            parts.push(if Some(i) == final_straight { "Final straight".into() } else { "Straight".into() });
        }
        parts.push(match slopes.iter().find(|s| (s.0..s.1).contains(&d)) {
            Some(s) => format!("{} {:+.1}%", if s.2 > 0.0 { "Uphill" } else { "Downhill" }, s.2),
            None => "Flat".into(),
        });
        parts.push(format!("elevation {:+.1} m", e_at(d) - emin));
        if has_speed {
            parts.push(format!("{:.1} m/s", s_at(d)));
        }
        parts.extend(regions.iter().filter(|r| (r.0..=r.1).contains(&d)).map(|r| r.2.clone()));
        parts.extend(marks.iter().filter(|m| (m.start..=m.end.max(m.start + dist * 0.004)).contains(&d)).map(|m| m.name.clone()));
        parts.extend(bands.iter().filter(|b| b.stretches.iter().any(|s| (s.0..=s.1).contains(&d))).map(|b| b.label.clone()));
        parts.join(" · ")
    });

    rsx! {
        svg { view_box: "0 0 {cw} {h}", style: SVG_STYLE, onresize: move |e| track(&mut size, e),
            for &t in ticks.iter() {
                line { x1: x(t), x2: x(t), y1: el_y, y2: bot, stroke: "var(--grid)", stroke_width: 1 }
            }
            {x_labels(&ticks, x, h - 7.0, cw - R - 24.0, |t| format!("{t} m"))}
            // Regions (activation bars) above the track.
            for (i, (a, b, label, c, bins)) in regions.iter().enumerate() {
                text { x: lanes[i].1, y: rg_y + lanes[i].0 as f32 * LANE + 9.0, fill: "var(--ink)", font_size: 11, "{label}" }
                if bins.is_empty() {
                    rect { x: x(*a), y: rg_y + lanes[i].0 as f32 * LANE + 12.0, width: (x(*b) - x(*a)).max(4.0), height: 5, rx: 2.5, fill: "{c}" }
                } else {
                    // Faint baseline over the band, bars as tall as how often it starts there.
                    rect { x: x(*a), y: rg_y + lanes[i].0 as f32 * LANE + 18.0, width: (x(*b) - x(*a)).max(1.0), height: 1, fill: "{c}", fill_opacity: 0.4 }
                    for &(f, t, w) in bins.iter() {
                        rect {
                            x: x(f), y: rg_y + lanes[i].0 as f32 * LANE + 19.0 - (1.5 + 6.5 * w),
                            width: (x(t) - x(f) - 0.5).max(1.0), height: 1.5 + 6.5 * w, fill: "{c}",
                        }
                    }
                }
            }
            // Phase spans + boundary lines.
            for (i, w) in bounds.windows(2).enumerate() {
                rect {
                    x: x(w[0]) + 1.0, y: ph_y, width: (x(w[1]) - x(w[0]) - 2.0).max(0.0), height: 18, rx: 4,
                    fill: if i >= 2 { "var(--accent)" } else { "var(--muted)" }, fill_opacity: if i >= 2 { 0.28 } else { 0.16 }
                }
                if fits(w[0], w[1], PHASES[i.min(3)]) {
                    text { x: (x(w[0]) + x(w[1])) / 2.0, y: ph_y + 9.0, fill: "var(--ink)", font_size: 11, text_anchor: "middle", dominant_baseline: "central", "{PHASES[i.min(3)]}" }
                }
                if i > 0 {
                    line { x1: x(w[0]), x2: x(w[0]), y1: ph_y, y2: bot, stroke: "var(--muted)", stroke_width: 1, stroke_dasharray: "3 3" }
                }
            }
            // Elevation silhouette; final leg in pink, slopes tinted on the ridge.
            polygon { points: "{full_area}", fill: "var(--muted)", fill_opacity: 0.2 }
            polygon { points: "{final_area}", fill: "var(--accent)", fill_opacity: 0.3 }
            // Mark tints: from the bar under the name down (or up) to the speed line or the ridge.
            for (m, (_, _, _, y)) in marks.iter().zip(placed.iter()) {
                {
                    let (a, b) = (m.start, m.end.max(m.start + dist * 0.003));
                    let mut v = vec![(x(a), y + 13.0), (x(b), y + 13.0)];
                    v.extend(if has_speed { s_line(a, b) } else { ridge(a, b) }.into_iter().rev());
                    rsx! { polygon { points: "{pts_str(&v)}", fill: "{m.color}", fill_opacity: 0.14 } }
                }
            }
            polyline { points: "{full_ridge}", fill: "none", stroke: "var(--muted)", stroke_width: 1.5, stroke_linejoin: "round" }
            for (pts, a, b, g) in slope_ridges.iter() {
                polyline { points: "{pts}", fill: "none", stroke: if *g > 0.0 { "var(--bad)" } else { "var(--good)" }, stroke_width: 3, stroke_linecap: "round", stroke_linejoin: "round" }
                if x(*b) - x(*a) > 14.0 {
                    text {
                        x: (x(*a) + x(*b)) / 2.0, y: slope_label_y(*a, *b), fill: "var(--ink)", font_size: 11,
                        text_anchor: "middle", stroke: "var(--surface)", stroke_width: 3, paint_order: "stroke", "{g:+.1}%"
                    }
                }
            }
            // Speed: m/s ticks on the right, the line, each mark's stretch of it in the mark's colour.
            if has_speed {
                for &v in sp_ticks.iter() {
                    line { x1: L, x2: cw - R, y1: sy(v), y2: sy(v), stroke: "var(--grid)", stroke_width: 1, stroke_dasharray: "2 4" }
                    text { x: cw - R, y: sy(v) - 2.0, fill: "var(--accent-2)", font_size: 9.5, text_anchor: "end", stroke: "var(--surface)", stroke_width: 3, paint_order: "stroke", "{v} m/s" }
                }
                polyline { points: "{pts_str(&s_line(0.0, dist))}", fill: "none", stroke: "var(--accent-2)", stroke_width: 1.8, stroke_linejoin: "round" }
                for m in marks.iter().filter(|m| m.end > m.start) {
                    polyline { points: "{pts_str(&s_line(m.start, m.end))}", fill: "none", stroke: "{m.color}", stroke_width: 3.4, stroke_linecap: "round", stroke_linejoin: "round" }
                }
            }
            // Straights / corners band.
            text { x: L - 8.0, y: lay_y + 9.0, fill: "var(--muted)", font_size: 11, text_anchor: "end", dominant_baseline: "central", "Layout" }
            for (i, &(a, b)) in straights.iter().enumerate() {
                rect {
                    x: x(a) + 1.0, y: lay_y, width: (x(b) - x(a) - 2.0).max(1.0), height: 18, rx: 4,
                    fill: "var(--accent-2)", fill_opacity: if Some(i) == final_straight { 0.6 } else { 0.3 }
                }
                if Some(i) == final_straight && fits(a, b, "Final straight") {
                    text { x: (x(a) + x(b)) / 2.0, y: lay_y + 9.0, fill: "var(--ink)", font_size: 11, text_anchor: "middle", dominant_baseline: "central", "Final straight" }
                } else if Some(i) != final_straight && fits(a, b, "Straight") {
                    text { x: (x(a) + x(b)) / 2.0, y: lay_y + 9.0, fill: "var(--ink)", font_size: 11, text_anchor: "middle", dominant_baseline: "central", "Straight" }
                }
            }
            for (i, &(a, b)) in corners.iter().enumerate() {
                rect { x: x(a) + 1.0, y: lay_y, width: (x(b) - x(a) - 2.0).max(1.0), height: 18, rx: 4, fill: "var(--muted)", fill_opacity: 0.4 }
                if fits(a, b, "C0") {
                    text { x: (x(a) + x(b)) / 2.0, y: lay_y + 9.0, fill: "var(--ink)", font_size: 11, text_anchor: "middle", dominant_baseline: "central", "C{i + 1}" }
                }
            }
            // Bands: a row each, label on the left, a bar per stretch with the band's icon in its middle.
            for (k, b) in bands.iter().enumerate() {
                text { x: L - 8.0, y: band_y + k as f32 * BAND + 7.0, fill: "var(--muted)", font_size: 11, text_anchor: "end", dominant_baseline: "central", "{b.label}" }
                for &(a, e) in b.stretches.iter() {
                    rect { x: x(a), y: band_y + k as f32 * BAND + 4.0, width: (x(e) - x(a)).max(2.0), height: 6, rx: 3, fill: "{b.color}" }
                }
            }
            for (k, cx) in band_markers(&bands, x, 16.0) {
                {band_icon(&bands[k], cx, band_y + k as f32 * BAND + 7.0, 7.0, false)}
            }
            if let Some(d) = hover() {
                line { x1: x(d), x2: x(d), y1: rg_y, y2: bot, stroke: "var(--ink)", stroke_width: 1, stroke_opacity: 0.6 }
                circle { cx: x(d), cy: ey(e_at(d)), r: 4, fill: "var(--ink)", stroke: "var(--surface)", stroke_width: 2 }
            }
            if let Some(s) = read { {readout(cw - R, 12.0, s)} }
            g { onmouseleave: move |_| hover.set(None),
                for i in 0..(pw / 4.0) as usize {
                    rect {
                        x: L + i as f32 * 4.0, y: rg_y, width: 4, height: bot - rg_y, fill: "transparent",
                        onmouseenter: move |_| hover.set(Some((i as f32 * 4.0 + 2.0) / pw * dist))
                    }
                }
            }
            // Mark names over the hover strip, so they keep their own hover: name, the bar under it, a dot where
            // it activated.
            for (m, (label, lx, anchor_side, y)) in marks.iter().zip(placed.iter()) {
                g {
                    title { "{m.tip}" }
                    text {
                        x: *lx, y: y + 10.0, fill: "var(--ink)", font_size: 11, font_weight: 700, text_anchor: "{anchor_side}",
                        stroke: "var(--surface)", stroke_width: 3, paint_order: "stroke", "{label}"
                    }
                    rect { x: x(m.start), y: y + 11.5, width: (x(m.end) - x(m.start)).max(3.0), height: 3.5, rx: 1.75, fill: "{m.color}" }
                    circle { cx: x(m.start), cy: anchor(m.start), r: 2.8, fill: "{m.color}", stroke: "var(--surface)", stroke_width: 1 }
                }
            }
        }
    }
}

/// Distance axis ticks (nice step, about one per 70 px), kept inside start..=distance.
pub(super) fn dist_ticks(start: f32, distance: f32, pw: f32) -> Vec<f32> {
    nice_domain(start, distance, (pw / 70.0).max(2.0) as usize).3.into_iter().filter(|&t| t >= start && t <= distance + 0.5).collect()
}
