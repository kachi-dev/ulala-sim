//! Stamina tab: the selected uma's last spurt and HP over many seeds, among NPCs or in her field, what stamina
//! costs her in lengths, and what the field's stamina debuffs take.

use super::results::{spurt_and_hp, spurt_style};
use super::*;
use crate::charts::{CourseHist, PlaceChart, PlaceSeries, StripChart};


/// HP line (mean) with p25-p75 and p10-p90 bands, or a dashed mean line for the baseline.
fn hp_series(h: &HorseStats, label: &str, dashed: bool) -> PlaceSeries {
    let col = |g: fn(&Dist) -> f32| h.hp_by_progress.iter().map(g).collect::<Vec<f32>>();
    PlaceSeries {
        label: label.to_string(),
        color: "var(--accent)".to_string(),
        line: col(|d| d.mean),
        inner: (!dashed).then(|| (col(|d| d.p25), col(|d| d.p75))),
        outer: (!dashed).then(|| (col(|d| d.p10), col(|d| d.p90))),
        dashed,
        highlight: !dashed,
    }
}

#[component]
pub fn StaminaTab() -> Element {
    let s = use_context::<S>();
    let running = use_busy(s);
    let input = s.input.read();
    if input.horses.is_empty() {
        return rsx! { section { class: "card", p { class: "muted", "Add an uma in Setup first." } } };
    }
    let field_n = input.horses.len();
    let hi = (*s.sel.read()).min(field_n - 1);
    let (card, chara) = (input.horses[hi].card_id, input.horses[hi].chara_id);
    let (mode, npc, size) = mode_seg(s, s.st_npc, field_n);
    // Races per seed: as is, at max stamina, each debuff muted, all muted.
    let n_debuffs = match (npc, s.race.read().as_ref()) {
        (false, Some(race)) => input.horses.iter().enumerate().filter(|x| x.0 != hi).flat_map(|x| &x.1.skills).filter(|k| drains_hp(race, k.skill_id)).count(),
        _ => 0,
    };
    let per_seed = 2 + n_debuffs + (n_debuffs > 0) as usize;
    // Among NPCs only this uma and the conditions matter: an edit elsewhere would rerun the same races.
    let base = s.baseline.cloned().filter(|b| has_edits(Some(b), &input) && (!npc || !conditions_same(b, &input) || !horse_same(&b.horses[hi], &input.horses[hi])));
    drop(input);
    let n = *s.st_n.read();
    // The baseline: as is and at max stamina.
    let runs = n * per_seed + if base.is_some() { 2 * n } else { 0 };
    let work = runs * size;
    let total = if base.is_some() { 2 * n } else { n };
    let eta = eta(s, work);
    let pinned = base.is_some();
    let start = move |_| {
        let (Some(race), Some(cv)) = (s.race.cloned(), s.cview.cloned()) else { return };
        let lobby = *s.npc_lobby.peek();
        let (input, i) = npc_field(&race, &s.input.peek(), hi, npc, lobby);
        let base = base.as_ref().map(|b| npc_field(&race, b, hi, npc, lobby).0);
        let rev = *s.rev.peek();
        let label = format!("Simulating {} races", thousands(runs));
        let field = Arc::new(input.horses.clone());
        let run = move |w: &Work| {
            // Same seeds for both, so they differ only by the edits.
            let seeds = seeds(input.random_seed, n);
            let st = Arc::new(stamina(&race, &input, i, &seeds, true, w));
            (st, base.as_ref().map(|b| Arc::new(stamina(&race, b, i, &seeds, false, w))))
        };
        measure(s, label, total, work, run, move |mut s, (st, base)| {
            s.st_dsel.set(None);
            s.st_res.set(Some(StamRes { card, chara, st, field, base, cv, npc, rev }));
            reveal("stamina");
        });
    };
    rsx! {
        section { class: "card",
            div { class: "card-h", h3 { "Stamina" } }
            div { class: "gsetup",
                {who_sel(s)}
                {mode}
                label { class: "field",
                    span { "Samples" }
                    {seeds_input("stn", s.st_n, 10, 20000)}
                }
            }
            div { class: "runbar",
                button { class: "btn big", disabled: running(), onclick: start, "Simulate {thousands(n)} races" }
                span { class: "muted", if pinned { "vs baseline · " } "{thousands(runs)} races, {eta}" }
                Progress {}
            }
        }
        Subtabs {}
    }
}

/// Spurt / Debuffs subtabs, both from the same run.
#[component]
fn Subtabs() -> Element {
    let s = use_context::<S>();
    let mut debuffs = s.st_debuffs;
    let on = *debuffs.read();
    rsx! {
        div { class: "seg subtabs",
            button { class: if !on { "on" }, onclick: move |_| debuffs.set(false), "Spurt" }
            button { class: if on { "on" }, onclick: move |_| debuffs.set(true), "Debuffs" }
        }
        div { hidden: on, StaminaResults {} }
        div { hidden: !on, Debuffs {} }
    }
}

#[component]
fn StaminaResults() -> Element {
    let s = use_context::<S>();
    let stale = use_stale(s, move || s.st_res.read().as_ref().map(|r| r.rev));
    let Some(r) = s.st_res.cloned() else { return rsx! {} };
    let (h, base, cv) = (&r.st.stats, r.base.as_deref(), &r.cv);
    let b = base.map(|x| &x.stats);
    let loss_of = |st: &Stamina| dist(st.loss.iter().map(|x| x.0).collect());
    let loss = loss_of(&r.st);
    let b_loss = base.map(loss_of);
    // ▲/▼ vs the baseline: `f` of this run and the baseline's, the change shown by `txt`.
    let vs = |f: &dyn Fn(&Stamina, &Dist) -> f32, lower_better: bool, txt: fn(f32) -> String| {
        base.zip(b_loss.as_ref()).map(|(bs, bl)| {
            let d = f(&r.st, &loss) - f(bs, bl);
            delta(d, lower_better, txt(d.abs()))
        })
    };
    let need = h.spurt_need_hp;
    let strip: Vec<(String, String, Vec<f32>)> = SpurtKind::ALL
        .into_iter()
        .map(|k| {
            let st = spurt_style(k);
            (st.label.to_string(), st.color.to_string(), r.st.loss.iter().filter(|x| x.1 == k).map(|x| x.0).collect())
        })
        .collect();
    let mut series: Vec<PlaceSeries> = b.map(|b| hp_series(b, "Baseline (mean)", true)).into_iter().collect();
    series.push(hp_series(h, if b.is_some() { "Edited (mean, p25-p75, p10-p90)" } else { "Mean, p25-p75, p10-p90" }, false));
    let top = series.iter().flat_map(|x| x.outer.as_ref().map_or(&x.line, |o| &o.1)).fold(need, |a, &v| a.max(v));
    rsx! {
        div { id: "stamina", class: "anchor" }
        if stale() {
            div { class: "stale", "The setup changed since this run. Simulate again to update it." }
        }
        section { class: "card detail",
            div { class: "card-h",
                {uma_icon(r.card, r.chara, None, "md")}
                h3 { "{chara_name(r.chara)}" }
                span { class: "chip", if r.npc { "NPCs" } else { "This field" } }
                if b.is_some() { span { class: "chip", "vs baseline" } }
                span { class: "muted", "over {thousands(h.times.len())} races" }
            }
            div { class: "kpis",
                div { b { "{pct(h.full_spurt_rate)} " {vs(&|s, _| s.stats.full_spurt_rate, false, pp)} } span { "full spurt" } }
                div { b { "{pct(h.hp_out_rate)} " {vs(&|s, _| s.stats.hp_out_rate, true, pp)} } span { "ran out of HP" } }
                div { b { "{h.hp_at_goal.p50:.0} " {vs(&|s, _| s.stats.hp_at_goal.p50, false, |v| format!("{v:.0}"))} } span { "median HP at goal" } }
                div { b { "{loss.mean:+.2} L " {vs(&|_, l| l.mean, false, |v| format!("{v:.2} L"))} } span { "length loss" } }
            }
            h4 { "HP over the course" }
            PlaceChart {
                series,
                field_size: 1,
                markers: vec![(cv.final_leg_start / cv.distance * 100.0, "Late race".to_string())],
                value_max: top,
                hlines: vec![(need, format!("Full spurt HP {need:.0}"))],
            }
            {spurt_and_hp(h, cv, "var(--accent)")}
            h4 { "Lengths lost due to stamina" }
            StripChart { rows: strip, unit: "L".to_string() }
        }
    }
}

/// The field's stamina debuffs: each one muted vs on, for the measured uma.
#[component]
fn Debuffs() -> Element {
    let s = use_context::<S>();
    let mut dsel = s.st_dsel;
    let Some(r) = s.st_res.cloned() else { return rsx! {} };
    let (st, cv) = (&r.st, &r.cv);
    if st.debuffs.is_empty() {
        return rsx! { section { class: "card", p { class: "muted", "No stamina debuffs in this field." } } };
    }
    let hit = st.debuffs.iter().filter(|d| d.hit_rate > 0.0).count();
    let lost: f32 = st.debuffs.iter().map(|d| d.hit_rate * d.hp_lost).sum();
    let (full, out) = (st.stats.full_spurt_rate, st.stats.hp_out_rate);
    let (full_m, out_m) = st.all_muted.unwrap_or((full, out));
    let pp = |on: f32, off: f32, lower_better: bool| pp_delta(off, on, lower_better);
    let colors = uma_colors(&RaceInput { horses: (*r.field).clone(), ..Default::default() }, None);
    rsx! {
        section { class: "card",
            div { class: "card-h",
                h3 { "Stamina debuffs" }
                span { class: "muted small", "each debuff in the field on vs muted, same seeds" }
            }
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
                                        td { div { class: "whobox", {horse_icon(o, Some(&colors[d.owner]), "sm")} b { "{chara_name(o.chara_id)}" } } }
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
}
