//! Replay rivals: the Field card's block (where the pool comes from, its size, this seed's draw) and the Rival pool
//! dialog (replays by filter, or picked one by one).

use super::*;
use std::collections::BTreeMap;

/// "Kitasan Black, Silence Suzuka, T.M. Opera O".
fn names(t: &PoolTeam) -> String {
    t.umas.iter().map(|u| chara_name(u.chara)).collect::<Vec<_>>().join(", ")
}

/// The pool's source: the folders (or the races picked); the CM, dates, and whole teams or single umas.
fn source(r: &RivalSetup) -> (String, String) {
    let head = match (r.pick, r.cats.is_empty()) {
        (true, _) => format!("{} race{} picked", r.picked.len(), if r.picked.len() == 1 { "" } else { "s" }),
        (false, true) => "No folders".to_string(),
        (false, false) => r.cats.join(", "),
    };
    let mut sub = vec![];
    if !r.pick {
        if r.course != 0 {
            sub.push(course_label(r.course));
        }
        if r.conds.iter().any(|&c| c != 0) {
            sub.push(setup::cond_names(r.conds));
        }
        match (r.from.as_str(), r.to.as_str()) {
            ("", "") => {}
            (f, "") => sub.push(format!("from {f}")),
            ("", t) => sub.push(format!("to {t}")),
            (f, t) => sub.push(format!("{f} to {t}")),
        }
    }
    sub.push(if r.singles { "single umas" } else { "whole teams" }.to_string());
    (head, sub.join(" · "))
}

/// The Field card's rivals, `slots` empty gates: the pool's source and size, and the teams the setup's seed draws.
pub(super) fn rival_block(mut s: S, slots: usize) -> Element {
    let setup = s.npcs.read().rivals.clone();
    let (mut pool, replays) = rival_pool(&s.saves.read(), &s.meta.read(), &setup);
    pool.retain(|t| !setup.dropped.contains(&t.key));
    if pool.is_empty() {
        return rsx! {
            div { class: "rv-empty",
                b { "No replays picked" }
                button { class: "btn alt", onclick: move |_| s.rivals_open.set(true), "Pick replays" }
            }
        };
    }
    let (head, sub) = source(&setup);
    let units = pool_units(&pool, setup.singles);
    let sizes: Vec<usize> = units.iter().map(|u| u.1.len()).collect();
    let seed = s.input.read().random_seed;
    // Units drawn, in gate order, each with its umas drawn (an uma drawn alone is a row of its own).
    let mut picks: Vec<(usize, Vec<usize>)> = vec![];
    let (drawn, whole) = draw(&sizes, seed, slots);
    let n_whole = drawn[..whole].iter().map(|x| x.0).collect::<HashSet<_>>().len();
    for (j, (u, m)) in drawn.into_iter().enumerate() {
        match picks.iter_mut().find(|p| p.0 == u).filter(|_| j < whole) {
            Some(p) => p.1.push(units[u].1[m]),
            None => picks.push((u, vec![units[u].1[m]])),
        }
    }
    let (n_pool, what) = if setup.singles { (pool_umas(&pool, true), "umas") } else { (pool.len(), "teams") };
    let short = rivals_short(s, slots);
    let runs = *s.n.read();
    rsx! {
        div { class: "rv-src",
            div {
                b { "{head}" }
                div { class: "muted small", "{sub}" }
            }
            button { class: "btn alt", onclick: move |_| s.rivals_open.set(true), "Edit pool" }
        }
        div { class: "rv-kpis",
            div { b { "{thousands(replays)}" } span { "replays" } }
            div { b { "{thousands(n_pool)}" } span { "{what} in the pool" } }
            div { b { "{n_whole}" } span { "{what} per race" } }
        }
        if let Some(e) = short {
            div { class: "warn", "{e}" }
        } else if slots > 0 {
            div { class: "rv-seed",
                b { "Seed {seed} draws" }
                span { class: "muted small", "Run ×{runs}: a new draw per seed" }
            }
            for (k, (u, ms)) in picks.into_iter().enumerate() {
                {
                    let t = &pool[units[u].0];
                    let ring = TEAM_HUES[(k + 1).min(3)][0];
                    rsx! {
                        div { key: "{t.key}", class: "rv-team",
                            span { class: "mobs",
                                for m in ms { {uma_icon(t.umas[m].card, t.umas[m].chara, Some(ring), "sm")} }
                            }
                            div { class: "grow",
                                b { "{t.trainer}" }
                                div { class: "muted small", "{names(t)}" }
                            }
                            span { class: "muted small", if t.last.is_empty() { "{t.category}" } else { "{t.category} · {t.last}" } }
                        }
                    }
                }
            }
        }
    }
}

/// A replay's teams as the pool sees them (`rival_pool`): field indices by `team_id`, team 0 each alone, NPCs left out;
/// and whether
/// it is one of the user's (a save owner's trainer).
fn teams_of(m: &SaveMeta, mine: &HashSet<&str>) -> Vec<(Vec<usize>, bool)> {
    let mut groups: Vec<Vec<usize>> = vec![];
    for (i, u) in m.field.iter().enumerate().filter(|(_, u)| u.card != 0) {
        match groups.iter_mut().find(|g| u.team != 0 && m.field[g[0]].team == u.team) {
            Some(g) => g.push(i),
            None => groups.push(vec![i]),
        }
    }
    groups.into_iter().map(|g| { let me = mine.contains(m.field[g[0]].trainer.as_str()); (g, me) }).collect()
}

/// Max rows a list renders (search narrows it).
const ROWS: usize = 300;

/// The Rival pool dialog: replays by folder, CM and dates, or picked one by one; the teams they give (each can be left
/// out); whole teams or single umas; the user's own umas skipped or not.
#[component]
pub(super) fn RivalsModal() -> Element {
    let mut s = use_context::<S>();
    let mut q = use_signal(String::new);
    // Pick races: the list's folder and CM filters.
    let (mut cat, mut course) = (use_signal(String::new), use_signal(|| 0));
    if !*s.rivals_open.read() {
        return rsx! {};
    }
    let setup = s.npcs.read().rivals.clone();
    let saves = s.saves.read();
    let meta = s.meta.read();
    let (pool, replays) = rival_pool(&saves, &meta, &setup);
    let live = pool.iter().filter(|t| !setup.dropped.contains(&t.key)).count();
    let mine: HashSet<&str> = meta.values().map(|m| m.player_trainer.as_str()).filter(|t| !t.is_empty()).collect();
    let mut cats: BTreeMap<&str, usize> = BTreeMap::new();
    for e in saves.iter() {
        *cats.entry(e.category.as_str()).or_default() += 1;
    }
    // By filter's course pickers (as on the Course card), and the courses the saves ran on (Pick races).
    let courses = s.courses.cloned();
    let cur = courses.iter().find(|c| c.id == setup.course).cloned();
    // The race being set up: its course and conditions ("This race").
    let this = { let i = s.input.read(); (i.course_set_id, setup::conds_of(&i)) };
    let mut ran_on: Vec<(i32, &str)> = meta.values().map(|m| (m.course_id, m.course.as_str())).filter(|c| !c.1.is_empty()).collect();
    ran_on.sort_by(|a, b| a.1.cmp(b.1));
    ran_on.dedup();
    let ql = q.read().to_lowercase();
    let close = move |_| s.rivals_open.set(false);
    let tab = |pick: bool| {
        move |_| {
            s.npcs.write().rivals.pick = pick;
            q.set(String::new());
        }
    };
    let body = if setup.pick {
        let (cat_v, course_v) = (cat.cloned(), *course.read());
        let hits: Vec<&SaveEntry> = saves
            .iter()
            .filter(|e| meta.contains_key(&e.path))
            .filter(|e| cat_v.is_empty() || e.category == cat_v)
            .filter(|e| course_v == 0 || meta.get(&e.path).is_some_and(|m| m.course_id == course_v))
            .filter(|e| ql.is_empty() || e.winner.to_lowercase().contains(&ql) || meta.get(&e.path).is_some_and(|m| m.course.to_lowercase().contains(&ql)))
            .collect();
        let shown: Vec<PathBuf> = hits.iter().take(ROWS).map(|e| e.path.clone()).collect();
        let n_hits = hits.len();
        rsx! {
            div { class: "races-filters",
                input { r#type: "search", placeholder: "Search by winner or course…", value: "{q}", oninput: move |e| q.set(e.value()) }
                select { onchange: move |e| cat.set(e.value()),
                    option { value: "", "All categories" }
                    for (c, _) in cats.iter() { option { value: "{c}", selected: *c == cat_v, "{c}" } }
                }
                select { onchange: move |e| course.set(e.value().parse().unwrap_or(0)),
                    option { value: "0", "All courses" }
                    for (id, label) in ran_on.iter() { option { value: "{id}", selected: *id == course_v, "{label}" } }
                }
                div { class: "grow" }
                span { class: "muted small", if n_hits > ROWS { "Newest {ROWS} of {thousands(n_hits)} replays" } else { "{thousands(n_hits)} replays" } }
                button {
                    class: "btn ghost small",
                    onclick: move |_| {
                        let mut w = s.npcs.write();
                        for p in &shown {
                            if !w.rivals.picked.contains(p) {
                                w.rivals.picked.push(p.clone());
                            }
                        }
                    },
                    "Pick all shown"
                }
                button { class: "btn ghost small", disabled: setup.picked.is_empty(), onclick: move |_| s.npcs.write().rivals.picked.clear(), "Clear" }
            }
            div { class: "races-list",
                table { class: "grid saves",
                    thead {
                        tr {
                            th {}
                            th { "Winner" }
                            th { class: "num", "Time" }
                            th { "Course" }
                            th { "Teams" }
                            th { "Date" }
                        }
                    }
                    tbody {
                        for e in hits.into_iter().take(ROWS) {
                            {
                                let m = &meta[&e.path];
                                let on = setup.picked.contains(&e.path);
                                let p = e.path.clone();
                                let toggle = move |_| {
                                    let mut w = s.npcs.write();
                                    match w.rivals.picked.iter().position(|x| *x == p) {
                                        Some(k) => { w.rivals.picked.remove(k); }
                                        None => w.rivals.picked.push(p.clone()),
                                    }
                                };
                                let time = e.time.trim_end_matches('s').parse::<f32>().map(ft).unwrap_or_default();
                                rsx! {
                                    tr { key: "{e.path.display()}", class: if on { "sel" }, onclick: toggle,
                                        td { input { r#type: "checkbox", class: "rv-check", checked: on, aria_label: "Pick this race" } }
                                        td { class: "who",
                                            div { class: "whobox",
                                                if m.winner.1 != 0 { {uma_icon(m.winner.0, m.winner.1, None, "sm")} }
                                                b { "{e.winner}" }
                                            }
                                        }
                                        td { class: "num", "{time}" }
                                        td { class: "muted", "{m.course}" }
                                        td {
                                            div { class: "rv-groups",
                                                for (g, me) in teams_of(m, &mine) {
                                                    span { class: if me && !setup.keep_mine { "rv-grp me" } else { "rv-grp" }, title: if me { "Your umas" } else { "{m.field[g[0]].trainer}" },
                                                        for &i in &g { {uma_icon(m.field[i].card, m.field[i].chara, None, "xs")} }
                                                    }
                                                }
                                            }
                                        }
                                        td { class: "muted", "{e.date}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    } else {
        let teams: Vec<&PoolTeam> =
            pool.iter().filter(|t| ql.is_empty() || t.trainer.to_lowercase().contains(&ql) || names(t).to_lowercase().contains(&ql)).collect();
        let n_teams = teams.len();
        rsx! {
            div { class: "rv-body",
                div { class: "rv-side",
                    div { class: "rv-grp-f",
                        span { class: "hd", "Replays" }
                        for (c, n) in cats.iter() {
                            {
                                let c = c.to_string();
                                let on = setup.cats.contains(&c);
                                rsx! {
                                    label { key: "{c}", class: "check",
                                        input {
                                            r#type: "checkbox",
                                            checked: on,
                                            onchange: {
                                                let c = c.clone();
                                                move |_| {
                                                    let mut w = s.npcs.write();
                                                    match w.rivals.cats.iter().position(|x| *x == c) {
                                                        Some(k) => { w.rivals.cats.remove(k); }
                                                        None => w.rivals.cats.push(c.clone()),
                                                    }
                                                }
                                            },
                                        }
                                        "{c}"
                                        span { class: "grow" }
                                        span { class: "muted small", "{thousands(*n)}" }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "rv-grp-f",
                        span { class: "hd", "Course" }
                        select {
                            id: "rv-load",
                            onchange: move |e| {
                                let v = e.value();
                                let mut w = s.npcs.write();
                                let r = &mut w.rivals;
                                if v == "any" {
                                    (r.course, r.conds) = (0, [0; 4]);
                                } else if v == "this" {
                                    (r.course, r.conds) = (this.0, this.1);
                                } else if let Some(p) = v.parse::<usize>().ok().and_then(|i| cm_presets().get(i)) {
                                    (r.course, r.conds) = (p.course_set_id, [p.ground, p.weather, p.season, p.time]);
                                }
                                drop(w);
                                resync("rv-load", "");
                            },
                            option { value: "", selected: true, "Load from…" }
                            option { value: "this", "This race" }
                            for (i, p) in cm_presets().iter().enumerate().filter(|(_, p)| courses.iter().any(|c| c.id == p.course_set_id)) {
                                option { value: "{i}", "CM {p.id} - {p.name}" if p.start.is_none() { " (upcoming)" } }
                            }
                            option { value: "any", "Any course and conditions" }
                        }
                        label { class: "check",
                            input {
                                r#type: "checkbox",
                                checked: cur.is_none(),
                                onchange: move |e| s.npcs.write().rivals.course = if e.checked() { 0 } else { this.0 },
                            }
                            "Any course"
                        }
                        if let Some(c) = cur.clone() {
                            div { class: "rv-course",
                                {setup::course_pickers(courses.clone(), &c, "rv-course", move |id| { let mut s = s; s.npcs.write().rivals.course = id; })}
                            }
                        }
                    }
                    div { class: "rv-grp-f",
                        span { class: "hd", "Conditions" }
                        div { class: "condrow",
                            for k in 0..4 {
                                {setup::cond_seg(k, setup.conds[k], true, move |v| { let mut s = s; s.npcs.write().rivals.conds[k] = v; })}
                            }
                        }
                    }
                    div { class: "rv-grp-f",
                        span { class: "hd", "Dates" }
                        div { class: "rv-dates",
                            span { class: "muted small", "From" }
                            input { r#type: "date", aria_label: "From", value: "{setup.from}", onchange: move |e| s.npcs.write().rivals.from = e.value() }
                            span { class: "muted small", "To" }
                            input { r#type: "date", aria_label: "To", value: "{setup.to}", onchange: move |e| s.npcs.write().rivals.to = e.value() }
                        }
                    }
                }
                div { class: "rv-main",
                    div { class: "rv-main-h",
                        b { "{pool.len()} teams" }
                        span { class: "muted", "from {thousands(replays)} replays" }
                        div { class: "grow" }
                        input { r#type: "search", placeholder: "Search by trainer or uma…", value: "{q}", oninput: move |e| q.set(e.value()) }
                    }
                    div { class: "races-list",
                        table { class: "grid saves",
                            thead {
                                tr {
                                    th {}
                                    th { "Team" }
                                    th { "Styles" }
                                    th { class: "num", "Seen in" }
                                    th { class: "num", "Won in replays" }
                                    th { "Last raced" }
                                }
                            }
                            tbody {
                                for t in teams.into_iter().take(ROWS) {
                                    {
                                        let on = !setup.dropped.contains(&t.key);
                                        let key = t.key.clone();
                                        let toggle = move |_| {
                                            let mut w = s.npcs.write();
                                            match w.rivals.dropped.iter().position(|x| *x == key) {
                                                Some(k) => { w.rivals.dropped.remove(k); }
                                                None => w.rivals.dropped.push(key.clone()),
                                            }
                                        };
                                        rsx! {
                                            tr { key: "{t.key}", class: if on { "" } else { "rv-off" }, onclick: toggle,
                                                td { input { r#type: "checkbox", class: "rv-check", checked: on, aria_label: "Use this team" } }
                                                td { class: "who",
                                                    div { class: "whobox",
                                                        span { class: "mobs", for u in &t.umas { {uma_icon(u.card, u.chara, None, "sm")} } }
                                                        div {
                                                            b { "{t.trainer}" }
                                                            div { class: "muted small", "{names(t)}" }
                                                        }
                                                    }
                                                }
                                                td { span { class: "mobs", for u in &t.umas { {style_badge(u.style)} } } }
                                                td { class: "num", "{t.seen}" }
                                                td { class: "num", "{t.won} of {t.seen}" }
                                                td { class: "muted", "{t.last}" div { class: "small", "{t.category}" } }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if n_teams > ROWS { p { class: "muted small", "Showing {ROWS} of {thousands(n_teams)}" } }
                        if pool.is_empty() { p { class: "muted races-empty", "No teams: tick a folder, or widen the course, conditions and dates." } }
                    }
                }
            }
        }
    };
    let dropped = pool.len() - live;
    rsx! {
        div { class: "scrim", onclick: close }
        div { class: "modal races rivals",
            div { class: "drawer-h",
                h3 { "Rival pool" }
                div { class: "seg",
                    button { class: if !setup.pick { "on" }, onclick: tab(false), "By filter" }
                    button { class: if setup.pick { "on" }, onclick: tab(true), "Pick races" }
                }
                div { class: "grow" }
                button { class: "btn icon", title: "Close (Esc)", onclick: close, {icon("x")} }
            }
            {body}
            div { class: "rv-foot",
                div { class: "seg small",
                    button { class: if !setup.singles { "on" }, onclick: move |_| s.npcs.write().rivals.singles = false, "Whole teams" }
                    button { class: if setup.singles { "on" }, onclick: move |_| s.npcs.write().rivals.singles = true, "Single umas" }
                }
                label { class: "check",
                    input { r#type: "checkbox", checked: !setup.keep_mine, onchange: move |e| s.npcs.write().rivals.keep_mine = !e.checked() }
                    "Skip my umas"
                }
                span { class: "muted small",
                    if setup.pick { "{setup.picked.len()} races · " }
                    "{live} teams in the pool"
                    if dropped > 0 { " · {dropped} left out" }
                }
                div { class: "grow" }
                button {
                    class: "btn ghost",
                    onclick: move |_| {
                        let mut w = s.npcs.write();
                        w.rivals = RivalSetup { on: w.rivals.on, pick: w.rivals.pick, ..Default::default() };
                    },
                    "Clear pool"
                }
                button { class: "btn", onclick: close, "Done" }
            }
        }
    }
}
