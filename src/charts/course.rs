use super::*;

/// Phase boundaries: 0, the phase starts inside the course, `d`.
pub(super) fn phase_bounds(phases: &[f32], d: f32) -> Vec<f32> {
    std::iter::once(0.0).chain(phases.iter().copied().filter(|&p| p > 0.0 && p < d)).chain([d]).collect()
}

/// Activation distances binned finely (`bin` m) over the whole course, with the course's phases (late race and last
/// spurt tinted) and corners shaded behind, so the bars line up with the track. Hover shows the bin and count.
/// `start` = left end of the x axis (m), to zoom in on the end of the course (the axis then reads e.g. "1700 m").
/// `gains` (distance, lengths): dots on a right-hand lengths axis, and the bin's mean gain in the hover readout.
#[component]
pub fn CourseHist(
    distances: Vec<f32>, course_distance: f32, phases: Vec<f32>, corners: Vec<(f32, f32)>, bin: f32, color: String,
    #[props(default)] start: f32, #[props(default)] gains: Vec<(f32, f32)>,
) -> Element {
    let (mut size, cw) = use_width();
    let mut hover = use_signal(|| None::<usize>);
    const H: f32 = 190.0;
    const L: f32 = 36.0;
    let r = if gains.is_empty() { 14.0 } else { 46.0 };
    const T: f32 = 22.0;
    const B: f32 = 40.0;
    let d = course_distance.max(1.0);
    let s0 = start.clamp(0.0, d - 1.0);
    let bw = bin.max(1.0);
    let nb = ((d - s0) / bw).ceil().max(1.0) as usize;
    let counts = bin_counts(distances.iter().copied().filter(|v| v.is_finite() && *v >= s0), s0, bw, nb);
    let total = distances.len().max(1);
    let (pw, ph) = (cw - L - r, H - T - B);
    let x = move |m: f32| L + (m.clamp(s0, d) - s0) / (d - s0) * pw;
    let (cmax, cstep, cticks) = count_axis(*counts.iter().max().unwrap_or(&0));
    let y = move |c: f32| T + (1.0 - c / cmax) * ph;
    let bounds = phase_bounds(&phases, d);
    let ticks = dist_ticks(s0, d, pw);
    let g_lo = gains.iter().map(|g| g.1).fold(0.0, f32::min);
    let g_hi = gains.iter().map(|g| g.1).fold(0.0, f32::max);
    let (g_lo, g_hi, g_step, g_ticks) = nice_domain(g_lo, g_hi, 4);
    let gy = move |v: f32| T + (1.0 - (v - g_lo) / (g_hi - g_lo)) * ph;
    let read = hover().filter(|&i| i < nb).map(|i| {
        let a = s0 + i as f32 * bw;
        let s = format!("{a:.0} to {:.0} m: {} ({:.1}%)", a + bw, counts[i], 100.0 * counts[i] as f32 / total as f32);
        let g: Vec<f32> = gains.iter().filter(|g| g.0 >= a && g.0 < a + bw).map(|g| g.1).collect();
        if g.is_empty() { s } else { format!("{s}, mean {:+.2} L", g.iter().sum::<f32>() / g.len() as f32) }
    });
    rsx! {
        svg { view_box: "0 0 {cw} {H}", style: SVG_STYLE, onresize: move |e| track(&mut size, e),
            // Phase tint and labels along the bottom, corners as grey bands under the axis.
            for (i, w) in bounds.windows(2).enumerate().filter(|(_, w)| w[1] > s0) {
                rect { x: x(w[0]), y: T, width: (x(w[1]) - x(w[0])).max(0.0), height: ph, fill: if i >= 2 { "var(--accent)" } else { "transparent" }, fill_opacity: if i == 2 { 0.08 } else { 0.14 } }
                if x(w[1]) - x(w[0]) > PHASES[i.min(3)].len() as f32 * 6.5 {
                    text { x: (x(w[0]) + x(w[1])) / 2.0, y: H - 6.0, fill: "var(--muted)", font_size: 10.5, text_anchor: "middle", "{PHASES[i.min(3)]}" }
                }
                if i > 0 && w[0] > s0 {
                    line { x1: x(w[0]), x2: x(w[0]), y1: T, y2: T + ph + 18.0, stroke: "var(--muted)", stroke_width: 1, stroke_dasharray: "3 3" }
                }
            }
            if s0 > 0.0 {
                text { x: cw - r, y: T - 8.0, fill: "var(--ink)", font_size: 11, font_weight: 700, text_anchor: "end", "Goal" }
                line { x1: cw - r, x2: cw - r, y1: T - 4.0, y2: T + ph, stroke: "var(--ink)", stroke_width: 1.5 }
            }
            for &(a, b) in corners.iter().filter(|c| c.1 > s0) {
                rect { x: x(a), y: T + ph + 2.0, width: (x(b) - x(a)).max(1.0), height: 5, rx: 2, fill: "var(--muted)", fill_opacity: 0.5 }
            }
            {y_grid(&cticks, y, L, cw - r, |t| fmt_tick(t, cstep, false))}
            {x_labels(&ticks, x, T + ph + 18.0, cw - r - 24.0, |t| format!("{t} m"))}
            for (i, &c) in counts.iter().enumerate() {
                if c > 0 {
                    rect { x: x(s0 + i as f32 * bw), y: y(c as f32), width: (pw / nb as f32).max(1.0), height: y(0.0) - y(c as f32), fill: "{color}", fill_opacity: if hover() == Some(i) { 0.7 } else { 1.0 } }
                }
            }
            line { x1: L, x2: cw - r, y1: y(0.0), y2: y(0.0), stroke: "var(--muted)", stroke_width: 1 }
            if !gains.is_empty() {
                for &t in &g_ticks {
                    text { x: cw - r + 6.0, y: gy(t), fill: "var(--muted)", font_size: 11, dominant_baseline: "middle", "{fmt_tick(t, g_step, false)} L" }
                }
                if g_lo < 0.0 {
                    line { x1: L, x2: cw - r, y1: gy(0.0), y2: gy(0.0), stroke: "var(--ink)", stroke_width: 1, stroke_dasharray: "3 3", stroke_opacity: 0.5 }
                }
                for &(d, g) in gains.iter().filter(|g| g.0 >= s0) {
                    circle { cx: x(d), cy: gy(g), r: 2.2, fill: "var(--ink)", fill_opacity: 0.45 }
                }
            }
            if let Some(s) = read { {readout(cw - r, 12.0, s)} }
            g { onmouseleave: move |_| hover.set(None),
                for i in 0..nb {
                    rect { x: x(s0 + i as f32 * bw), y: T, width: (pw / nb as f32).max(1.0), height: ph, fill: "transparent", onmouseenter: move |_| hover.set(Some(i)) }
                }
            }
        }
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
///   scale exaggerated (factor shown on the left). Empty = derived from `slopes`.
/// - `corners`, `straights`: (start m, end m); corners numbered, the last straight labelled as the final straight.
/// - `slopes`: (start m, end m, grade %), + = uphill; tinted on the silhouette ridge and labelled.
/// - `phases`: start m of Early race, Mid race, Late race, Last spurt (a leading 0 is optional).
/// - `regions`: (start m, end m, label, colour) activation bars, packed into lanes above the track.
/// Final leg (from `final_leg_start`) is pink. Hover shows metres, phase, corner/straight, grade and regions there.
#[component]
pub fn CourseProfile(
    distance: f32, elevation: Vec<f32>, corners: Vec<(f32, f32)>, straights: Vec<(f32, f32)>,
    slopes: Vec<(f32, f32, f32)>, phases: Vec<f32>, final_leg_start: f32, regions: Vec<(f32, f32, String, String)>,
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
        let (a, b, label, _) = &regions[i];
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
    let h = lay_y + 18.0 + 24.0;

    let elev = elevation_or_slopes(&elevation, &slopes, dist);
    let (emin, emax) = elev.iter().fold((f32::MAX, f32::MIN), |(a, b), &e| (a.min(e), b.max(e)));
    let vs = EH * 0.7 / (emax - emin).max(1.0); // px per metre of height; >= 1 m range so flat courses stay flat
    let exaggeration = vs / (pw / dist);
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
        parts.extend(regions.iter().filter(|r| (r.0..=r.1).contains(&d)).map(|r| r.2.clone()));
        parts.join(" · ")
    });

    rsx! {
        svg { view_box: "0 0 {cw} {h}", style: SVG_STYLE, onresize: move |e| track(&mut size, e),
            for &t in ticks.iter() {
                line { x1: x(t), x2: x(t), y1: el_y, y2: lay_y + 18.0, stroke: "var(--grid)", stroke_width: 1 }
            }
            {x_labels(&ticks, x, h - 7.0, cw - R - 24.0, |t| format!("{t} m"))}
            // Regions (activation bars) above the track.
            for (i, (a, b, label, c)) in regions.iter().enumerate() {
                text { x: lanes[i].1, y: rg_y + lanes[i].0 as f32 * LANE + 9.0, fill: "var(--ink)", font_size: 11, "{label}" }
                rect { x: x(*a), y: rg_y + lanes[i].0 as f32 * LANE + 12.0, width: (x(*b) - x(*a)).max(4.0), height: 5, rx: 2.5, fill: "{c}" }
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
                    line { x1: x(w[0]), x2: x(w[0]), y1: ph_y, y2: lay_y + 18.0, stroke: "var(--muted)", stroke_width: 1, stroke_dasharray: "3 3" }
                }
            }
            // Elevation silhouette; final leg in pink, slopes tinted on the ridge.
            polygon { points: "{full_area}", fill: "var(--muted)", fill_opacity: 0.2 }
            polygon { points: "{final_area}", fill: "var(--accent)", fill_opacity: 0.3 }
            polyline { points: "{full_ridge}", fill: "none", stroke: "var(--muted)", stroke_width: 1.5, stroke_linejoin: "round" }
            for (pts, a, b, g) in slope_ridges.iter() {
                polyline { points: "{pts}", fill: "none", stroke: if *g > 0.0 { "var(--bad)" } else { "var(--good)" }, stroke_width: 3, stroke_linecap: "round", stroke_linejoin: "round" }
                if x(*b) - x(*a) > 14.0 {
                    text {
                        x: (x(*a) + x(*b)) / 2.0, y: ey(e_at((a + b) / 2.0).max(e_at(*a)).max(e_at(*b))) - 7.0, fill: "var(--ink)", font_size: 11,
                        text_anchor: "middle", stroke: "var(--surface)", stroke_width: 3, paint_order: "stroke", "{g:+.1}%"
                    }
                }
            }
            g {
                title { "Heights are drawn {exaggeration:.0} times steeper than distances, so slopes are visible" }
                text { x: L - 8.0, y: el_y + EH / 2.0 - 7.0, fill: "var(--muted)", font_size: 11, text_anchor: "end", "Elevation" }
                text { x: L - 8.0, y: el_y + EH / 2.0 + 8.0, fill: "var(--muted)", font_size: 11, text_anchor: "end", if emax - emin < 0.3 { "(flat)" } else { "(×{exaggeration:.0})" } }
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
            if let Some(d) = hover() {
                line { x1: x(d), x2: x(d), y1: rg_y, y2: lay_y + 18.0, stroke: "var(--ink)", stroke_width: 1, stroke_opacity: 0.6 }
                circle { cx: x(d), cy: ey(e_at(d)), r: 4, fill: "var(--ink)", stroke: "var(--surface)", stroke_width: 2 }
            }
            if let Some(s) = read { {readout(cw - R, 12.0, s)} }
            g { onmouseleave: move |_| hover.set(None),
                for i in 0..(pw / 4.0) as usize {
                    rect {
                        x: L + i as f32 * 4.0, y: rg_y, width: 4, height: lay_y + 18.0 - rg_y, fill: "transparent",
                        onmouseenter: move |_| hover.set(Some((i as f32 * 4.0 + 2.0) / pw * dist))
                    }
                }
            }
        }
    }
}

/// Distance axis ticks (nice step, about one per 70 px), kept inside start..=distance.
pub(super) fn dist_ticks(start: f32, distance: f32, pw: f32) -> Vec<f32> {
    nice_domain(start, distance, (pw / 70.0).max(2.0) as usize).3.into_iter().filter(|&t| t >= start && t <= distance + 0.5).collect()
}
