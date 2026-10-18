//! Stamina subtab of the batch uma detail: HP and the last spurt from the batch; on demand, lengths
//! lost to stamina and what the field's stamina debuffs take, on the run's field, over their own seed count.

use super::results::{spurt_and_hp, spurt_style};
use super::*;
use crate::charts::{CourseHist, StripChart};

/// Horse `h` of the batch vs `b`, the same uma on the baseline (when there is one): full spurt, HP out, HP at goal
/// and the last spurt.
pub(super) fn stamina_cards(h: &HorseStats, b: Option<&HorseStats>, cv: &CourseView) -> Element {
    let vs = |f: &dyn Fn(&HorseStats) -> f32, lower_better: bool, txt: fn(f32) -> String| {
        b.map(|b| {
            let d = f(h) - f(b);
            delta(d, lower_better, txt(d.abs()))
        })
    };
    rsx! {
        div { class: "kpis",
            div { b { "{pct(h.full_spurt_rate)} " {vs(&|x| x.full_spurt_rate, false, pp)} } span { "full spurt" } }
            div { b { "{pct(h.hp_out_rate)} " {vs(&|x| x.hp_out_rate, true, pp)} } span { "ran out of HP" } }
            div { b { "{h.hp_at_goal.p50:.0} " {vs(&|x| x.hp_at_goal.p50, false, |v| format!("{v:.0}"))} } span { "median HP at goal" } }
        }
        {spurt_and_hp(h, cv, "var(--accent)")}
    }
}

/// Horse `i` of the batch on screen: the stamina calculation's button (beside the detail's subtabs).
#[component]
pub(super) fn StaminaRun(i: usize) -> Element {
    let s = use_context::<S>();
    let running = use_busy(s);
    let (Some((input, rivals)), Some(race), Some(cv)) = (setup_for(s, i), s.race.cloned(), s.cview.cloned()) else { return rsx! {} };
    // The baseline as it runs, and uma `i` in it (none when she was added since).
    let base = s.baseline.cloned().filter(|b| has_edits(Some(b), &s.input.peek())).and_then(|b| {
        let j = pairing(&b, &s.input.peek()).get(i).copied().flatten()?;
        let (b, r) = setup::filled(s, &race, &b).ok()?;
        Some((Arc::new(b), r, j))
    });
    let n = *s.st_n.read();
    // Per seed: as is, at max stamina, each debuff muted, all muted; the baseline as is and at max stamina.
    let n_debuffs = field_debuffs(&race, &input, i).iter().filter(|d| rivals.as_ref().is_none_or(|r| d.0 < r.from)).count();
    let runs = n * (2 + n_debuffs + (n_debuffs > 0) as usize) + if base.is_some() { 2 * n } else { 0 };
    let work = runs * input.horses.len();
    let total = if base.is_some() { 2 * n } else { n };
    let eta = eta(s, work);
    let start = move |_| {
        let (race, input, rivals, base, cv) = (race.clone(), input.clone(), rivals.clone(), base.clone(), cv.clone());
        let seeds = seeds(input.random_seed, n);
        let rev = *s.rev.peek();
        let label = format!("Simulating {} races", thousands(runs));
        let field = Arc::new(input.horses.clone());
        let run = move |w: &Work| {
            // Same seeds for both, so they differ only by the edits.
            let st = Arc::new(stamina(&race, &input, rivals.as_ref(), i, &seeds, true, w));
            (st, base.as_ref().map(|(b, r, j)| Arc::new(stamina(&race, b, r.as_ref(), *j, &seeds, false, w))))
        };
        measure(s, label, total, work, run, move |mut s, (st, base)| {
            s.st_dsel.set(None);
            s.st_res.set(Some(StamRes { horse: i, st, field, base, cv, rev }));
        });
    };
    rsx! {
        div { class: "runn",
            button { class: "btn small", disabled: running(), onclick: start, "Calculate stamina loss and debuffs" }
            {seeds_input("stn", s.st_n, 10, 20000)}
        }
        span { class: "muted small", "{thousands(runs)} races, {eta}" }
        Progress {}
    }
}

/// The measurement of horse `i`, once simulated: lengths lost to stamina and the field's stamina debuffs.
#[component]
pub(super) fn StaminaOut(i: usize) -> Element {
    let s = use_context::<S>();
    let stale = use_stale(s, move || s.st_res.read().as_ref().map(|r| r.rev));
    let Some(r) = s.st_res.cloned().filter(|r| r.horse == i) else { return rsx! {} };
    // Mean lengths lost (`loss` is negative when lost): fewer is better.
    let lost_of = |st: &Stamina| -st.loss.iter().map(|x| x.0).sum::<f32>() / st.loss.len().max(1) as f32;
    let lost = lost_of(&r.st);
    let vs = r.base.as_deref().map(|b| {
        let d = lost - lost_of(b);
        delta(d, true, format!("{:.2} L", d.abs()))
    });
    let strip: Vec<(String, String, Vec<f32>)> = SpurtKind::ALL
        .into_iter()
        .map(|k| {
            let st = spurt_style(k);
            (st.label.to_string(), st.color.to_string(), r.st.loss.iter().filter(|x| x.1 == k).map(|x| x.0).collect())
        })
        .collect();
    rsx! {
        if stale() {
            div { class: "stale", "The setup changed since this run. Simulate again to update it." }
        }
        h4 { "Lengths lost due to stamina" span { class: "muted small", " (vs the same uma at max stamina, same seeds)" } }
        div { class: "kpis",
            div { b { "{lost:.2} L " {vs} } span { "mean lengths lost" } }
        }
        StripChart { rows: strip, unit: "L".to_string() }
        {debuffs(s, &r)}
    }
}

/// The field's stamina debuffs: each one muted vs on, for the measured uma.
fn debuffs(s: S, r: &StamRes) -> Element {
    let mut dsel = s.st_dsel;
    let (st, cv) = (&r.st, &r.cv);
    if st.debuffs.is_empty() {
        return rsx! { p { class: "muted", "No stamina debuffs from other teams." } };
    }
    let hit = st.debuffs.iter().filter(|d| d.hit_rate > 0.0).count();
    let lost: f32 = st.debuffs.iter().map(|d| d.hit_rate * d.hp_lost).sum();
    let (full, out) = (st.stats.full_spurt_rate, st.stats.hp_out_rate);
    let (full_m, out_m) = st.all_muted.unwrap_or((full, out));
    let pp = |on: f32, off: f32, lower_better: bool| pp_delta(off, on, lower_better);
    let colors = uma_colors(&RaceInput { horses: (*r.field).clone(), ..Default::default() }, None);
    rsx! {
        h4 { "Stamina debuffs" span { class: "muted small", " (each debuff of another team on vs muted, same seeds)" } }
        div { class: "kpis",
            div { b { "{hit} of {st.debuffs.len()}" } span { "debuffs hit" } }
            div { b { "-{lost:.0} HP" } span { "mean HP lost, all combined" } }
            div { b { "{pct(full)} " {pp(full, full_m, false)} } span { "full spurt ({pct(full_m)} muted)" } }
            div { b { "{pct(out)} " {pp(out, out_m, true)} } span { "ran out of HP ({pct(out_m)} muted)" } }
        }
        div { class: "scroll-x",
            table { class: "grid",
                thead {
                    tr {
                        th { "Skill" }
                        th { "Owner" }
                        th { class: "num", "Activates" }
                        th { class: "num", "Hits" }
                        th { class: "num", "HP lost" }
                        th { class: "num", "Full spurt" }
                        th { class: "num", "Ran out of HP" }
                    }
                }
                tbody {
                    for d in st.debuffs.iter() {
                        {
                            let key = (d.owner, d.skill_id);
                            let open = *dsel.read() == Some(key);
                            let o = &r.field[d.owner];
                            rsx! {
                                tr { key: "{d.owner}-{d.skill_id}", class: if open { "sel" },
                                    onclick: move |_| dsel.set(if open { None } else { Some(key) }),
                                    td { {skill_row(&skill_info(d.skill_id), None, "")} }
                                    td { div { class: "whobox", {horse_icon(o, Some(&colors[d.owner]), "sm")} b { "{horse_name(o)}" } } }
                                    td { class: "num", "{pct(d.rate)}" }
                                    td { class: "num",
                                        div { class: "kb-t hitbar", div { class: "kb-f", style: "width: {d.hit_rate * 100.0:.2}%; background: var(--bad)" } }
                                        span { class: "hitv", "{pct(d.hit_rate)}" }
                                    }
                                    td { class: "num", if d.hit_rate > 0.0 { "-{d.hp_lost:.0} HP" } else { span { class: "muted", "-" } } }
                                    td { class: "num", {pp(d.full.0, d.full.1, false)} }
                                    td { class: "num", {pp(d.out.0, d.out.1, true)} }
                                }
                                if open {
                                    tr { key: "{d.owner}-{d.skill_id}-detail", class: "expand",
                                        td { colspan: "7",
                                            div { class: "gdetail",
                                                div { class: "kpis",
                                                    div { b { "{pct(d.rate)}" } span { "activates" } }
                                                    div { b { "{pct(d.hit_rate)}" } span { "hits" } }
                                                    if d.hit_rate > 0.0 {
                                                        div { b { "-{d.hp_lost:.0} HP" } span { "HP lost when hit" } }
                                                        div { b { "{pct(d.full_hit.0)} " {pp(d.full_hit.0, d.full_hit.1, false)} } span { "full spurt when hit" } }
                                                        div { b { "{pct(d.out_hit.0)} " {pp(d.out_hit.0, d.out_hit.1, true)} } span { "ran out of HP when hit" } }
                                                    }
                                                }
                                                {skill_facts(d.skill_id, Some(cv.distance))}
                                                if d.hit_rate > 0.0 {
                                                    h4 { "Hit position" }
                                                    CourseHist {
                                                        distances: d.hit_at.clone(),
                                                        course_distance: cv.distance,
                                                        phases: cv.phases.to_vec(),
                                                        corners: cv.corner_spans(),
                                                        straights: cv.straights.iter().map(|c| (c.start, c.end)).collect::<Vec<_>>(),
                                                        bin: 25.0,
                                                        color: "var(--bad)".to_string(),
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
}
