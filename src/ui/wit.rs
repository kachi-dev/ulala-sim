//! Wit subtab of the batch uma detail (only for an uma whose Wit differs from the baseline): what the Wit change does
//! through each mechanic that reads Wit, one at a time, on the run's field and seeds.

use super::*;
use crate::charts::{GainChart, Histogram};
use uma_race_sim::api::{span_kind, wiz_mechanic};

/// `WIT_MODES[m]` is downhill accel (the rest are position keep modes).
fn is_downhill(m: usize) -> bool {
    WIT_MODES[m].0 == span_kind::DOWNHILL
}

/// A change of `d` seconds in `WIT_MODES[m]`: more downhill accel is good; the position keep modes are neither.
fn mode_delta(m: usize, d: f32) -> Element {
    if is_downhill(m) { delta(d, false, format!("{:.2} s", d.abs())) } else { rsx! { span { class: "d flat", "{d:+.2} s" } } }
}

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

/// Horse `h` of the batch vs `b`, the same uma on the baseline (when there is one): lengths, full spurt, rushed and time in
/// the Wit-driven modes, straight from the two batches.
pub(super) fn wit_cards(h: &HorseStats, b: Option<&HorseStats>, diff: Option<&BatchDiff>) -> Element {
    let rushed = |x: &HorseStats| x.states[span_kind::RUSHED as usize].0;
    // Mean seconds per race in `WIT_MODES[m]` (`states` holds the share of races and the mean over those).
    let secs = |x: &HorseStats, m: usize| {
        let (share, s) = x.states[WIT_MODES[m].0 as usize];
        share * s
    };
    // "old → new", or just the value without a baseline.
    let val = |f: &dyn Fn(&HorseStats) -> f32, fmt: &dyn Fn(f32) -> String| match b {
        Some(b) => format!("{} → {}", fmt(f(b)), fmt(f(h))),
        None => fmt(f(h)),
    };
    let ch = |f: &dyn Fn(&HorseStats) -> f32, lower_better: bool, txt: &dyn Fn(f32) -> String| {
        b.map(|b| {
            let d = f(h) - f(b);
            delta(d, lower_better, txt(d.abs()))
        })
    };
    let mode_ch = |m: usize| Some(mode_delta(m, secs(h, m) - secs(b?, m)));
    // Modes the uma is ever in (a Front runner never paces up or down, the others never speed up or overtake).
    let used: Vec<usize> = (0..WIT_MODES.len()).filter(|&m| secs(h, m) > 0.0 || b.is_some_and(|b| secs(b, m) > 0.0)).collect();
    // Mean lengths vs the baseline and its 95% range.
    let lengths = diff.map(|d| {
        let n = d.length_deltas.len().max(2) as f32;
        let sd = (d.length_deltas.iter().map(|l| (l - d.lengths.mean).powi(2)).sum::<f32>() / (n - 1.0)).sqrt();
        (d.lengths.mean, 1.96 * sd / n.sqrt())
    });
    rsx! {
        div { class: "kpis",
            if let Some((l, ci)) = lengths {
                div { b { "{l:+.2} L" } div { class: "kpi2", span { "lengths" } span { title: "95% range of the mean", "{l - ci:+.2} to {l + ci:+.2}" } } }
            }
            div { b { {val(&|x| x.full_spurt_rate, &|v| pct(v))} } div { class: "kpi2", span { "full spurt" } {ch(&|x| x.full_spurt_rate, false, &pp)} } }
            div { b { {val(&rushed, &|v| pct(v))} } div { class: "kpi2", span { "rushed" } {ch(&rushed, true, &pp)} } }
            for m in used {
                div { key: "{m}",
                    b { {val(&|x| secs(x, m), &|v| format!("{v:.2}"))} " s" }
                    div { class: "kpi2", span { "{WIT_MODES[m].1.to_lowercase()}" } {mode_ch(m)} }
                }
            }
        }
    }
}

/// Horse `i` of the batch on screen, whose baseline Wit was `old`: the length calculation's button (beside the
/// detail's subtabs).
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
        button { class: "btn small", disabled: running(), onclick: start, "Calculate length differences" }
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
    let all = &r.wit.all;
    let rushed = |v: &[WitPair]| means(v, |x| x.rushed as u8 as f32);
    let skills = |v: &[WitPair]| means(v, |x| x.skills as f32);
    let secs = |v: &[WitPair], m: usize| means(v, |x| x.modes[m]);
    // "Pace up 20.17 s +0.70 s": mode `m` on the new Wit, and its change from the old.
    let mode = |m: usize, v: &[WitPair]| {
        let (o, n) = secs(v, m);
        rsx! { "{WIT_MODES[m].1} {n:.2} s " {mode_delta(m, n - o)} }
    };
    // Modes the uma is ever in (a Front runner never paces up or down, the others never speed up or overtake).
    let used: Vec<usize> = (0..WIT_MODES.len()).filter(|&m| all.iter().any(|p| p.new.modes[m] > 0.0 || p.old.modes[m] > 0.0)).collect();
    let downhill = used.iter().copied().find(|&m| is_downhill(m));
    let pk: Vec<usize> = used.iter().copied().filter(|&m| !is_downhill(m)).collect();
    let pp = |(o, n): (f32, f32), lower_better: bool| pp_delta(o, n, lower_better);
    // Mechanics by mean gain, most first.
    let mut order: Vec<usize> = (0..WIT_MECHANICS.len()).collect();
    order.sort_by(|&a, &b| gain(&rows[b]).total_cmp(&gain(&rows[a])));
    let row = |name: &str, color: &str, v: &[WitPair]| (name.to_string(), color.to_string(), box_stats(v), v.len());
    let mut chart: Vec<(String, String, [f32; 6], usize)> = order.iter().map(|&k| row(WIT_MECHANICS[k], "var(--accent)", &rows[k])).collect();
    chart.push(row("Total", "var(--accent-2)", all));
    // Per mechanic: what it changes besides lengths, old to new Wit.
    let effect = |k: usize| -> Element {
        let v = &rows[k];
        let none = rsx! { span { class: "muted", "-" } };
        match 1u8 << k {
            wiz_mechanic::DOWNHILL => downhill.map_or(none, |m| mode(m, v)),
            wiz_mechanic::POSITION_KEEP if !pk.is_empty() => rsx! {
                for (j, &m) in pk.iter().enumerate() {
                    if j > 0 { " · " }
                    {mode(m, v)}
                }
            },
            // Wit only picks among the shorter spurts (full spurt is decided by HP first): where they started.
            wiz_mechanic::LAST_SPURT => {
                let p: Vec<(f32, f32)> = v.iter().filter(|p| p.gain != 0.0 && p.new.spurt_start >= 0.0 && p.old.spurt_start >= 0.0).map(|p| (p.new.spurt_start, p.old.spurt_start)).collect();
                if p.is_empty() {
                    none
                } else {
                    let (a, b) = (p.iter().map(|x| x.0).sum::<f32>() / p.len() as f32, p.iter().map(|x| x.1).sum::<f32>() / p.len() as f32);
                    rsx! { "Spurt start {a:.0} m " {delta(a - b, true, format!("{:.0} m", (a - b).abs()))} }
                }
            }
            wiz_mechanic::RUSHED => rsx! { "Rushed {pct(rushed(v).1)} " {pp(rushed(v), true)} },
            wiz_mechanic::SKILLS => {
                let (o, n) = skills(v);
                rsx! { "{n:.2} skills per race " {delta(n - o, false, format!("{:.2}", (n - o).abs()))} }
            }
            _ => none,
        }
    };
    rsx! {
        if stale() {
            div { class: "stale", "The setup changed since this run. Simulate again to update it." }
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
