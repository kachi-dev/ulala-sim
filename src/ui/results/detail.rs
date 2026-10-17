//! Batch uma detail: places, vs baseline, last spurt and HP, skills.

use super::*;
use crate::charts::*;

/// Percentile `p` (10..=90) of `d`, linear between its stored p10 / p25 / p50 / p75 / p90.
// ponytail: interpolated in-between percentiles for the fan's inner layers; store more in Dist if they look off.
fn pctl(d: &Dist, p: f32) -> f32 {
    let k = [(10.0, d.p10), (25.0, d.p25), (50.0, d.p50), (75.0, d.p75), (90.0, d.p90)];
    let i = k.windows(2).position(|w| p <= w[1].0).unwrap_or(3);
    let ((p0, v0), (p1, v1)) = (k[i], k[i + 1]);
    v0 + (v1 - v0) * (p.clamp(p0, p1) - p0) / (p1 - p0)
}

/// Means over the course, with a p10-p90 fan (p10-p90, p20-p80, p30-p70, p40-p60).
fn race_traces(h: &HorseStats) -> RaceTraces {
    let t = |v: &[Dist]| Trace {
        line: v.iter().map(|d| d.mean).collect(),
        fan: [10.0, 20.0, 30.0, 40.0].map(|p| (v.iter().map(|d| pctl(d, p)).collect(), v.iter().map(|d| pctl(d, 100.0 - p)).collect())).to_vec(),
    };
    RaceTraces { place: t(&h.place_by_progress), speed: t(&h.speed_by_progress), hp: t(&h.hp_by_progress) }
}

pub(super) fn horse_detail(
    s: S,
    res: &Res,
    i: usize,
    cur: &Arc<Batch>,
    base: Option<&Arc<Batch>>,
    diff: Option<&BatchDiff>,
) -> Element {
    let Some(race) = s.race.cloned() else { return rsx! {} };
    let (mut focus, mut skill_sel, mut detail_tab) = (s.focus, s.skill_sel, s.detail_tab);
    let tab = *detail_tab.read();
    let h = &cur.horses[i];
    let b = base.zip(res.base_of(i)).map(|(b, j)| &b.horses[j]);
    // Charts of one uma: the accent; her team / gate colour stays on her icon.
    let color = "var(--accent)".to_string();
    let n = cur.horses.len();
    let cv = &res.cv;

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
    // ▲/▼ vs the baseline (nothing without one): `f` of this run minus the baseline's, worded by `txt`.
    let vs = |f: &dyn Fn(&HorseStats) -> f32, lower_better: bool, txt: fn(f32) -> String| {
        b.map(|b| {
            let d = f(h) - f(b);
            delta(d, lower_better, txt(d.abs()))
        })
    };
    // Race events the uma was ever in, here or on the baseline.
    let events: Vec<&RaceEvent> = EVENTS.iter().filter(|e| h.states[e.kind as usize].0 > 0.0 || b.is_some_and(|b| b.states[e.kind as usize].0 > 0.0)).collect();
    let strip: Vec<(String, String, Vec<f32>)> =
        h.skills.iter().map(|k| (skill_info(k.skill_id).name, color.clone(), k.distances.clone())).collect();

    rsx! {
        section { class: "card detail",
            div { class: "card-h",
                UmaPick { i }
                span { class: "muted", "{style_name(&race, &res.input.horses[i])} · over {cur.seeds.len()} races" }
                div { class: "grow" }
                UmaLinks { i, card: res.input.horses[i].card_id }
                button { class: "btn icon", title: "Close", onclick: move |_| focus.set(None), {icon("x")} }
            }
            div { class: "dtabs-row",
                div { class: "seg small dtabs",
                    for (t, label) in [(DetailTab::Summary, "Summary"), (DetailTab::Skills, "Skills"), (DetailTab::Stamina, "Stamina"), (DetailTab::Wit, "Wit"), (DetailTab::Mechanics, "Other Mechanics")] {
                        button {
                            class: if tab == t { "on" },
                            onclick: move |_| {
                                detail_tab.set(t);
                                // The open skill also marks the course map; it belongs to the Skills tab.
                                if t != DetailTab::Skills {
                                    skill_sel.set(None);
                                }
                            },
                            "{label}"
                        }
                    }
                }
                if tab == DetailTab::Wit {
                    crate::ui::wit::WitRun { i }
                }
                if tab == DetailTab::Stamina {
                    crate::ui::stamina::StaminaRun { i }
                }
                if tab == DetailTab::Mechanics {
                    crate::ui::mechanics::MechanicsRun { i }
                }
            }
            if tab == DetailTab::Summary {
                div { class: "kpirow",
                    div { class: "kpis",
                        div { b { "{pct(h.win_rate)}" } div { class: "kpi2", span { "win" } {vs(&|x| x.win_rate, false, pp)} } }
                        div { b { "{h.place.mean:.2}" } div { class: "kpi2", span { "avg place" } {vs(&|x| x.place.mean, true, |v| format!("{v:.2}"))} } }
                        div { b { "{ft(h.finish_time.p50)}" } div { class: "kpi2", span { "median time" } {vs(&|x| x.finish_time.p50, true, |v| format!("{v:.3}s"))} } }
                        div { b { "{pct(h.full_spurt_rate)}" } div { class: "kpi2", span { "full spurt" } {vs(&|x| x.full_spurt_rate, false, pp)} } }
                        div { b { "{pct(h.hp_out_rate)}" } div { class: "kpi2", span { "ran out of HP" } {vs(&|x| x.hp_out_rate, true, pp)} } }
                        for e in events {
                            {
                                let k = e.kind as usize;
                                let (rate, secs) = h.states[k];
                                rsx! {
                                    div { key: "{k}", title: "{e.hint}",
                                        div { class: "kpi2",
                                            b { "{pct(rate)}" }
                                            {vs(&move |x| x.states[k].0, !e.more_good, pp)}
                                        }
                                        div { class: "kpi2",
                                            span { "{e.label.to_lowercase()}" }
                                            span { "{secs:.1}s " {vs(&move |x| x.states[k].1, !e.more_good, |v| format!("{v:.1}s"))} }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if n > 1 {
                        div { class: "placebox",
                            span { class: "muted small", "Finishing place" if b.is_some() { " (outline: baseline)" } }
                            BarChart { bars, compare, color: color.clone() }
                        }
                    }
                }
                if let Some(d) = diff {
                    {vs_baseline(d, res.ed_mask.get(i) == Some(&true), &color)}
                }
                h4 { "Over the course" span { class: "muted small", " (mean, shaded p10 to p90)" } }
                RaceChart {
                    cur: race_traces(h),
                    base: b.map(race_traces),
                    field_size: n,
                    course_distance: cv.distance,
                    phases: cv.phases.to_vec(),
                    corners: cv.corner_spans(),
                    straights: cv.straights.iter().map(|c| (c.start, c.end)).collect::<Vec<_>>(),
                }
            }
            if tab == DetailTab::Stamina {
                {crate::ui::stamina::stamina_cards(h, b, cv)}
                crate::ui::stamina::StaminaOut { i }
            }
            if tab == DetailTab::Wit {
                {crate::ui::wit::wit_cards(h, b, diff)}
                crate::ui::wit::WitOut { i }
            }
            if tab == DetailTab::Mechanics {
                {crate::ui::mechanics::mechanics_cards(h, b)}
                crate::ui::mechanics::MechanicsOut { i }
            }
            if tab == DetailTab::Skills {
                {outcome_legend()}
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
                                            td {
                                                if let Some(o) = outcomes(id) { div { class: "oline", {outcome_bar(&o)} } }
                                                if b.is_some_and(|b| pct(rate(b, id)) != pct(r)) { div { class: "was", visibility: "hidden", "was" } }
                                            }
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
}

/// The detail's uma as a dropdown of the run's umas in finishing order (a batch: by mean place): costume icon, name,
/// finishing place / rank, team tags and costume. Picking one focuses it, which also selects it in the field.
#[component]
pub(super) fn UmaPick(i: usize) -> Element {
    let s = use_context::<S>();
    let mut open = use_signal(|| false);
    let Some(res) = s.res.cloned() else { return rsx! {} };
    let mut order: Vec<usize> = (0..res.input.horses.len()).collect();
    match &res.kind {
        Kind::Batch { cur, .. } => order.sort_by(|&a, &b| cur.horses[a].place.mean.total_cmp(&cur.horses[b].place.mean)),
        Kind::Single { cur, .. } => order.sort_by_key(|&a| cur.horses[a].place),
    }
    let row = |j: usize, rank: usize, size: &str| {
        let h = &res.input.horses[j];
        rsx! {
            {horse_icon(h, Some(&res.colors[j]), size)}
            div { class: "upick-who",
                div {
                    b { "{horse_name(h)}" }
                    span { class: "gate", "#{rank}" }
                    {team_tags(&res.input, res.player, j)}
                }
                div { class: "muted small", "{card_title(h.card_id)}" }
            }
        }
    };
    let rank = |j: usize| order.iter().position(|&k| k == j).unwrap_or(0) + 1;
    rsx! {
        div { class: "upick",
            button { class: "upick-btn", title: "Show another uma", onclick: move |_| open.toggle(),
                {row(i, rank(i), "md")}
                {icon("down")}
            }
            if open() {
                div { class: "menu-catch", onclick: move |_| open.set(false) }
                div { class: "upick-list",
                    for (r, &j) in order.iter().enumerate() {
                        {
                            let card = res.input.horses[j].card_id;
                            rsx! {
                                button {
                                    key: "{j}",
                                    class: if j == i { "on" },
                                    onclick: move |_| {
                                        open.set(false);
                                        focus_gate(s, j, card);
                                    },
                                    {row(j, r + 1, "sm")}
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// CSS class colouring an `OutcomeKind` (bar segment, legend swatch, reason text).
pub(in crate::ui) fn outcome_class(k: OutcomeKind) -> &'static str {
    match k {
        OutcomeKind::Activated => "o-act",
        OutcomeKind::WitCheck => "o-wit",
        OutcomeKind::CantHere => "o-cant",
        OutcomeKind::NotMet => "o-notmet",
    }
}

/// Legend of `outcome_bar`'s colours.
fn outcome_legend() -> Element {
    rsx! {
        div { class: "olegend muted small",
            for k in OutcomeKind::ALL {
                span { title: "{k.hint()}", i { class: outcome_class(k) } "{k.label()}" }
            }
        }
    }
}

/// Uma `horse`'s skills over batch stats `h` (her skills in the game's order, then anything else she activated): how
/// often each activated and why it didn't, with the legend.
pub(in crate::ui) fn skill_activations(h: &HorseStats, horse: &HorseInput) -> Element {
    let mut ids: Vec<i32> = game_order(&horse.skills).into_iter().map(|(_, k)| k.skill_id).collect();
    ids.extend(h.skills.iter().map(|k| k.skill_id).filter(|id| !horse.skills.iter().any(|k| k.skill_id == *id)));
    let stat = |id: i32| h.skills.iter().find(|k| k.skill_id == id);
    rsx! {
        {outcome_legend()}
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
                    tr { key: "{id}",
                        td { {skill_row(&skill_info(id), None, "")} }
                        td { class: "num", "{pct(stat(id).map_or(0.0, |k| k.rate))}" }
                        td {
                            if let Some(k) = stat(id) { div { class: "oline", {outcome_bar(&k.outcomes)} } }
                        }
                    }
                }
            }
        }
    }
}

/// Stacked bar of `SkillStat::outcomes`, one color per `OutcomeKind`.
fn outcome_bar(o: &[f32; 4]) -> Element {
    rsx! {
        div { class: "obar",
            for (k, f) in OutcomeKind::ALL.into_iter().zip(o) {
                if *f > 0.0 {
                    div { class: outcome_class(k), style: "width: {f * 100.0:.2}%", title: "{k.label()}: {pct(*f)}. {k.hint()}." }
                }
            }
        }
    }
}

/// "Edit uma" / "Measure skill gains" for gate `i` of a past run: the same uma in the current field (same gate if it
/// still is that costume, else the first match). A component, so following the field doesn't re-render the results.
#[component]
pub(super) fn UmaLinks(i: usize, card: i32) -> Element {
    let s = use_context::<S>();
    // NPCs aren't in the setup.
    if card == 0 {
        return rsx! {};
    }
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
                for (k, f) in OutcomeKind::ALL.into_iter().zip(st.outcomes) {
                    if k != OutcomeKind::Activated && f > 0.0 {
                        div { class: outcome_class(k), title: "{k.hint()}", b { "{pct(f)}" } span { "{k.label().to_lowercase()}" } }
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

/// A spurt kind as the results show it.
pub(in crate::ui) struct SpurtStyle {
    pub label: &'static str,
    pub hint: &'static str,
    pub color: &'static str,
}

pub(in crate::ui) fn spurt_style(k: SpurtKind) -> SpurtStyle {
    let (label, hint, color) = match k {
        SpurtKind::Full => ("Full spurt", "Max spurt speed from the final-leg start", "var(--good)"),
        SpurtKind::Delayed => ("Delayed", "Max speed, but started after the final-leg start", "var(--accent-2)"),
        SpurtKind::Reduced => ("Reduced", "Started on time, but at less than max speed (not a full spurt)", "#e8ae1c"),
        SpurtKind::DelayedReduced => ("Delayed + reduced", "Started late and below max speed", "var(--accent)"),
        SpurtKind::NoSpurt => ("No spurt", "Never switched to a last spurt", "var(--bad)"),
    };
    SpurtStyle { label, hint, color }
}

/// Batch uma detail: when the last spurt starts vs the expected point, how good it was, and HP left at the goal.
pub(in crate::ui) fn spurt_and_hp(h: &HorseStats, cv: &CourseView, color: &str) -> Element {
    let fl = cv.final_leg_start;
    let on_time: f32 = SpurtKind::ALL.iter().zip(h.spurt_kinds).filter(|(k, _)| **k != SpurtKind::NoSpurt && !k.delayed()).map(|x| x.1).sum();
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
                    for (k, f) in SpurtKind::ALL.into_iter().zip(h.spurt_kinds) {
                        if f > 0.0 {
                            {
                                let st = spurt_style(k);
                                rsx! {
                                    div { class: "kb", title: "{st.hint}",
                                        span { class: "kb-l", "{st.label}" }
                                        div { class: "kb-t", div { class: "kb-f", style: "width: {f * 100.0:.2}%; background: {st.color}" } }
                                        b { class: "kb-v", "{pct(f)}" }
                                    }
                                }
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
