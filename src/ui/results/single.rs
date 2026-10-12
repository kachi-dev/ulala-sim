//! Single-run results: finish table, positions over the course, skill activations.

use super::*;
use crate::charts::*;

pub(super) fn single_view(s: S, res: &Res, names: &[String], cur: &Run, base: Option<&Run>) -> Element {
    let mut focus = s.focus;
    let n = cur.horses.len();
    let f = (*focus.read()).filter(|&f| f < n);
    let diffs = base.map(|b| diff_runs(b, cur));
    let ed = &res.ed_mask;
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by_key(|&i| cur.horses[i].place);
    let improved = diffs.as_ref().is_some_and(|d| d.iter().zip(ed).any(|(d, &m)| m && d.place.1 < d.place.0));

    let line = |run: &Run, i: usize, dashed: bool| PlaceSeries {
        label: if dashed { format!("{} (baseline)", names[i]) } else { names[i].clone() },
        color: res.colors[i].clone(),
        line: run.horses[i].place_by_progress.iter().map(|&p| p as f32).collect(),
        inner: None,
        outer: None,
        dashed,
        highlight: f == Some(i) || (f.is_none() && ed[i]),
    };
    let mut series: Vec<PlaceSeries> = Vec::new();
    if let Some(b) = base {
        series.extend((0..n).filter(|&i| ed[i] || f == Some(i)).map(|i| line(b, i, true)));
    }
    series.extend((0..n).map(|i| line(cur, i, false)));

    let strip: Vec<(String, String, Vec<f32>)> = (0..n)
        .filter(|&i| f.is_none_or(|f| f == i))
        .map(|i| {
            let d = cur.activations.iter().filter(|a| a.horse == i).map(|a| a.distance).collect();
            (names[i].clone(), if f.is_some() { "var(--accent)".to_string() } else { res.colors[i].clone() }, d)
        })
        .collect();
    let acts: Vec<&Activation> = cur.activations.iter().filter(|a| f.is_none_or(|f| f == a.horse)).collect();
    let missed: Vec<(usize, &SkillCheck)> = (0..n)
        .filter(|&i| f.is_none_or(|f| f == i))
        .flat_map(|i| cur.horses[i].skills.iter().filter(|c| c.outcome != SkillOutcome::Activated).map(move |c| (i, c)))
        .collect();
    let all = f.is_none() && n > 1;
    let cv = &res.cv;

    rsx! {
        section { class: "card",
            div { class: "card-h",
                h3 { if base.is_some() { "Result: edited vs baseline" } else { "Result" } }
                span { class: "muted", "seed {cur.seed} · click a row to focus an uma" }
                if improved { img { class: "great", src: "/a/great.webp", alt: "Great!" } }
            }
            div { class: "scroll-x",
                table { class: "grid results",
                    thead {
                        tr {
                            th { "Place" }
                            th { "Uma" }
                            th { class: "num", "Time" }
                            th { class: "num c-behind", "Behind" }
                            th { class: "num c-spurtfrom", "Spurt from" }
                            th { title: "On time from the final-leg start and at max spurt speed", "Full spurt" }
                            th { class: "num", "HP at goal" }
                            if base.is_some() { th { "Activations gained / lost" } }
                        }
                    }
                    tbody {
                        for i in order {
                            {
                                let h = &cur.horses[i];
                                let d = diffs.as_ref().map(|d| &d[i]);
                                let card = res.input.horses[i].card_id;
                                rsx! {
                                    tr {
                                        key: "{i}",
                                        class: if f == Some(i) { "sel" } else if ed[i] { "edited" },
                                        onclick: move |_| if f == Some(i) { focus.set(None) } else { focus_gate(s, i, card) },
                                        td { class: "place",
                                            b { "{h.place}" }
                                            if let Some(d) = d {
                                                if d.place.0 != d.place.1 {
                                                    {delta(d.place.0 as f32 - d.place.1 as f32, false, d.place.0.abs_diff(d.place.1).to_string())}
                                                }
                                            }
                                        }
                                        td { class: "who", {who(res, names, i)} }
                                        td { class: "num",
                                            "{ft(h.finish_time)}"
                                            if let Some(d) = d {
                                                if (d.finish_time.1 - d.finish_time.0).abs() > 1e-4 {
                                                    div { {time_delta(d.finish_time.1 - d.finish_time.0)} }
                                                    div { {len_delta(d.lengths)} }
                                                }
                                            }
                                        }
                                        td { class: "num muted c-behind",
                                            if h.place > 1 {
                                                "+{h.behind:.2}s · {h.behind_lengths:.1} L"
                                                // The game's margin: finish time gap to the horse in front × 15 m/s, in lengths.
                                                div { class: "margin", title: "Margin to the uma in front, as the game shows it", "{margin_label((h.finish_time - ahead_of(cur, h.place)) * 15.0 / 2.5)}" }
                                            }
                                        }
                                        td { class: "num c-spurtfrom",
                                            if h.spurt_start >= 0.0 { "{h.spurt_start:.0} m" } else { "-" }
                                            if let Some(d) = d {
                                                if (d.spurt_start.0 - d.spurt_start.1).abs() > 0.5 {
                                                    div { class: "was", "was {d.spurt_start.0:.0} m" }
                                                }
                                            }
                                        }
                                        td { class: if h.full_spurt { "ok" } else { "no" }, if h.full_spurt { "✓" } else { "✗" } }
                                        td { class: if h.hp_out { "num no" } else { "num" },
                                            "{h.hp_at_goal:.0}"
                                            if let Some(at) = h.hp_out_at { div { class: "small no", title: "Ran out of stamina here", "out at {at:.0} m" } }
                                            if let Some(d) = d {
                                                if (d.hp_at_goal.0 - d.hp_at_goal.1).abs() > 0.5 {
                                                    div { class: "was", "was {d.hp_at_goal.0:.0}" }
                                                }
                                            }
                                        }
                                        if let Some(d) = d {
                                            td { class: "skills",
                                                for id in d.skills_gained.iter() {
                                                    SkillX { id: *id, extra: "added" }
                                                }
                                                for id in d.skills_lost.iter() {
                                                    SkillX { id: *id, extra: "removed" }
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
        if n > 1 {
            section { class: "card",
                div { class: "card-h",
                    h3 { "Position over the course" }
                    if base.is_some() { span { class: "muted", "dashed: baseline" } }
                }
                PlaceChart { series, field_size: n, markers: final_leg_marker(cv) }
            }
        }
        section { class: "card",
            div { class: "card-h",
                h3 { "Skill activations" }
                span { class: "muted", if let Some(f) = f { "{names[f]}" } else { "all umas, click a result row to focus one" } }
            }
            CourseStrip { rows: strip, course_distance: cv.distance, final_leg_start: cv.final_leg_start }
            if !missed.is_empty() {
                details { class: "acts", open: f.is_some() || n == 1,
                    summary { "{missed.len()} skill" if missed.len() != 1 { "s" } " didn't activate" }
                    table { class: "grid",
                        thead {
                            tr {
                                if all { th { "Uma" } }
                                th { "Skill" }
                                th { "Reason" }
                            }
                        }
                        tbody {
                            for (i, c) in missed {
                                tr {
                                    if all { td { class: "who", {who(res, names, i)} } }
                                    td { SkillX { id: c.skill_id } }
                                    td { {why(c, &cur.horses[i], cur.goal, n)} }
                                }
                            }
                        }
                    }
                }
            }
            details { class: "acts",
                summary { "{acts.len()} activations" }
                table { class: "grid",
                    thead {
                        tr {
                            th { class: "num", "Time" }
                            th { "Uma" }
                            th { "Skill" }
                            th { class: "num", "Distance" }
                        }
                    }
                    tbody {
                        for a in acts {
                            tr {
                                td { class: "num", "{ft(a.time)}" }
                                td { class: "who", {who(res, names, a.horse)} }
                                td { SkillX { id: a.skill_id } }
                                td { class: "num", "{a.distance:.0} m" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Why a skill didn't activate: the reason, colored like its `outcome_bar` segment, then the details.
fn why(c: &SkillCheck, h: &HorseRun, goal: f32, n: usize) -> Element {
    let cond = || condition_text(c.skill_id);
    let (head, detail) = match &c.outcome {
        SkillOutcome::Activated => return rsx! {},
        SkillOutcome::WitCheck { chance } => ("Failed the wit check", format!("{chance:.0}% chance to pass.")),
        SkillOutcome::CantHere(t) => ("Can't activate in this race", format!("Needs {}.", t.join(" + "))),
        SkillOutcome::Precondition => ("Precondition never held", cond()),
        SkillOutcome::NotMet { window: Some((a, b)) } => {
            let place = h.place_by_progress[((a / goal * 100.0).round() as usize).min(100)];
            let at = if n > 1 { format!(", order {place} of {n} there") } else { String::new() };
            ("Conditions never all held", format!("{}. Random point {a:.0}-{b:.0}m{at}.", cond()))
        }
        SkillOutcome::NotMet { window: None } => ("Conditions never all held", cond()),
    };
    rsx! {
        div { class: "why o{c.outcome.kind()}",
            b { "{head}" }
            div { class: "muted small", "{detail}" }
        }
    }
}

/// Finish time of the horse finishing `place - 1`.
fn ahead_of(run: &Run, place: usize) -> f32 {
    run.horses.iter().find(|x| x.place + 1 == place).map_or(0.0, |x| x.finish_time)
}
