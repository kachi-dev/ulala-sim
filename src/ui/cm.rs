//! CM tab: the save owner's Champions Meeting teams, from horseACT's saves. Simulate runs each saved race N times for
//! what the sim expects (win rates, places, positions), shown against what really happened.

use super::*;
use crate::charts::{PlaceChart, PlaceSeries};

/// Races list filter of the team detail.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum RaceFilter {
    All,
    Won,
    Lost,
    Upsets,
}

/// Mean place at the end of the early, mid and late race (% of the course).
const PHASE_ENDS: [usize; 3] = [17, 67, 83];

/// A Champions Meeting save is indexed (the CM tab shows).
pub(super) fn has_cm(s: S) -> bool {
    let meta = s.meta.read();
    s.saves.read().iter().any(|e| e.category == CM_CATEGORY && meta.get(&e.path).is_some_and(|m| cm_of_save(m.course_id, &e.date).is_some()))
}

/// "Jul 15" from "2026-07-15".
fn short_date(d: &str) -> String {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let m = d.get(5..7).and_then(|m| m.parse::<usize>().ok()).and_then(|m| MONTHS.get(m.wrapping_sub(1)));
    let day = d.get(8..10).and_then(|x| x.parse::<u32>().ok());
    match (m, day) {
        (Some(m), Some(day)) => format!("{m} {day}"),
        _ => d.to_string(),
    }
}

/// "Jul 15 to Jul 16" over races (newest first), or one date.
fn date_range(races: &[CmRace]) -> String {
    let (Some(new), Some(old)) = (races.first(), races.last()) else { return String::new() };
    if new.date == old.date { short_date(old.date) } else { format!("{} to {}", short_date(old.date), short_date(new.date)) }
}

/// Team member colours: the player team's hues.
fn hue(j: usize) -> &'static str {
    TEAM_HUES[0][j % 6]
}

/// Header cells for `win_cells`.
fn win_heads(sim: bool) -> Element {
    rsx! {
        th { class: "num", "Win rate" }
        if sim {
            th { class: "num", title: "Win rate the sim expects from these races", "Sim" }
            th { class: "num", title: "Win rate minus the sim's: green = won more than the sim expects", "Luck" }
        }
    }
}

/// Win rate (and wins); with a sim, the sim's win rate and the luck (actual minus sim).
fn win_cells(wins: usize, races: usize, sim: Option<f32>) -> Element {
    let w = wins as f32 / races.max(1) as f32;
    rsx! {
        td { class: "num",
            b { "{pct(w)}" }
            span { class: "muted small", " {wins}" }
        }
        if let Some(p) = sim {
            td { class: "num", "{pct(p)}" }
            td { class: "num", {pp_delta(p, w, false)} }
        }
    }
}

/// Wins / Sim expects / Luck cards.
fn win_kpis(wins: usize, races: usize, sim: Option<&SimTally>) -> Element {
    let rate = pct(wins as f32 / races.max(1) as f32);
    let d = sim.map_or(0.0, |x| wins as f32 - x.expected);
    rsx! {
        div {
            span { "Wins" }
            b { "{wins} of {races}" }
            div { class: "cap", "{rate} win rate" }
        }
        if let Some(x) = sim {
            div {
                span { "Sim expects" }
                b { "{x.expected:.1} wins" }
                div { class: "cap", "{pct(x.win)} win rate" }
            }
            div {
                span { "Luck" }
                b { {delta(d, false, format!("{:.1} wins", d.abs()))} }
                div { class: "cap", if d >= 0.0 { "above the sim" } else { "below the sim" } }
            }
        }
    }
}

/// Places histogram: actual (solid), the sim's (outline) when there is one.
fn place_hist(act: &[f32], sim: Option<&[f32]>) -> Element {
    let n = act.len().max(sim.map_or(0, |s| s.len()));
    let at = |v: &[f32], k: usize| v.get(k).copied().unwrap_or(0.0);
    let max = (0..n).map(|k| at(act, k).max(sim.map_or(0.0, |s| at(s, k)))).fold(0.0001, f32::max);
    let tip = |k: usize| match sim {
        Some(s) => format!("{}: {}, sim {}", k + 1, pct(at(act, k)), pct(at(s, k))),
        None => format!("{}: {}", k + 1, pct(at(act, k))),
    };
    rsx! {
        div { class: "mini vs",
            for k in 0..n {
                div { title: "{tip(k)}",
                    i { style: "height: {at(act, k) / max * 100.0}%" }
                    if let Some(s) = sim { u { style: "height: {at(s, k) / max * 100.0}%" } }
                }
            }
        }
    }
}

fn icons(members: &[TeamMember], size: &str) -> Element {
    rsx! {
        span { class: "stack",
            for m in members { {uma_icon(m.card, m.chara, None, size)} }
        }
    }
}

/// What changed from the team before: "Curren Chan → Symboli Rudolf", "Haru Urara retrained".
fn swaps(prev: &CmTeam, cur: &CmTeam) -> Vec<String> {
    let gone = prev.members.iter().filter(|m| !cur.key.contains(&member_id(m)));
    let came = cur.members.iter().filter(|m| !prev.key.contains(&member_id(m)));
    gone.zip(came)
        .map(|(a, b)| if a.chara == b.chara { format!("{} retrained", chara_name(a.chara)) } else { format!("{} → {}", chara_name(a.chara), chara_name(b.chara)) })
        .collect()
}

#[component]
pub fn CmTab() -> Element {
    let mut s = use_context::<S>();
    let running = use_busy(s);
    let saves = s.saves.read();
    let meta = s.meta.read();
    let sims = s.cm_sim.read();
    // Every indexed CM save with its CM, newest first.
    let all: Vec<(&CmPreset, CmRace)> = saves
        .iter()
        .filter(|e| e.category == CM_CATEGORY)
        .filter_map(|e| {
            let m = meta.get(&e.path)?;
            let p = cm_of_save(m.course_id, &e.date)?;
            Some((p, CmRace { path: &e.path, date: &e.date, meta: m, sim: sims.get(&e.path).map(|x| &**x) }))
        })
        .collect();
    let cms: Vec<&CmPreset> = cm_presets().iter().filter(|p| all.iter().any(|(q, _)| q.id == p.id)).collect();
    let sel_id = *s.cm_sel.read();
    let sel = cms.iter().find(|p| p.id == sel_id).copied();
    let races: Vec<CmRace> = all.iter().filter(|(p, _)| sel.is_none_or(|q| q.id == p.id)).map(|x| x.1).collect();
    if races.is_empty() {
        return rsx! { section { class: "card", p { class: "muted", "No Champions Meeting saves yet." } } };
    }

    let n = *s.cm_n.read();
    let todo: Vec<(PathBuf, Vec<usize>)> = races.iter().map(|r| (r.path.to_path_buf(), r.meta.team.iter().map(|m| m.horse).collect())).collect();
    let work = todo.iter().map(|_| n * 9).sum::<usize>();
    let eta = eta(s, work);
    let all_done = races.iter().all(|r| r.sim.is_some());
    let label = if all_done { "Run again" } else if sel.is_none() { "Simulate all" } else { "Simulate" };
    let start = move |_| {
        let todo = todo.clone();
        let total = todo.len() * n;
        let label = format!("Simulating {} races × {n}", thousands(todo.len()));
        measure(s, label, total, work, move |w| sim_races(&todo, n, w), |mut s, (out, err)| {
            s.cm_sim.write().extend(out.into_iter().map(|(p, r)| (p, Arc::new(r))));
            if let Some(e) = err {
                s.err.set(Some(e));
            }
        });
    };
    let t = tally(&races);
    let course = races[0].meta.course.clone();

    rsx! {
        section { class: "card",
            div { class: "card-h",
                select {
                    class: "cm",
                    onchange: move |e| {
                        s.cm_sel.set(e.value().parse().unwrap_or(0));
                        s.cm_team.set(None);
                    },
                    option { value: "0", "All CMs" }
                    for p in cms.iter() {
                        option { value: "{p.id}", selected: sel.is_some_and(|q| q.id == p.id), "CM {p.id} - {p.name}" }
                    }
                }
                if let Some(p) = sel { span { class: "muted", "{course} · {cm_conditions(p)}" } }
            }
            div { class: "runbar",
                button { class: "btn", disabled: running(), onclick: start, "{label}" }
                label { class: "runn",
                    {seeds_input("cmn", s.cm_n, 10, 5000)}
                    span { class: "muted", "per race" }
                }
                span { class: "muted small", "{thousands(races.len())} races · {thousands(races.len() * n)} runs, {eta}" }
                Progress {}
            }
            div { class: "kpis",
                {win_kpis(t.wins, t.races, t.sim.as_ref())}
                if let Some(((_, chara), w)) = t.top {
                    div { span { "Top uma" } b { "{chara_name(chara)} " span { class: "muted small", "{w} wins" } } }
                }
            }
            if sel.is_none() {
                {cm_table(s, &cms, &all, t.sim.is_some())}
            } else {
                {teams_view(s, &races)}
            }
        }
        if sel.is_some() {
            {team_detail(s, &races)}
        }
    }
}

/// All CMs: one row each.
fn cm_table(mut s: S, cms: &[&CmPreset], all: &[(&CmPreset, CmRace)], sim: bool) -> Element {
    rsx! {
        div { class: "scroll-x",
            table { class: "grid",
                thead {
                    tr {
                        th { "CM" }
                        th { "Teams" }
                        th { class: "num", "Races" }
                        {win_heads(sim)}
                        th { class: "num", title: "The team's best place, mean", "Best place" }
                        th { "Top uma" }
                    }
                }
                tbody {
                    for p in cms.iter().copied() {
                        {
                            let rs: Vec<CmRace> = all.iter().filter(|(q, _)| q.id == p.id).map(|x| x.1).collect();
                            let t = tally(&rs);
                            let teams = cm_teams(&rs);
                            let id = p.id;
                            rsx! {
                                tr {
                                    key: "{id}",
                                    onclick: move |_| {
                                        s.cm_sel.set(id);
                                        s.cm_team.set(None);
                                    },
                                    td {
                                        b { "CM {p.id} - {p.name}" }
                                        div { class: "muted small", "{rs[0].meta.course} · {date_range(&rs)}" }
                                    }
                                    td {
                                        div { class: "stacks",
                                            for tm in teams.iter() { {icons(&tm.members, "sm")} }
                                        }
                                    }
                                    td { class: "num", "{t.races}" }
                                    {win_cells(t.wins, t.races, t.sim.as_ref().map(|x| x.win))}
                                    td { class: "num", "{t.best_place:.2}" }
                                    td {
                                        if let Some(((card, chara), w)) = t.top {
                                            div { class: "whobox",
                                                {uma_icon(card, chara, None, "sm")}
                                                span { "{chara_name(chara)} " span { class: "muted small", "{w} wins" } }
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

/// One CM: its teams.
fn teams_view(mut s: S, races: &[CmRace]) -> Element {
    let teams = cm_teams(races);
    let sel = s.cm_team.cloned();
    let sim = races.iter().all(|r| r.sim.is_some());
    rsx! {
        div { class: "card-h teams-h",
            h3 { "{teams.len()} team" if teams.len() != 1 { "s" } }
            span { class: "muted small", "click a team for details" }
        }
        div { class: "scroll-x",
            table { class: "grid",
                thead {
                    tr {
                        th { "Team" }
                        th { class: "num", "Races" }
                        {win_heads(sim)}
                        th { class: "num", title: "The team's best place, mean", "Best place" }
                        th { "Wins by" }
                        if sim { th { class: "num", title: "Lost at {UPSET_LOST * 100.0:.0}%+ or won at {UPSET_WON * 100.0:.0}% or less", "Upsets" } }
                    }
                }
                tbody {
                    for (i, tm) in teams.iter().enumerate() {
                        {
                            let rs: Vec<CmRace> = tm.races.iter().map(|&k| races[k]).collect();
                            let t = tally(&rs);
                            let ms = member_tallies(tm, races);
                            let key = tm.key.clone();
                            let on = sel.as_ref() == Some(&tm.key);
                            let sw = if i > 0 { swaps(&teams[i - 1], tm) } else { vec![] };
                            let wins: usize = ms.iter().map(|m| m.wins).sum();
                            rsx! {
                                tr {
                                    key: "{i}",
                                    class: if on { "sel" },
                                    onclick: move |_| s.cm_team.set(if on { None } else { Some(key.clone()) }),
                                    td {
                                        div { class: "whobox",
                                            {icons(&tm.members, "sm")}
                                            div {
                                                b { "Team {i + 1}" }
                                                div { class: "muted small",
                                                    "{date_range(&rs)}"
                                                    for x in sw { span { class: "tag line", "{x}" } }
                                                }
                                            }
                                        }
                                    }
                                    td { class: "num", "{t.races}" }
                                    {win_cells(t.wins, t.races, t.sim.as_ref().map(|x| x.win))}
                                    td { class: "num", "{t.best_place:.2}" }
                                    td {
                                        if wins > 0 {
                                            div { class: "share",
                                                for (j, m) in ms.iter().enumerate().filter(|x| x.1.wins > 0) {
                                                    div { style: "width: {m.wins as f32 / wins as f32 * 100.0}%; background: {hue(j)}" }
                                                }
                                            }
                                            div { class: "sharecap",
                                                {ms.iter().filter(|m| m.wins > 0).map(|m| format!("{} {}", chara_name(m.member.chara), m.wins)).collect::<Vec<_>>().join(" · ")}
                                            }
                                        }
                                    }
                                    if let Some(x) = &t.sim { td { class: "num", "{x.upsets.0 + x.upsets.1}" } }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The selected team: per-uma results, the sim's positions and win chances, the races.
fn team_detail(s: S, races: &[CmRace]) -> Element {
    let teams = cm_teams(races);
    let sel = s.cm_team.cloned();
    let Some((i, tm)) = teams.iter().enumerate().find(|(_, t)| Some(&t.key) == sel.as_ref()) else { return rsx! {} };
    let rs: Vec<CmRace> = tm.races.iter().map(|&k| races[k]).collect();
    let t = tally(&rs);
    let ms = member_tallies(tm, races);
    let sim = t.sim.as_ref();
    let filter = *s.cm_filter.read();
    let shown: Vec<&CmRace> = rs
        .iter()
        .filter(|r| match filter {
            RaceFilter::All => true,
            RaceFilter::Won => r.meta.won,
            RaceFilter::Lost => !r.meta.won,
            RaceFilter::Upsets => r.upset() == Some(true),
        })
        .collect();
    // Lost races by the team's best place: "2nd 6 · 4th 1".
    let mut lost: Vec<usize> = rs.iter().filter(|r| !r.meta.won).map(|r| r.best()).collect();
    lost.sort();
    lost.dedup();
    let lost_by = lost.iter().map(|&p| format!("{} {}", ordinal(p), rs.iter().filter(|r| !r.meta.won && r.best() == p).count())).collect::<Vec<_>>().join(" · ");
    let field = ms.first().map_or(9, |m| m.place_hist.len());
    let seg = move |f: RaceFilter, label: String| {
        let mut sig = s.cm_filter;
        rsx! { button { class: if filter == f { "on" }, onclick: move |_| sig.set(f), "{label}" } }
    };
    let won = t.wins;
    rsx! {
        section { class: "card", id: "cm-team",
            div { class: "card-h",
                {icons(&tm.members, "md")}
                div {
                    h3 { "Team {i + 1}" }
                    div { class: "muted small", "{date_range(&rs)} · {t.races} races" if sim.is_some() { " · {s.cm_n.peek()} sims each" } }
                }
            }
            div { class: "kpis",
                {win_kpis(won, t.races, sim)}
                div {
                    span { "Best place, avg" }
                    b { "{t.best_place:.2}" }
                    if let Some(x) = sim { div { class: "cap", "sim {x.best_place:.2}" } }
                }
                if !lost_by.is_empty() {
                    div {
                        span { "Lost" }
                        b { "{t.races - won}" }
                        div { class: "cap", "{lost_by}" }
                    }
                }
                if let Some(x) = sim {
                    div {
                        span { "Upsets" }
                        b { "{x.upsets.0 + x.upsets.1}" }
                        div { class: "cap", "{x.upsets.0} lost, {x.upsets.1} won" }
                    }
                }
            }
            h4 { "Umas" }
            div { class: "scroll-x",
                table { class: "grid",
                    thead {
                        tr {
                            th { "Uma" }
                            {win_heads(sim.is_some())}
                            th { class: "num", "Avg place" }
                            if sim.is_some() { th { class: "num", title: "Average place the sim expects", "Sim" } }
                            th { "Places 1 to {field}" }
                            if sim.is_some() {
                                th { title: "Mean place at the end of the early, mid and late race", "Early · Mid · Late" }
                                th { class: "num", title: "Share of races with a full spurt: on time and at max speed", "Last spurt" }
                                th { class: "num", "HP" }
                            }
                        }
                    }
                    tbody {
                        for m in ms.iter() {
                            tr { key: "{member_id(&m.member)}",
                                td {
                                    div { class: "whobox",
                                        {uma_icon(m.member.card, m.member.chara, None, "sm")}
                                        div {
                                            b { "{chara_name(m.member.chara)}" }
                                            div { class: "muted small", "{style_label(m.member.style)}" }
                                        }
                                    }
                                }
                                {win_cells(m.wins, t.races, m.sim.as_ref().map(|x| x.win))}
                                td { class: "num", "{m.place:.2}" }
                                if let Some(x) = &m.sim { td { class: "num", "{x.place:.2}" } }
                                td { {place_hist(&m.place_hist, m.sim.as_ref().map(|x| x.place_hist.as_slice()))} }
                                if let Some(x) = &m.sim {
                                    td { class: "posline",
                                        {PHASE_ENDS.iter().map(|&p| format!("{:.1}", x.pos.get(p).copied().unwrap_or(0.0))).collect::<Vec<_>>().join(" → ")}
                                    }
                                    td { class: "num", b { "{pct(x.full_spurt)}" } }
                                    td { class: "num",
                                        if x.hp_out < 1.0 { div { class: "ok", "Survived" if x.hp_out > 0.0 { " {pct(1.0 - x.hp_out)}" } } }
                                        if x.hp_out > 0.0 { div { class: if x.hp_out < 1.0 { "small no" } else { "no" }, "Died" if x.hp_out < 1.0 { " {pct(x.hp_out)}" } } }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if sim.is_some() {
                div { class: "legend",
                    span { span { class: "sw act" } "Actual places" }
                    span { span { class: "sw sim" } "Sim places" }
                }
                div { class: "cols",
                    div {
                        h4 { "Position, sim average" }
                        PlaceChart {
                            series: ms
                                .iter()
                                .enumerate()
                                .map(|(j, m)| PlaceSeries {
                                    label: chara_name(m.member.chara),
                                    color: hue(j).to_string(),
                                    line: m.sim.as_ref().map(|x| x.pos.clone()).unwrap_or_default(),
                                    inner: None,
                                    outer: None,
                                    dashed: false,
                                    highlight: false,
                                })
                                .collect::<Vec<_>>(),
                            field_size: field,
                            markers: [100.0 / 6.0, 200.0 / 3.0, 500.0 / 6.0].into_iter().zip(&PHASES[1..]).map(|(x, l)| (x, l.to_string())).collect::<Vec<_>>(),
                        }
                    }
                    div {
                        h4 { "Wins vs the sim, race by race" }
                        {luck_chart(&rs)}
                        div { class: "legend",
                            span { span { class: "sw act" } "Your wins" }
                            span { span { class: "sw sim" } "Sim expects" }
                        }
                    }
                }
            }
            div { class: "card-h teams-h",
                h3 { "Races" }
                div { class: "seg small",
                    {seg(RaceFilter::All, format!("All {}", t.races))}
                    {seg(RaceFilter::Won, format!("Won {won}"))}
                    {seg(RaceFilter::Lost, format!("Lost {}", t.races - won))}
                    if let Some(x) = sim { {seg(RaceFilter::Upsets, format!("Upsets {}", x.upsets.0 + x.upsets.1))} }
                }
                span { class: "muted small", "click to open in Results" }
            }
            div { class: "scroll-x",
                table { class: "grid",
                    thead {
                        tr {
                            th { "Date" }
                            th { "Result" }
                            th { "Opponents" }
                            if sim.is_some() {
                                th { class: "num", "Sim team win" }
                                th {}
                            }
                        }
                    }
                    tbody {
                        for r in shown {
                            {
                                let path = r.path.to_path_buf();
                                let best = r.meta.team.iter().filter(|m| m.place > 0).min_by_key(|m| m.place);
                                rsx! {
                                    tr { key: "{r.path.display()}", title: "{r.path.display()}", onclick: move |_| open(s, path.clone()),
                                        td { class: "muted", "{short_date(r.date)}" }
                                        td {
                                            if let Some(m) = best {
                                                div { class: "whobox",
                                                    {uma_icon(m.card, m.chara, None, "sm")}
                                                    span {
                                                        if m.place == 1 { b { class: "ok", "Won" } } else { b { "{ordinal(m.place)}" } }
                                                        span { class: "muted small", " {chara_name(m.chara)}" }
                                                    }
                                                }
                                            }
                                        }
                                        td {
                                            span { class: "stack",
                                                for &(card, chara) in r.meta.rivals.iter() { {uma_icon(card, chara, None, "sm")} }
                                            }
                                        }
                                        if let Some(x) = r.sim {
                                            td { class: "num",
                                                span { class: "pbar", div { style: "width: {x.team_win * 100.0}%" } }
                                                b { "{x.team_win * 100.0:.0}%" }
                                            }
                                            td {
                                                if r.upset() == Some(true) {
                                                    if r.meta.won { span { class: "tag", "Upset win" } } else { span { class: "tag warn", "Upset loss" } }
                                                }
                                            }
                                        } else if sim.is_some() {
                                            td {}
                                            td {}
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

/// "1st", "2nd", "3rd", "4th".
fn ordinal(n: usize) -> String {
    let suf = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suf}")
}

/// Running wins race by race (oldest first) against the sim's running expected wins: the gap is the luck.
fn luck_chart(rs: &[CmRace]) -> Element {
    const W: f32 = 460.0;
    const H: f32 = 220.0;
    const L: f32 = 30.0;
    const R: f32 = 60.0;
    const T: f32 = 10.0;
    const B: f32 = 24.0;
    let (mut act, mut exp) = (vec![0.0f32], vec![0.0f32]);
    for r in rs.iter().rev() {
        act.push(act[act.len() - 1] + if r.meta.won { 1.0 } else { 0.0 });
        exp.push(exp[exp.len() - 1] + r.sim.map_or(0.0, |x| x.team_win));
    }
    let n = rs.len().max(1) as f32;
    let (a, e) = (act[act.len() - 1], exp[exp.len() - 1]);
    // Four even y steps to a round top.
    let step = (a.max(e) / 4.0).ceil().max(1.0);
    let x = |i: usize| L + i as f32 / n * (W - L - R);
    let y = |v: f32| H - B - v / (step * 4.0) * (H - T - B);
    let pts = |v: &[f32]| v.iter().enumerate().map(|(i, &w)| format!("{},{}", x(i), y(w))).collect::<Vec<_>>().join(" ");
    // End labels at least a line apart.
    let (ya, ye) = (y(a), y(e));
    let (ya, ye) = if (ya - ye).abs() >= 13.0 { (ya, ye) } else if a >= e { ((ya + ye) / 2.0 - 7.0, (ya + ye) / 2.0 + 7.0) } else { ((ya + ye) / 2.0 + 7.0, (ya + ye) / 2.0 - 7.0) };
    let mid = rs.len().div_ceil(2);
    rsx! {
        svg { class: "luckchart", view_box: "0 0 {W} {H}", width: "100%",
            for k in 0..=4 {
                line { class: "gridl", x1: "{L}", x2: "{W - R}", y1: "{y(k as f32 * step)}", y2: "{y(k as f32 * step)}" }
                text { class: "lab", x: "{L - 6.0}", y: "{y(k as f32 * step) + 4.0}", text_anchor: "end", "{k as f32 * step:.0}" }
            }
            for i in [1, mid, rs.len()] {
                text { class: "lab", x: "{x(i)}", y: "{H - 6.0}", text_anchor: "middle", "Race {i}" }
            }
            polyline { class: "sim", points: "{pts(&exp)}" }
            polyline { class: "act", points: "{pts(&act)}" }
            circle { class: "act", cx: "{x(rs.len())}", cy: "{y(a)}", r: "4" }
            text { class: "end", x: "{W - R + 8.0}", y: "{ya + 4.0}", "{a:.0} wins" }
            text { class: "end sim", x: "{W - R + 8.0}", y: "{ye + 4.0}", "{e:.1} sim" }
        }
    }
}
