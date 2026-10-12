//! Batch uma detail: places, vs baseline, last spurt and HP, skills.

use super::*;
use crate::charts::*;

fn place_series(name: &str, color: &str, h: &HorseStats, label: &str, dashed: bool) -> PlaceSeries {
    let col = |g: fn(&Dist) -> f32| h.place_by_progress.iter().map(g).collect::<Vec<f32>>();
    PlaceSeries {
        label: format!("{name} ({label})"),
        color: color.to_string(),
        line: col(|d| d.mean),
        inner: (!dashed).then(|| (col(|d| d.p25), col(|d| d.p75))),
        outer: (!dashed).then(|| (col(|d| d.p10), col(|d| d.p90))),
        dashed,
        highlight: !dashed,
    }
}

pub(super) fn horse_detail(
    s: S,
    res: &Res,
    names: &[String],
    i: usize,
    cur: &Arc<Batch>,
    base: Option<&Arc<Batch>>,
    diff: Option<&BatchDiff>,
) -> Element {
    let Some(race) = s.race.cloned() else { return rsx! {} };
    let (mut focus, mut skill_sel) = (s.focus, s.skill_sel);
    let h = &cur.horses[i];
    let b = base.map(|b| &b.horses[i]);
    let color = res.colors[i].clone();
    let n = cur.horses.len();
    let cv = &res.cv;

    let mut series = vec![];
    if let Some(b) = b {
        series.push(place_series(&names[i], &res.colors[i], b, "baseline", true));
    }
    series.push(place_series(&names[i], &res.colors[i], h, if b.is_some() { "edited" } else { "mean, p25-p75, p10-p90" }, false));
    let bars: Vec<(String, f32)> = h.place_hist.iter().enumerate().map(|(k, v)| ((k + 1).to_string(), *v)).collect();
    let compare = b.map(|b| b.place_hist.clone());

    // Skill rows: union of baseline and edited skill ids.
    let mut ids: Vec<i32> = h.skills.iter().map(|k| k.skill_id).collect();
    for k in b.map(|b| &b.skills[..]).unwrap_or(&[]) {
        if !ids.contains(&k.skill_id) {
            ids.push(k.skill_id);
        }
    }
    let rate = |hs: &HorseStats, id: i32| hs.skills.iter().find(|k| k.skill_id == id).map(|k| k.rate).unwrap_or(0.0);
    let outcomes = |id: i32| h.skills.iter().find(|k| k.skill_id == id).map(|k| k.outcomes);
    let sel_stat = skill_sel.read().filter(|x| x.0 == i).and_then(|(_, id)| h.skills.iter().find(|k| k.skill_id == id));
    let has = |hs: &HorseStats, id: i32| hs.skills.iter().any(|k| k.skill_id == id);
    let mark = |id: i32| match b {
        Some(b) if !has(b, id) => "added",
        Some(_) if !has(h, id) => "removed",
        _ => "",
    };
    let strip: Vec<(String, String, Vec<f32>)> =
        h.skills.iter().map(|k| (skill_info(k.skill_id).name, color.clone(), k.distances.clone())).collect();

    rsx! {
        section { class: "card detail",
            div { class: "card-h",
                {horse_icon(&res.input.horses[i], Some(&res.colors[i]), "md")}
                h3 { "{names[i]}" }
                span { class: "muted", "{style_name(&race, &res.input.horses[i])} · over {cur.seeds.len()} races" }
                div { class: "grow" }
                UmaLinks { i, card: res.input.horses[i].card_id }
                button { class: "btn icon", title: "Close", onclick: move |_| focus.set(None), {icon("x")} }
            }
            div { class: "kpis",
                div { b { "{pct(h.win_rate)}" } span { "win" } }
                div { b { "{h.place.mean:.2}" } span { "avg place" } }
                div { b { "{ft(h.finish_time.p50)}" } span { "median time" } }
                div { b { "{pct(h.full_spurt_rate)}" } span { "full spurt" } }
                div { b { "{pct(h.hp_out_rate)}" } span { "ran out of HP" } }
            }
            if let Some(d) = diff {
                {vs_baseline(d, res.ed_mask.get(i) == Some(&true), &res.colors[i])}
            }
            if n > 1 {
                div { class: "cols",
                    div {
                        h4 { "Position over the course" }
                        PlaceChart { series, field_size: n, markers: final_leg_marker(cv) }
                    }
                    div {
                        h4 { "Finishing place" if b.is_some() { span { class: "muted small", " (outline: baseline)" } } }
                        BarChart { bars, compare, color: color.clone() }
                    }
                }
            }
            {spurt_and_hp(h, cv, &color)}
            h4 { "Skills" }
            div { class: "olegend muted small",
                for (k, (label, hint)) in OUTCOMES.iter().enumerate() {
                    span { title: "{hint}", i { class: "o{k}" } "{label}" }
                }
            }
            div { class: "cols",
                table { class: "grid",
                    thead {
                        tr {
                            th { "Skill" }
                            th { class: "num", "Activates" }
                            th { "Outcomes" }
                        }
                    }
                    tbody {
                        for id in ids {
                            {
                                let r = rate(h, id);
                                let on = *skill_sel.read() == Some((i, id));
                                rsx! {
                                    tr {
                                        class: if on { "sel" },
                                        title: "Click for where it activates",
                                        onclick: move |_| {
                                            skill_sel.set(if on { None } else { Some((i, id)) });
                                            reveal("skdetail");
                                        },
                                        td { {skill_row(&skill_info(id), None, mark(id))} }
                                        td { class: "num",
                                            "{pct(r)}"
                                            if let Some(b) = b {
                                                if pct(rate(b, id)) != pct(r) { div { class: "was", "was {pct(rate(b, id))}" } }
                                            }
                                        }
                                        td { if let Some(o) = outcomes(id) { {outcome_bar(&o)} } }
                                    }
                                }
                            }
                        }
                    }
                }
                CourseStrip { rows: strip, course_distance: cv.distance, final_leg_start: cv.final_leg_start }
            }
            if let Some(st) = sel_stat {
                {skill_detail(st, cv, &color, skill_sel)}
            }
        }
    }
}

/// Stacked bar of `SkillStat::outcomes`, one color per `OUTCOMES` entry.
fn outcome_bar(o: &[f32; 4]) -> Element {
    rsx! {
        div { class: "obar",
            for (k, f) in o.iter().enumerate() {
                if *f > 0.0 {
                    div { class: "o{k}", style: "width: {f * 100.0:.2}%", title: "{OUTCOMES[k].0}: {pct(*f)}. {OUTCOMES[k].1}." }
                }
            }
        }
    }
}

/// "Edit uma" / "Measure skill gains" for gate `i` of a past run: the same uma in the current field (same gate if it
/// still is that costume, else the first match). A component, so following the field doesn't re-render the results.
#[component]
fn UmaLinks(i: usize, card: i32) -> Element {
    let s = use_context::<S>();
    let (mut sel, mut g_owned, mut editing) = (s.sel, s.g_owned, s.editing);
    let at = {
        let field = &s.input.read().horses;
        field.get(i).is_some_and(|h| h.card_id == card).then_some(i).or_else(|| field.iter().position(|h| h.card_id == card))
    };
    let gone = "This uma is no longer in the field";
    rsx! {
        button {
            class: "btn ghost",
            disabled: at.is_none(),
            title: if at.is_some() { "Edit this uma; run again from the bar below" } else { gone },
            onclick: move |_| editing.set(at),
            "Edit uma"
        }
        button {
            class: "btn alt",
            disabled: at.is_none(),
            title: if at.is_some() { "Measure how many lengths each of this uma's skills is worth (Skill Gains tab)" } else { gone },
            onclick: move |_| {
                if let Some(j) = at {
                    sel.set(j);
                    g_owned.set(true);
                    go(s, Tab::Gains);
                }
            },
            "Measure skill gains"
        }
    }
}

/// One skill of one uma over the batch: fine activation histogram along the course, rate, mean / median point.
fn skill_detail(st: &SkillStat, cv: &CourseView, color: &str, mut skill_sel: Signal<Option<(usize, i32)>>) -> Element {
    let k = skill_info(st.skill_id);
    let mut d: Vec<f32> = st.distances.iter().copied().filter(|x| x.is_finite()).collect();
    d.sort_by(f32::total_cmp);
    let mean = if d.is_empty() { 0.0 } else { d.iter().sum::<f32>() / d.len() as f32 };
    let median = d.get(d.len() / 2).copied().unwrap_or(0.0);
    let phase = |m: f32| ["Early race", "Mid race", "Late race", "Last spurt"][cv.phases.iter().filter(|&&p| p <= m).count().clamp(1, 4) - 1];
    rsx! {
        div { class: "skdetail", id: "skdetail",
            div { class: "card-h",
                {skill_row(&k, None, "")}
                div { class: "grow" }
                button { class: "btn icon", title: "Close", onclick: move |_| skill_sel.set(None), {icon("x")} }
            }
            div { class: "kpis",
                div { b { "{pct(st.rate)}" } span { "of races it activated" } }
                if !d.is_empty() {
                    div { b { "{mean:.0} m" } span { "mean point ({phase(mean)})" } }
                    div { b { "{median:.0} m" } span { "median point" } }
                    div { b { "{d[0]:.0} to {d[d.len() - 1]:.0} m" } span { "earliest to latest" } }
                }
                for k in 1..OUTCOMES.len() {
                    if st.outcomes[k] > 0.0 {
                        div { class: "o{k}", title: "{OUTCOMES[k].1}", b { "{pct(st.outcomes[k])}" } span { "{OUTCOMES[k].0.to_lowercase()}" } }
                    }
                }
            }
            {skill_facts(st.skill_id, Some(cv.distance))}
            CourseHist {
                distances: d.clone(),
                course_distance: cv.distance,
                phases: cv.phases.to_vec(),
                corners: cv.corner_spans(),
                straights: cv.straights.iter().map(|c| (c.start, c.end)).collect::<Vec<_>>(),
                bin: 10.0,
                color: color.to_string(),
            }
        }
    }
}

/// Paired per-seed comparison with the baseline for one uma (the field changed, so unedited umas shift too).
fn vs_baseline(d: &BatchDiff, edited: bool, color: &str) -> Element {
    let n = d.length_deltas.len().max(1) as f32;
    let ahead = d.length_deltas.iter().filter(|&&x| x > 0.005).count() as f32 / n;
    let behind = d.length_deltas.iter().filter(|&&x| x < -0.005).count() as f32 / n;
    let l = &d.lengths;
    rsx! {
        div { class: if edited { "vsbase edited" } else { "vsbase" },
            div { class: "vs-h",
                h4 { "vs baseline" }
                span { class: "muted small",
                    if edited { "This uma was edited. " } else { "Not edited: the change comes from the rest of the field. " }
                    "Same seeds, lengths ahead (+) or behind (-) at the finish."
                }
            }
            div { class: "kpis",
                div { b { {len_delta(l.mean)} } span { "mean" } }
                div { b { {len_delta(l.p50)} } span { "median" } }
                div { b { {len_delta(l.min)} } span { "min" } }
                div { b { {len_delta(l.max)} } span { "max" } }
                div { b { "{l.p10:+.2} to {l.p90:+.2}" } span { "p10 to p90 (L)" } }
                div { b { span { class: "d good", "{pct(ahead)}" } " / " span { class: "d bad", "{pct(behind)}" } } span { "seeds ahead / behind ({pct((1.0 - ahead - behind).max(0.0))} equal)" } }
            }
            Histogram { values: d.length_deltas.clone(), color: color.to_string(), unit: "L".to_string() }
        }
    }
}

pub(in crate::ui) const SPURT_KINDS: [(&str, &str); 5] = [
    ("Full spurt", "Max spurt speed from the final-leg start"),
    ("Delayed", "Max speed, but started after the final-leg start"),
    ("Reduced", "Started on time, but at less than max speed (not a full spurt)"),
    ("Delayed + reduced", "Started late and below max speed"),
    ("No spurt", "Never switched to a last spurt"),
];

/// Batch uma detail: when the last spurt starts vs the expected point, how good it was, and HP left at the goal.
pub(in crate::ui) fn spurt_and_hp(h: &HorseStats, cv: &CourseView, color: &str) -> Element {
    let fl = cv.final_leg_start;
    let on_time = h.spurt_kinds[0] + h.spurt_kinds[2];
    let mut late: Vec<f32> = h.spurt_delays.iter().copied().filter(|&d| d > SPURT_ON_TIME_M).collect();
    late.sort_by(f32::total_cmp);
    let med_late = late.get(late.len() / 2).copied();
    let mut outs = h.hp_out_distances.clone();
    outs.sort_by(f32::total_cmp);
    let before_goal = outs.get(outs.len() / 2).map(|d| cv.distance - d);
    // Zoom to where it happens: from a round 50 m before the earliest HP-out (never before the start) to the goal.
    let zoom = outs.first().map_or(0.0, |&m| ((m - 50.0) / 50.0).floor().max(0.0) * 50.0);
    rsx! {
        div { class: "spurt",
            div { class: "sp-start",
                h4 { "Last spurt start" }
                div { class: "muted small",
                    "Meters after the final-leg start ({fl:.0} m), decided at its first step. On time in {pct(on_time)} of races"
                    if let Some(m) = med_late { "; when late, typically {m:.0} m late" }
                    "."
                }
                // Delay, not position: starts are quantised by the sim step, so position bins would alternate empty.
                DelayHist { values: h.spurt_delays.clone(), on_time: SPURT_ON_TIME_M, color: color.to_string() }
            }
            div { class: "sp-kind",
                h4 { "Spurt quality" }
                div { class: "kindbars",
                    for (k, f) in h.spurt_kinds.iter().enumerate() {
                        if *f > 0.0 {
                            div { class: "kb", title: "{SPURT_KINDS[k].1}",
                                span { class: "kb-l", "{SPURT_KINDS[k].0}" }
                                div { class: "kb-t", div { class: "kb-f k{k}", style: "width: {f * 100.0:.2}%" } }
                                b { class: "kb-v", "{pct(*f)}" }
                            }
                        }
                    }
                }
            }
            div { class: "sp-hp",
                h4 { "Ran out of stamina" }
                if outs.is_empty() {
                    div { class: "small ok-line", "✓ Never ran out of stamina." }
                } else {
                    div { class: "small no",
                        "Ran out in {pct(h.hp_out_rate)} of races"
                        if let Some(b) = before_goal { ", typically {b:.0} m before the goal" }
                        "."
                    }
                    if zoom > 0.0 {
                        div { class: "muted small", "Zoomed to the end of the course: {zoom:.0} m to the goal ({cv.distance:.0} m)." }
                    }
                    CourseHist {
                        start: zoom,
                        distances: outs.clone(),
                        course_distance: cv.distance,
                        phases: cv.phases.to_vec(),
                        corners: vec![],
                        bin: if zoom > 0.0 { 10.0 } else { 25.0 },
                        color: "var(--bad)".to_string(),
                    }
                }
            }
        }
    }
}
