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

/// Mean of `f` over samples.
fn mean(v: &[WitSample], f: impl Fn(&WitSample) -> f32) -> f32 {
    v.iter().map(f).sum::<f32>() / v.len().max(1) as f32
}

/// The races `v` changed: lengths != 0 (a pair that never diverged finishes exactly level).
fn changed(v: &[WitSample]) -> Vec<f32> {
    v.iter().map(|x| x.lengths).filter(|&l| l != 0.0).collect()
}

/// [p10, p25, p50, p75, p90, mean] of the lengths, for `GainChart`.
fn box_stats(v: &[WitSample]) -> [f32; 6] {
    let d = dist(v.iter().map(|x| x.lengths).collect());
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
    // Per seed: as set up, each mechanic on the old Wit, all of them.
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
    let (cur, alt) = (&r.wit.cur, &r.wit.alt);
    let all = &alt[WIT_MECHANICS.len()];
    let full = |v: &[WitSample]| mean(v, |x| x.full_spurt as u8 as f32);
    let rushed = |v: &[WitSample]| mean(v, |x| x.rushed as u8 as f32);
    let skills = |v: &[WitSample]| mean(v, |x| x.skills as f32);
    let secs = |v: &[WitSample], m: usize| mean(v, |x| x.modes[m]);
    // Change in seconds in mode `m` as set up vs `v`. More downhill accel is good; the position keep modes are neither.
    let change = |m: usize, v: &[WitSample]| -> Element {
        let d = secs(cur, m) - secs(v, m);
        if m == 0 { delta(d, false, format!("{:.2} s", d.abs())) } else { rsx! { span { class: "d flat", "{d:+.2} s" } } }
    };
    let mode = |m: usize, v: &[WitSample]| rsx! { "{WIT_MODES[m].1} {secs(cur, m):.2} s " {change(m, v)} };
    // Modes the uma is ever in (a Front runner never paces up or down, the others never speed up or overtake).
    let used: Vec<usize> = (0..WIT_MODES.len()).filter(|&m| cur.iter().chain(all).any(|x| x.modes[m] > 0.0)).collect();
    let pk: Vec<usize> = used.iter().copied().filter(|&m| m > 0).collect();
    let pp = |a: f32, b: f32, lower_better: bool| delta(a - b, lower_better, format!("{:.1} pp", (a - b).abs() * 100.0));
    let len = |v: &[WitSample]| mean(v, |x| x.lengths);
    let total = len(all);
    // 95% range of the mean.
    let sd = (all.iter().map(|x| (x.lengths - total).powi(2)).sum::<f32>() / (all.len().max(2) - 1) as f32).sqrt();
    let ci = 1.96 * sd / (all.len().max(1) as f32).sqrt();
    // Mechanics by mean gain, most first.
    let mut order: Vec<usize> = (0..WIT_MECHANICS.len()).collect();
    order.sort_by(|&a, &b| len(&alt[b]).total_cmp(&len(&alt[a])));
    let row = |name: &str, color: &str, v: &[WitSample]| (name.to_string(), color.to_string(), box_stats(v), v.len());
    let mut chart: Vec<(String, String, [f32; 6], usize)> = order.iter().map(|&k| row(WIT_MECHANICS[k], "var(--accent)", &alt[k])).collect();
    chart.push(row("Total", "var(--accent-2)", all));
    // Per mechanic: what it changes besides lengths, as set up vs on the old Wit.
    let effect = |k: usize| -> Element {
        let v = &alt[k];
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
                let p: Vec<(f32, f32)> = cur.iter().zip(v).filter(|(a, b)| b.lengths != 0.0 && a.spurt_start >= 0.0 && b.spurt_start >= 0.0).map(|(a, b)| (a.spurt_start, b.spurt_start)).collect();
                if p.is_empty() {
                    rsx! { span { class: "muted", "-" } }
                } else {
                    let (a, b) = (p.iter().map(|x| x.0).sum::<f32>() / p.len() as f32, p.iter().map(|x| x.1).sum::<f32>() / p.len() as f32);
                    rsx! { "Spurt start {a:.0} m " {delta(a - b, true, format!("{:.0} m", (a - b).abs()))} }
                }
            }
            "Rushed" => rsx! { "Rushed {pct(rushed(cur))} " {pp(rushed(cur), rushed(v), true)} },
            "Skills" => {
                let d = skills(cur) - skills(v);
                rsx! { "{skills(cur):.2} skills per race " {delta(d, false, format!("{:.2}", d.abs()))} }
            }
            _ => rsx! { span { class: "muted", "-" } },
        }
    };
    rsx! {
        if stale() {
            div { class: "stale", "The setup changed since this run. Simulate again to update it." }
        }
        div { class: "kpis",
            div { b { "{total:+.2} L" } div { class: "kpi2", span { "lengths" } span { title: "95% range of the mean", "{total - ci:+.2} to {total + ci:+.2}" } } }
            div { b { "{pct(full(all))} → {pct(full(cur))}" } div { class: "kpi2", span { "full spurt" } {pp(full(cur), full(all), false)} } }
            div { b { "{pct(rushed(all))} → {pct(rushed(cur))}" } div { class: "kpi2", span { "rushed" } {pp(rushed(cur), rushed(all), true)} } }
            for &m in used.iter() {
                div { key: "{m}",
                    b { "{secs(all, m):.2} → {secs(cur, m):.2} s" }
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
                            let v = &alt[k];
                            let changed = changed(v);
                            let open = *wsel.read() == Some(k);
                            rsx! {
                                tr { key: "{k}", class: if open { "sel" },
                                    onclick: move |_| wsel.set(if open { None } else { Some(k) }),
                                    td { b { "{WIT_MECHANICS[k]}" } }
                                    td { class: "num", "{mean(v, |x| x.lengths):+.2} L" }
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
