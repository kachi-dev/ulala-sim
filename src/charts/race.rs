use super::*;

/// One value over the course: 101 points (x = 0..=100 % of the course), plus nested (lo, hi) bands, outermost first,
/// drawn stacked so the colour builds toward the middle (batch: p10-p90 fading in to p40-p60).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Trace {
    pub line: Vec<f32>,
    pub fan: Vec<(Vec<f32>, Vec<f32>)>,
}

/// Place, speed (m/s) and HP of one uma over the course.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RaceTraces {
    pub place: Trace,
    pub speed: Trace,
    pub hp: Trace,
}

const KEYS: [(&str, &str); 3] = [("Position", "var(--accent)"), ("Speed", "var(--accent-2)"), ("HP", "var(--good)")];

fn traces(r: &RaceTraces) -> [&Trace; 3] {
    [&r.place, &r.speed, &r.hp]
}

/// Moving average over ±2 points: whole places every 1 % are steps; this draws them as a curve.
fn soften(v: &[f32]) -> Vec<f32> {
    (0..v.len()).map(|i| {
        let w = &v[i.saturating_sub(2)..(i + 3).min(v.len())];
        w.iter().sum::<f32>() / w.len() as f32
    }).collect()
}

/// Place (left axis, 1 at the top), speed and HP (right axes) of one uma on one course axis, as curves, over the
/// course strip of `CourseHist` (phases, corners, straights). Tabs fade all but one value; `base` is drawn dashed
/// (toggle). Hover reads all three at that point. `bands` are drawn on the plot as tinted stretches with their icon in
/// a lane at the top; "Events" picks the current run's or `base_bands` (the baseline's), or none.
#[component]
pub fn RaceChart(
    cur: RaceTraces, base: Option<RaceTraces>, field_size: usize, course_distance: f32, phases: Vec<f32>,
    corners: Vec<(f32, f32)>, straights: Vec<(f32, f32)>, #[props(default)] bands: Vec<Band>,
    #[props(default)] base_bands: Vec<Band>,
) -> Element {
    let (mut size, cw) = use_width();
    let mut lens = use_signal(|| None::<usize>);
    let mut show_base = use_signal(|| true);
    // Events shown: the current run's, the baseline's (`Some(false)`) or none.
    let mut events = use_signal(|| Some(true));
    let has_events = !bands.is_empty() || !base_bands.is_empty();
    let ghost = events() == Some(false) && base.is_some();
    let bands = match events() {
        Some(true) => bands,
        Some(false) if base.is_some() => base_bands,
        _ => vec![],
    };
    let mut hover = use_signal(|| None::<usize>);
    const H: f32 = 340.0;
    const L: f32 = 30.0;
    const R: f32 = 92.0;
    const T: f32 = 22.0;
    const B: f32 = 46.0;
    let (pw, ph) = (cw - L - R, H - T - B);
    let d = course_distance.max(1.0);
    let x = move |pct: f32| L + pct.clamp(0.0, 100.0) / 100.0 * pw;
    let xm = move |m: f32| x(m / d * 100.0);
    // Event icons packed into lanes from the top of the plot: the first lane where one clears the previous icon.
    // (band, x, lane)
    const ICON: f32 = 22.0;
    let mut lane_end: Vec<f32> = vec![];
    let icons: Vec<(usize, f32, usize)> = band_markers(&bands, xm, ICON + 3.0).into_iter().map(|(k, cx)| {
        let lane = lane_end.iter().position(|&e| e <= cx).unwrap_or_else(|| { lane_end.push(f32::MIN); lane_end.len() - 1 });
        lane_end[lane] = cx + ICON + 3.0;
        (k, cx, lane)
    }).collect();

    // Right axes from everything drawn; speed skips the start dash (0 → race speed in the first few %).
    let drawn: Vec<&RaceTraces> = std::iter::once(&cur).chain(base.as_ref()).collect();
    let values = |k: usize, from: usize| -> Vec<f32> {
        drawn.iter().flat_map(|r| {
            let t = traces(r)[k];
            std::iter::once(&t.line).chain(t.fan.iter().flat_map(|(a, b)| [a, b])).flat_map(|v| v.iter().skip(from).copied())
        }).collect()
    };
    let (smin, smax) = values(1, 5).into_iter().fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(v), b.max(v)));
    let (slo, shi, sstep, sticks) = nice_domain(smin.min(smax), smax.max(smin + 1.0), 4);
    let (_, hhi, _, hticks) = nice_domain(0.0, values(2, 0).into_iter().fold(1.0, f32::max), 4);
    let n = field_size.max(2) as f32;
    let y = move |k: usize, v: f32| match k {
        0 => T + (v.clamp(1.0, n) - 1.0) / (n - 1.0) * ph,
        1 => T + (1.0 - (v.clamp(slo, shi) - slo) / (shi - slo)) * ph,
        _ => T + (1.0 - v.clamp(0.0, hhi) / hhi) * ph,
    };
    let pts = |k: usize, v: &[f32]| {
        let v = if k == 0 { soften(v) } else { v.to_vec() };
        let s = 100.0 / (v.len().max(2) - 1) as f32;
        v.iter().enumerate().map(|(i, &p)| (x(i as f32 * s), y(k, p))).collect::<Vec<_>>()
    };
    let band = |k: usize, (lo, hi): &(Vec<f32>, Vec<f32>)| {
        let back: Vec<_> = pts(k, lo).into_iter().rev().collect();
        format!("{}{}Z", curve(&pts(k, hi), false), curve(&back, true))
    };
    // Back to front: HP, speed, place. (key, opacity, fan paths, baseline path, line path)
    let on = |k: usize| lens().is_none_or(|l| l == k);
    let layers: Vec<_> = [2, 1, 0].into_iter().map(|k| (
        k,
        if on(k) { 1.0 } else { 0.15 },
        traces(&cur)[k].fan.iter().map(|b| band(k, b)).collect::<Vec<_>>(),
        base.as_ref().filter(|_| show_base()).map(|b| curve(&pts(k, &traces(b)[k].line), false)),
        curve(&pts(k, &traces(&cur)[k].line), false),
    )).collect();
    let axis_ink = |k: usize| if on(k) { KEYS[k].1 } else { "var(--muted)" };
    let places: Vec<f32> = (1..=n as usize).filter(|p| n <= 12.0 || (p - 1) % 2 == 0).map(|p| p as f32).collect();
    let at = |v: &[f32], i: usize| v.get(i * (v.len().max(1) - 1) / 100).copied().unwrap_or(0.0);
    let read = hover().map(|i| {
        let [p, s, h] = traces(&cur).map(|t| at(&t.line, i));
        let m = i as f32 / 100.0 * d;
        let states: String =
            bands.iter().filter(|b| b.stretches.iter().any(|r| (r.0..=r.1).contains(&m))).map(|b| format!(" · {}", b.label)).collect();
        (i, format!("{m:.0} m · place {} · {s:.2} m/s · {h:.0} HP{states}", fmt_place(p)), [p, s, h])
    });

    rsx! {
        div {
            div { class: "race-h",
                div { class: "seg small",
                    button { class: if lens().is_none() { "on" }, onclick: move |_| lens.set(None), "Combined" }
                    for (k, (label, c)) in KEYS.iter().enumerate() {
                        button { class: if lens() == Some(k) { "on" }, onclick: move |_| lens.set(Some(k)),
                            i { class: "dot", style: "background:{c}" }
                            "{label}"
                        }
                    }
                }
                if base.is_some() {
                    button { class: if show_base() { "chip-btn on" } else { "chip-btn" }, onclick: move |_| show_base.toggle(), "Baseline" }
                }
                // Events: Baseline | Current; clicking the one shown turns them off.
                if has_events {
                    span { class: "seg-label", "Events" }
                    div { class: "seg small",
                        if base.is_some() {
                            button {
                                class: if events() == Some(false) { "on" },
                                onclick: move |_| events.set(if events() == Some(false) { None } else { Some(false) }),
                                "Baseline"
                            }
                        }
                        button {
                            class: if events() == Some(true) { "on" },
                            onclick: move |_| events.set(if events() == Some(true) { None } else { Some(true) }),
                            "Current"
                        }
                    }
                }
            }
            svg { view_box: "0 0 {cw} {H}", style: SVG_STYLE, onresize: move |e| track(&mut size, e),
                {course_axis(xm, &phase_bounds(&phases, d), 0.0, T, T + ph, H, cw - R, &corners, &straights, &dist_ticks(0.0, d, pw), false)}
                // Event stretches behind the curves: a tint over the plot and a solid strip along its bottom; the
                // baseline's hatched in a dashed outline, like its dashed lines.
                if ghost {
                    {band_hatches("rc-h", &bands)}
                }
                for (k, b) in bands.iter().enumerate() {
                    for &(a, e) in b.stretches.iter() {
                        if ghost {
                            rect {
                                x: xm(a), y: T + 0.5, width: (xm(e) - xm(a)).max(2.0), height: ph - 1.0, fill: "url(#rc-h-{k})", fill_opacity: 0.45,
                                stroke: "{b.color}", stroke_width: 1.2, stroke_dasharray: "5 4", stroke_opacity: 0.9
                            }
                        } else {
                            rect { x: xm(a), y: T, width: (xm(e) - xm(a)).max(2.0), height: ph, fill: "{b.color}", fill_opacity: 0.14 }
                            rect { x: xm(a), y: T + ph - 3.0, width: (xm(e) - xm(a)).max(2.0), height: 3, fill: "{b.color}" }
                        }
                    }
                }
                {y_grid(&places, |v| y(0, v), L, cw - R, |p| format!("{p}"))}
                for &t in sticks.iter() {
                    text { x: cw - R + 8.0, y: y(1, t), fill: axis_ink(1), font_size: 11, dominant_baseline: "middle", "{fmt_tick(t, sstep, false)}" }
                }
                for &t in hticks.iter() {
                    text { x: cw - 6.0, y: y(2, t), fill: axis_ink(2), font_size: 11, text_anchor: "end", dominant_baseline: "middle", "{t}" }
                }
                text { x: L - 6.0, y: T - 9.0, fill: axis_ink(0), font_size: 11, font_weight: 700, text_anchor: "end", "Place" }
                text { x: cw - R + 8.0, y: T - 9.0, fill: axis_ink(1), font_size: 11, font_weight: 700, "m/s" }
                text { x: cw - 6.0, y: T - 9.0, fill: axis_ink(2), font_size: 11, font_weight: 700, text_anchor: "end", "HP" }
                for (k, o, fan, dashed, line) in layers.iter() {
                    g { opacity: *o,
                        for b in fan.iter() {
                            path { d: "{b}", fill: KEYS[*k].1, fill_opacity: 0.09 }
                        }
                        if let Some(p) = dashed {
                            path { d: "{p}", fill: "none", stroke: KEYS[*k].1, stroke_width: 1.5, stroke_opacity: 0.8, stroke_dasharray: "6 4" }
                        }
                        path { d: "{line}", fill: "none", stroke: KEYS[*k].1, stroke_width: if *k == 0 { 2.5 } else { 2.0 }, stroke_linecap: "round" }
                    }
                }
                // Event icons over the curves.
                for &(k, cx, lane) in icons.iter() {
                    {band_icon(&bands[k], cx, T + 4.0 + lane as f32 * (ICON + 3.0) + ICON / 2.0, ICON / 2.0, ghost)}
                }
                if let Some((i, s, v)) = read {
                    line { x1: x(i as f32), x2: x(i as f32), y1: T, y2: T + ph, stroke: "var(--ink)", stroke_width: 1, stroke_opacity: 0.5 }
                    for k in 0..3 {
                        circle { cx: x(i as f32), cy: y(k, v[k]), r: 4, fill: KEYS[k].1, stroke: "var(--surface)", stroke_width: 2 }
                    }
                    {readout(cw - R, 12.0, s)}
                }
                g { onmouseleave: move |_| hover.set(None),
                    for i in 0..=100usize {
                        rect {
                            x: x(i as f32 - 0.5).max(L), y: T, width: pw / 100.0, height: ph, fill: "transparent",
                            onmouseenter: move |_| hover.set(Some(i))
                        }
                    }
                }
            }
        }
    }
}
