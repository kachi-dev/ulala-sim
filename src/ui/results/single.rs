//! Single-run results: the result card (Summary: finish table; More: every uma's positions over the course and skill
//! activations), and the focused uma's detail (Summary: over the course with events; Skills: activations and misses).

use super::*;
use super::detail::{UmaLinks, UmaPick, outcome_class};
use crate::charts::*;

pub(super) fn single_view(s: S, res: &Res, names: &[String], cur: &Run, base: Option<&Run>) -> Element {
    let (mut focus, mut detail_tab, mut run_tab) = (s.focus, s.detail_tab, s.run_tab);
    // A single run has only Summary and Skills: the batch-only subtabs show Summary.
    let tab = if *detail_tab.read() == DetailTab::Skills { DetailTab::Skills } else { DetailTab::Summary };
    let rtab = *run_tab.read();
    let n = cur.horses.len();
    let f = (*focus.read()).filter(|&f| f < n);
    let diffs = base.map(|b| diff_runs(b, cur));
    let ed = &res.ed_mask;
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by_key(|&i| cur.horses[i].place);
    let improved = diffs.as_ref().is_some_and(|d| d.iter().zip(ed).any(|(d, &m)| m && d.place.1 < d.place.0));

    // Focused: that uma's chart. Else every uma's position, edited ones on top with their baseline dashed.
    let me = f.unwrap_or(order[0]);
    let line = |run: &Run, i: usize, dashed: bool| PlaceSeries {
        label: if dashed { format!("{} (baseline)", names[i]) } else { names[i].clone() },
        color: res.colors[i].clone(),
        line: run.horses[i].place_by_progress.iter().map(|&p| p as f32).collect(),
        inner: None,
        outer: None,
        dashed,
        highlight: ed[i],
    };
    let mut series: Vec<PlaceSeries> = base.map_or(vec![], |b| (0..n).filter(|&i| ed[i]).map(|i| line(b, i, true)).collect());
    series.extend((0..n).map(|i| line(cur, i, false)));
    let traces = |run: &Run| {
        let h = &run.horses[me];
        let t = |line: Vec<f32>| Trace { line, fan: vec![] };
        RaceTraces {
            place: t(h.place_by_progress.iter().map(|&p| p as f32).collect()),
            speed: t(h.speed_by_progress.clone()),
            hp: t(h.hp_by_progress.clone()),
        }
    };

    let cv = &res.cv;
    let style = match (f, s.race.cloned()) {
        (Some(f), Some(race)) => format!("{} · seed {}", style_name(&race, &res.input.horses[f]), cur.seed),
        _ => String::new(),
    };

    rsx! {
        section { class: "card",
            div { class: "card-h",
                h3 { if base.is_some() { "Result: edited vs baseline" } else { "Result" } }
                span { class: "muted", "seed {cur.seed} · click a row to focus an uma" }
                if improved { img { class: "great", src: "/a/great.webp", alt: "Great!" } }
            }
            div { class: "seg small dtabs",
                for (t, label) in [(RunTab::Summary, "Summary"), (RunTab::More, "More")] {
                    button { class: if rtab == t { "on" }, onclick: move |_| run_tab.set(t), "{label}" }
                }
            }
            if rtab == RunTab::Summary {
                div { class: "scroll-x",
                    table { class: "grid results",
                        thead {
                            tr {
                                th { "Place" }
                                th { "Uma" }
                                th { "Style" }
                                th { class: "num", "Time" }
                                th { class: "num c-behind", "Behind" }
                                th { class: "num", title: "✓ full spurt: on time and at max speed. ✗ (+metres late after the final-leg start, -m/s below max spurt speed)", "Last spurt" }
                                th { class: "num", "HP" }
                                for e in EVENTS.iter() { th { class: "num", title: "{e.hint}. Seconds in this race.", "{e.label}" } }
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
                                            class: if f == Some(i) { "sel" },
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
                                            td { {style_badge(res.input.horses[i].running_style)} }
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
                                            td { class: "num",
                                                {
                                                    let (ok, label) = spurt_label(h);
                                                    let was = base.map(|b| spurt_label(&b.horses[i]).1).filter(|w| *w != label);
                                                    rsx! {
                                                        span { class: if ok { "ok nowrap" } else { "no nowrap" }, "{label}" }
                                                        if let Some(w) = was { div { class: "was", "was {w}" } }
                                                    }
                                                }
                                            }
                                            td { class: "num",
                                                {
                                                    let label = hp_label(h, cur.goal);
                                                    let was = base.map(|b| hp_label(&b.horses[i], b.goal)).filter(|w| *w != label);
                                                    rsx! {
                                                        span { class: if h.hp_out { "no nowrap" } else { "ok nowrap" }, title: "Died: ran out of HP this far before the goal", "{label}" }
                                                        if let Some(w) = was { div { class: "was", "was {w}" } }
                                                    }
                                                }
                                            }
                                            for e in EVENTS.iter() {
                                                {
                                                    let (kind, more_good) = (e.kind, e.more_good);
                                                    let t = h.state_time[kind as usize];
                                                    rsx! {
                                                        td { class: "num",
                                                            if t > 0.0 { "{t:.1}s" } else { span { class: "muted", "-" } }
                                                            if let Some(b) = base {
                                                                {
                                                                    let was = b.horses[i].state_time[kind as usize];
                                                                    state_arrow(was, t, 0.05, more_good, format!("was {was:.1}s"))
                                                                }
                                                            }
                                                        }
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
            } else {
                h4 { "Over the course" span { class: "muted small", if n > 1 { " (positions of every uma" } if base.is_some() && ed.iter().any(|&m| m) { ", edited ones on top with the baseline dashed" } if n > 1 { ")" } } }
                PlaceChart { series, field_size: n, markers: vec![(cv.final_leg_start / cv.distance * 100.0, "Late race".to_string())] }
                h4 { "Skill activations" span { class: "muted small", " (every uma)" } }
                {activations(res, names, cur, None)}
            }
        }
        if let Some(f) = f {
            section { class: "card detail",
                div { class: "card-h",
                    UmaPick { i: f }
                    span { class: "muted", "{style}" }
                    div { class: "grow" }
                    UmaLinks { i: f, card: res.input.horses[f].card_id }
                    button { class: "btn icon", title: "Close", onclick: move |_| focus.set(None), {icon("x")} }
                }
                div { class: "seg small dtabs",
                    for (t, label) in [(DetailTab::Summary, "Summary"), (DetailTab::Skills, "Skills")] {
                        button { class: if tab == t { "on" }, onclick: move |_| detail_tab.set(t), "{label}" }
                    }
                }
                if tab == DetailTab::Skills {
                    {activations(res, names, cur, Some(f))}
                } else {
                    {event_kpis(&cur.horses[f], base.map(|b| &b.horses[f]))}
                    h4 { "Over the course" }
                    RaceChart {
                        cur: traces(cur),
                        base: base.map(traces),
                        field_size: n,
                        course_distance: cv.distance,
                        phases: cv.phases.to_vec(),
                        corners: cv.corner_spans(),
                        straights: cv.straights.iter().map(|c| (c.start, c.end)).collect::<Vec<_>>(),
                        bands: state_bands(&cur.horses[me]),
                        base_bands: base.map_or(vec![], |b| state_bands(&b.horses[me])),
                    }
                }
            }
        }
    }
}

/// Skill activations of uma `only` (None: every uma): where along the course, what didn't activate and why, and the
/// activations in time order.
fn activations(res: &Res, names: &[String], cur: &Run, only: Option<usize>) -> Element {
    let n = cur.horses.len();
    let cv = &res.cv;
    let strip: Vec<(String, String, Vec<f32>)> = (0..n)
        .filter(|&i| only.is_none_or(|f| f == i))
        .map(|i| {
            let d = cur.activations.iter().filter(|a| a.horse == i).map(|a| a.distance).collect();
            (names[i].clone(), if only.is_some() { "var(--accent)".to_string() } else { res.colors[i].clone() }, d)
        })
        .collect();
    let acts: Vec<&Activation> = cur.activations.iter().filter(|a| only.is_none_or(|f| f == a.horse)).collect();
    let missed: Vec<(usize, &SkillCheck)> = (0..n)
        .filter(|&i| only.is_none_or(|f| f == i))
        .flat_map(|i| cur.horses[i].skills.iter().filter(|c| c.outcome != SkillOutcome::Activated).map(move |c| (i, c)))
        .collect();
    let all = only.is_none() && n > 1;
    rsx! {
        CourseStrip { rows: strip, course_distance: cv.distance, final_leg_start: cv.final_leg_start }
        if !missed.is_empty() {
            details { class: "acts", open: !all,
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
        div { class: "why {outcome_class(c.outcome.kind())}",
            b { "{head}" }
            div { class: "muted small", "{detail}" }
        }
    }
}

/// Seconds in each race event (duel, spot struggle, rushed, blocked, ...) the uma was in, here or on the baseline,
/// with the change from the baseline.
fn event_kpis(h: &HorseRun, b: Option<&HorseRun>) -> Element {
    let secs = |x: &HorseRun, k: u8| x.state_time[k as usize];
    let shown: Vec<&RaceEvent> = EVENTS.iter().filter(|e| secs(h, e.kind) > 0.0 || b.is_some_and(|b| secs(b, e.kind) > 0.0)).collect();
    if shown.is_empty() {
        return rsx! {};
    }
    rsx! {
        div { class: "kpis",
            for e in shown {
                {
                    let t = secs(h, e.kind);
                    let d = b.map(|b| t - secs(b, e.kind)).filter(|d| d.abs() >= 0.05);
                    rsx! {
                        div { key: "{e.kind}", title: "{e.hint}",
                            b { "{t:.1}s " if let Some(d) = d { {delta(d, !e.more_good, format!("{:.1}s", d.abs()))} } }
                            span { "{e.label.to_lowercase()}" }
                        }
                    }
                }
            }
        }
    }
}

/// "✓" for a full spurt, else "✗ (+metres late, -m/s below max)" with only what fell short, or "✗ (no spurt)".
fn spurt_label(h: &HorseRun) -> (bool, String) {
    match h.spurt_kind {
        SpurtKind::Full => (true, "✓".into()),
        SpurtKind::NoSpurt => (false, "✗ (no spurt)".into()),
        k => {
            let mut parts = vec![];
            if k.delayed() {
                parts.push(format!("+{:.1}m", h.spurt_delay));
            }
            if k.reduced() {
                parts.push(format!("-{:.2}", h.max_spurt_speed - h.spurt_speed));
            }
            (false, format!("✗ ({})", parts.join(", ")))
        }
    }
}

/// "Survived", or "Died -24m" (how far before the goal HP ran out).
fn hp_label(h: &HorseRun, goal: f32) -> String {
    match (h.hp_out, h.hp_out_at) {
        (false, _) => "Survived".into(),
        (true, Some(at)) => format!("Died -{:.0}m", goal - at),
        (true, None) => "Died".into(),
    }
}

/// Finish time of the horse finishing `place - 1`.
fn ahead_of(run: &Run, place: usize) -> f32 {
    run.horses.iter().find(|x| x.place + 1 == place).map_or(0.0, |x| x.finish_time)
}
