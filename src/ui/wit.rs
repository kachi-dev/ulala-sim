//! Wit subtab of the batch uma detail (only for an uma whose Wit differs from the baseline): what the Wit change does
//! through each mechanic that reads Wit, one at a time, on the run's field and seeds.

use super::*;
use crate::charts::{GainChart, Histogram};

/// Horse `i`'s Wit on the baseline, when it differs from `input`'s.
pub(super) fn old_wit(s: S, input: &RaceInput, i: usize) -> Option<i32> {
    let base = s.baseline.read();
    let b = base.as_deref().filter(|b| comparable(b, input))?;
    let (old, new) = (b.horses.get(i)?.wiz, input.horses.get(i)?.wiz);
    (old != new).then_some(old)
}

/// Mean of `f` over the races of `v`: (old Wit, new Wit).
fn means(v: &[WitPair], f: impl Fn(&WitSample) -> f32) -> (f32, f32) {
    let n = v.len().max(1) as f32;
    (v.iter().map(|p| f(&p.old)).sum::<f32>() / n, v.iter().map(|p| f(&p.new)).sum::<f32>() / n)
}

/// Mean lengths gained.
fn gain(v: &[WitPair]) -> f32 {
    v.iter().map(|p| p.gain).sum::<f32>() / v.len().max(1) as f32
}

/// The races `v` changed: gain != 0 (a pair that never diverged finishes exactly level).
fn changed(v: &[WitPair]) -> Vec<f32> {
    v.iter().map(|p| p.gain).filter(|&l| l != 0.0).collect()
}

/// [p10, p25, p50, p75, p90, mean] of the gains, for `GainChart`.
fn box_stats(v: &[WitPair]) -> [f32; 6] {
    let d = dist(v.iter().map(|p| p.gain).collect());
    [d.p10, d.p25, d.p50, d.p75, d.p90, d.mean]
}

/// Horse `i` of the batch on screen, whose baseline Wit was `old`: the Simulate button (beside the detail's subtabs).
#[component]
pub(super) fn WitRun(i: usize, old: i32) -> Element {
    let s = use_context::<S>();
    let running = use_busy(s);
    let res = s.res.cloned();
    let Some((input, seeds)) = res.and_then(|r| match r.kind {
        Kind::Batch { cur, .. } => Some((r.input, cur.seeds.clone())),
        Kind::Single { .. } => None,
    }) else {
        return rsx! {};
    };
    let new = input.horses[i].wiz;
    // Per seed: the baseline, then each mechanic on the new Wit, then all of them.
    let runs = seeds.len() * (WIT_MECHANICS.len() + 2);
    let work = runs * input.horses.len();
    let eta = eta(s, work);
    let n = seeds.len();
    let start = move |_| {
        let Some(race) = s.race.cloned() else { return };
        let (input, seeds) = (input.clone(), seeds.clone());
        let rev = *s.rev.peek();
        let label = format!("Simulating {} races", thousands(runs));
        let run = move |w: &Work| Arc::new(wit(&race, &input, i, old, &seeds, w));
        measure(s, label, n, work, run, move |mut s, wit| {
            s.w_sel.set(None);
            s.w_res.set(Some(WitRes { horse: i, old, new, wit, rev }));
        });
    };
    rsx! {
        button { class: "btn small", disabled: running(), onclick: start, "Simulate {thousands(n)} races" }
        span { class: "chip", "Wit {old} → {new}" }
        span { class: "muted small", "{thousands(runs)} races, {eta}" }
        Progress {}
    }
}

/// The measurement of horse `i` (Wit `old` on the baseline), once simulated.
#[component]
pub(super) fn WitOut(i: usize, old: i32) -> Element {
    let s = use_context::<S>();
    let new = s.res.read().as_ref().map_or(0, |r| r.input.horses[i].wiz);
    match s.w_res.cloned().filter(|r| (r.horse, r.old, r.new) == (i, old, new)) {
        Some(r) => rsx! { WitResults { r } },
        None => rsx! {},
    }
}

#[component]
fn WitResults(r: WitRes) -> Element {
    let s = use_context::<S>();
    let mut wsel = s.w_sel;
    let rev = r.rev;
    let stale = use_stale(s, move || Some(rev));
    let rows = &r.wit.rows;
    let all = &rows[WIT_MECHANICS.len()];
    let full = |v: &[WitPair]| means(v, |x| x.full_spurt as u8 as f32);
    let rushed = |v: &[WitPair]| means(v, |x| x.rushed as u8 as f32);
    let skills = |v: &[WitPair]| means(v, |x| x.skills as f32);
    let secs = |v: &[WitPair], m: usize| means(v, |x| x.modes[m]);
    // Change in seconds in mode `m`, old to new Wit. More downhill accel is good; the position keep modes are neither.
    let change = |m: usize, v: &[WitPair]| -> Element {
        let (o, n) = secs(v, m);
        let d = n - o;
        if m == 0 { delta(d, false, format!("{:.2} s", d.abs())) } else { rsx! { span { class: "d flat", "{d:+.2} s" } } }
    };
    let mode = |m: usize, v: &[WitPair]| rsx! { "{WIT_MODES[m].1} {secs(v, m).1:.2} s " {change(m, v)} };
    // Modes the uma is ever in (a Front runner never paces up or down, the others never speed up or overtake).
    let used: Vec<usize> = (0..WIT_MODES.len()).filter(|&m| all.iter().any(|p| p.new.modes[m] > 0.0 || p.old.modes[m] > 0.0)).collect();
    let pk: Vec<usize> = used.iter().copied().filter(|&m| m > 0).collect();
    // `(old, new)` as "▲ 3.8 pp".
    let pp = |(o, n): (f32, f32), lower_better: bool| delta(n - o, lower_better, format!("{:.1} pp", (n - o).abs() * 100.0));
    let total = gain(all);
    // 95% range of the mean.
    let sd = (all.iter().map(|p| (p.gain - total).powi(2)).sum::<f32>() / (all.len().max(2) - 1) as f32).sqrt();
    let ci = 1.96 * sd / (all.len().max(1) as f32).sqrt();
    // Mechanics by mean gain, most first.
    let mut order: Vec<usize> = (0..WIT_MECHANICS.len()).collect();
    order.sort_by(|&a, &b| gain(&rows[b]).total_cmp(&gain(&rows[a])));
    let row = |name: &str, color: &str, v: &[WitPair]| (name.to_string(), color.to_string(), box_stats(v), v.len());
    let mut chart: Vec<(String, String, [f32; 6], usize)> = order.iter().map(|&k| row(WIT_MECHANICS[k], "var(--accent)", &rows[k])).collect();
    chart.push(row("Total", "var(--accent-2)", all));
    // Per mechanic: what it changes besides lengths, old to new Wit.
    let effect = |k: usize| -> Element {
        let v = &rows[k];
        match WIT_MECHANICS[k] {
            "Downhill" if used.contains(&0) => mode(0, v),
            "Position keep" if !pk.is_empty() => rsx! {
                for (j, &m) in pk.iter().enumerate() {
                    if j > 0 { " · " }
                    {mode(m, v)}
                }
            },
            // Wit only picks among the shorter spurts (full spurt is decided by HP first): where they started.
            "Spurt candidates" => {
                let p: Vec<(f32, f32)> = v.iter().filter(|p| p.gain != 0.0 && p.new.spurt_start >= 0.0 && p.old.spurt_start >= 0.0).map(|p| (p.new.spurt_start, p.old.spurt_start)).collect();
                if p.is_empty() {
                    rsx! { span { class: "muted", "-" } }
                } else {
                    let (a, b) = (p.iter().map(|x| x.0).sum::<f32>() / p.len() as f32, p.iter().map(|x| x.1).sum::<f32>() / p.len() as f32);
                    rsx! { "Spurt start {a:.0} m " {delta(a - b, true, format!("{:.0} m", (a - b).abs()))} }
                }
            }
            "Rushed" => rsx! { "Rushed {pct(rushed(v).1)} " {pp(rushed(v), true)} },
            "Skills" => {
                let (o, n) = skills(v);
                rsx! { "{n:.2} skills per race " {delta(n - o, false, format!("{:.2}", (n - o).abs()))} }
            }
            _ => rsx! { span { class: "muted", "-" } },
        }
    };
    let (fo, fnew) = full(all);
    let (ro, rn) = rushed(all);
    rsx! {
        if stale() {
            div { class: "stale", "The setup changed since this run. Simulate again to update it." }
        }
        div { class: "kpis",
            div { b { "{total:+.2} L" } div { class: "kpi2", span { "lengths" } span { title: "95% range of the mean", "{total - ci:+.2} to {total + ci:+.2}" } } }
            div { b { "{pct(fo)} → {pct(fnew)}" } div { class: "kpi2", span { "full spurt" } {pp(full(all), false)} } }
            div { b { "{pct(ro)} → {pct(rn)}" } div { class: "kpi2", span { "rushed" } {pp(rushed(all), true)} } }
            for &m in used.iter() {
                div { key: "{m}",
                    b { "{secs(all, m).0:.2} → {secs(all, m).1:.2} s" }
                    div { class: "kpi2", span { "{WIT_MODES[m].1.to_lowercase()}" } {change(m, all)} }
                }
            }
        }
        h4 { "Lengths gained per mechanic" }
        GainChart { rows: chart }
        h4 { "Mechanics" }
        div { class: "scroll-x",
            table { class: "grid",
                thead {
                    tr {
                        th { "Mechanic" }
                        th { class: "num", "Mean" }
                        th { class: "num", "Races changed" }
                        th { "Effect" }
                    }
                }
                tbody {
                    for k in order {
                        {
                            let v = &rows[k];
                            let changed = changed(v);
                            let open = *wsel.read() == Some(k);
                            rsx! {
                                tr { key: "{k}", class: if open { "sel" },
                                    onclick: move |_| wsel.set(if open { None } else { Some(k) }),
                                    td { b { "{WIT_MECHANICS[k]}" } }
                                    td { class: "num", "{gain(v):+.2} L" }
                                    td { class: "num", "{pct(changed.len() as f32 / v.len().max(1) as f32)}" }
                                    td { {effect(k)} }
                                }
                                if open {
                                    tr { key: "{k}-detail", class: "expand",
                                        td { colspan: "4",
                                            div { class: "gdetail",
                                                if changed.is_empty() {
                                                    p { class: "muted", "No effect here." }
                                                } else {
                                                    h4 { "Gain per changed race (lengths)" }
                                                    Histogram { values: changed, color: "var(--accent)".to_string(), unit: "L".to_string() }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
