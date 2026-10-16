//! Uma picker and the per-uma edit drawer.

use super::*;
use super::field::skill_list;
use std::rc::Rc;

// ---------- uma picker ----------

#[component]
pub fn Picker() -> Element {
    let mut s = use_context::<S>();
    let mut q = use_signal(String::new);
    // Tab (Characters, Veterans, My umas); the veterans' sort and required skills (kept between openings).
    let mut tab = use_signal(|| 0);
    let sort = use_signal(|| 0);
    let need = use_signal(Vec::new);
    let apt_min = use_signal(|| [0; 10]);
    let mins = use_signal(|| [0; 3]);
    if !*s.picker.read() {
        return rsx! {};
    }
    // Opened from the edit drawer: the pick replaces that uma instead of adding one.
    let replace = s.editing.cloned();
    let ql = q.read().to_lowercase();
    // Costumes grouped by character (cards() is sorted by name, then card id).
    let mut groups: Vec<(&str, i32, Vec<&CardInfo>)> = vec![];
    for c in cards().iter().filter(|c| c.name.to_lowercase().contains(&ql) || c.title.to_lowercase().contains(&ql)) {
        match groups.last_mut() {
            Some(g) if g.1 == c.chara_id => g.2.push(c),
            _ => groups.push((&c.name, c.chara_id, vec![c])),
        }
    }
    rsx! {
        div { class: "scrim top", onclick: move |_| s.picker.set(false) }
        div { class: if tab() > 0 { "modal over vets-modal" } else { "modal over" },
            div { class: "drawer-h",
                h3 { if replace.is_some() { "Change uma" } else { "Add an uma" } }
                div { class: "seg small",
                    for (k, label) in ["Characters", "Veterans", "My umas"].into_iter().enumerate() {
                        button { class: if tab() == k { "on" }, onclick: move |_| tab.set(k), "{label}" }
                    }
                }
                input {
                    r#type: "search",
                    placeholder: if tab() == 2 { "Search by name, uma or CM…" } else { "Search by name or costume…" },
                    autofocus: true,
                    value: "{q}",
                    oninput: move |e| q.set(e.value()),
                }
                button { class: "btn icon", title: "Close", onclick: move |_| s.picker.set(false), {icon("x")} }
            }
            if tab() == 1 {
                VeteranList { q: ql, sort, need, apt_min, mins, replace }
            } else if tab() == 2 {
                MyUmaList { q: ql, replace }
            } else {
            div { class: "cgroups",
                if groups.is_empty() { p { class: "muted", "No uma matches." } }
                for (name, chara, list) in groups {
                    div { key: "{chara}", class: "cgroup",
                        div { class: "cg-name", "{name}" }
                        div { class: "cg-cards",
                            for c in list {
                                button {
                                    key: "{c.card_id}",
                                    class: "chara",
                                    title: "{c.name} {c.title}",
                                    onclick: move |_| {
                                        if let Some(i) = replace {
                                            edit(s, i, |h| set_card(h, c.card_id));
                                            s.picker.set(false);
                                            return;
                                        }
                                        let Some(race) = s.race.cloned() else { return };
                                        let h = new_horse(&race, c.card_id);
                                        let i = s.input.read().horses.len();
                                        edit_race(s, |inp| inp.horses.push(h));
                                        s.trainers.write().push(String::new());
                                        s.picker.set(false);
                                        select(s, i);
                                        s.editing.set(Some(i));
                                    },
                                    {uma_icon(c.card_id, c.chara_id, None, "xl")}
                                    span { "{c.title}" }
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

const VET_SORTS: [&str; 9] = ["Newest", "Oldest", "Rating", "SP", "Speed", "Stamina", "Power", "Guts", "Wit"];
/// Aptitude rows of the veteran cards and filters; `vet_apts` order.
const APT_ROWS: [(&str, usize, &[&str]); 3] =
    [("Surface", 0, &SURFACES), ("Distance", 2, &DISTANCES), ("Style", 6, &STYLES)];

/// Turf, Dirt, Sprint … Long, Front … End.
fn vet_apts(h: &HorseInput) -> [i32; 10] {
    let mut a = [0; 10];
    a[..2].copy_from_slice(&h.proper_ground);
    a[2..6].copy_from_slice(&h.proper_distance);
    a[6..].copy_from_slice(&h.proper_running_style);
    a
}

/// Letter rank badge (`rank_label` scale).
fn rank_img(rank: i32, class: &str) -> Element {
    let l = rank_label(rank);
    rsx! { img { class: "{class}", src: "/a/rank/{(rank - 1).clamp(0, 97)}.png", alt: "{l}" } }
}

/// horseACT's veteran list as cards: name / costume search (`q`), required skills, aptitude minimums, sort. Click
/// adds the uma as trained. Mounted only while shown, so every opening reads the list fresh.
#[component]
fn VeteranList(
    q: String,
    mut sort: Signal<usize>,
    mut need: Signal<Vec<i32>>,
    mut apt_min: Signal<[i32; 10]>,
    /// Minimum rating, rank and speed; 0 = any.
    mut mins: Signal<[i32; 3]>,
    replace: Option<usize>,
) -> Element {
    let s = use_context::<S>();
    let list = use_hook(|| Rc::new(load_veterans()));
    let mut skill_q = use_signal(String::new);
    let mut skill_open = use_signal(|| false);
    let Ok(all) = &*list else {
        return rsx! {
            p { class: "muted vets-empty", "No veteran list in {saved_races_dir().display()}." }
            SetupHint { heading: false }
        };
    };
    let need_v = need.cloned();
    // Skill search: skills some veteran has, not yet required, by name.
    let sq = skill_q.read().to_lowercase();
    let mut ids: Vec<i32> = all.iter().flat_map(|v| v.horse.skills.iter().map(|k| k.skill_id)).filter(|id| !need_v.contains(id)).collect();
    ids.sort_unstable();
    ids.dedup();
    let mut opts: Vec<SkillInfo> = if sq.is_empty() { vec![] } else { ids.into_iter().map(skill_info).filter(|k| k.name.to_lowercase().contains(&sq)).collect() };
    opts.sort_by(|a, b| a.name.cmp(&b.name).then(a.inherited.cmp(&b.inherited)));
    opts.truncate(30);
    let first = opts.first().map(|k| k.id);
    let am = apt_min.cloned();
    let mn = mins.cloned();
    let n_apt = am.iter().chain(&mn).filter(|&&g| g > 0).count();
    let mut ranks: Vec<i32> = all.iter().map(|v| v.rank).collect();
    ranks.sort_unstable_by(|a, b| b.cmp(a));
    ranks.dedup();
    let has = |v: &Veteran, id: i32| v.horse.skills.iter().any(|k| k.skill_id == id);
    let mut hits: Vec<&Veteran> = all
        .iter()
        .filter(|v| chara_name(v.horse.chara_id).to_lowercase().contains(&q) || card_title(v.horse.card_id).to_lowercase().contains(&q))
        .filter(|v| need_v.iter().all(|&id| has(v, id)))
        .filter(|v| vet_apts(&v.horse).iter().zip(am).all(|(&a, m)| a >= m))
        .filter(|v| v.rating >= mn[0] && v.rank >= mn[1] && v.horse.speed >= mn[2])
        .collect();
    match sort() {
        0 => hits.sort_by(|a, b| b.created.cmp(&a.created)),
        1 => hits.sort_by(|a, b| a.created.cmp(&b.created)),
        2 => hits.sort_by_key(|v| std::cmp::Reverse(v.rating)),
        3 => hits.sort_by_key(|v| std::cmp::Reverse(skills_sp(&v.horse.skills))),
        k => hits.sort_by_key(|v| std::cmp::Reverse(stats(&v.horse)[k - 4])),
    }
    rsx! {
        div { class: "vet-bar",
            div { class: "vet-sk",
                input {
                    r#type: "search",
                    placeholder: "Filter by skill…",
                    value: "{skill_q}",
                    oninput: move |e| {
                        skill_q.set(e.value());
                        skill_open.set(true);
                    },
                    onfocus: move |_| skill_open.set(true),
                    onblur: move |_| skill_open.set(false),
                    onkeydown: move |e| match e.key() {
                        Key::Enter => {
                            if let Some(id) = first {
                                need.write().push(id);
                                skill_q.set(String::new());
                            }
                        }
                        Key::Escape => skill_q.set(String::new()),
                        _ => {}
                    },
                }
                if skill_open() && !sq.is_empty() {
                    div { class: "vet-sk-menu",
                        if opts.is_empty() { p { class: "muted small", "No veteran has a skill by that name." } }
                        for k in opts {
                            {
                                let id = k.id;
                                rsx! {
                                    button {
                                        key: "{id}",
                                        class: "pick-card",
                                        // mousedown, not click: keeps focus in the search so the menu stays for the next pick.
                                        onmousedown: move |e| {
                                            e.prevent_default();
                                            need.write().push(id);
                                            skill_q.set(String::new());
                                        },
                                        {skill_row(&k, None, "")}
                                        div { class: "pick-desc", "{k.description}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            select { onchange: move |e| sort.set(e.value().parse().unwrap_or(0)),
                for (i, l) in VET_SORTS.iter().enumerate() {
                    option { value: "{i}", selected: i == sort(), "{l}" }
                }
            }
            span { class: "muted small", "{hits.len()} of {all.len()}" }
        }
        div { class: "vet-aptf",
            for (label, off, names) in APT_ROWS {
                div { class: "sf-row",
                    span { class: "sf-label", "{label}" }
                    for (j, n) in names.iter().enumerate() {
                        label { class: "vaf",
                            "{n}"
                            select { onchange: move |e| apt_min.write()[off + j] = e.value().parse().unwrap_or(0),
                                option { value: "0", selected: am[off + j] == 0, "Any" }
                                for g in (1..=8).rev() {
                                    option { value: "{g}", selected: am[off + j] == g, "≥{grade(g)}" }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "sf-row",
                span { class: "sf-label", "Minimum" }
                label { class: "vaf",
                    "Rating"
                    input { r#type: "number", min: "0", step: "1000", placeholder: "Any", value: if mn[0] > 0 { mn[0].to_string() } else { String::new() },
                        oninput: move |e| mins.write()[0] = e.value().parse().unwrap_or(0) }
                }
                label { class: "vaf",
                    "Rank"
                    select { onchange: move |e| mins.write()[1] = e.value().parse().unwrap_or(0),
                        option { value: "0", selected: mn[1] == 0, "Any" }
                        for r in ranks {
                            option { value: "{r}", selected: mn[1] == r, "≥{rank_label(r)}" }
                        }
                    }
                }
                label { class: "vaf",
                    "Speed"
                    input { r#type: "number", min: "0", step: "100", placeholder: "Any", value: if mn[2] > 0 { mn[2].to_string() } else { String::new() },
                        oninput: move |e| mins.write()[2] = e.value().parse().unwrap_or(0) }
                }
            }
            if n_apt > 0 {
                button { class: "btn ghost small", onclick: move |_| { apt_min.set([0; 10]); mins.set([0; 3]); }, "Clear" }
            }
        }
        if !need_v.is_empty() {
            div { class: "vet-need",
                for id in need_v.iter().copied() {
                    div { key: "{id}",
                        {skill_row(&skill_info(id), None, "")}
                        button { class: "btn icon", title: "Remove", onclick: move |_| need.write().retain(|&x| x != id), {icon("x")} }
                    }
                }
            }
        }
        div { class: "vets",
            if hits.is_empty() { p { class: "muted", "No veteran matches." } }
            for v in hits {
                {
                    let h = v.horse.clone();
                    let title = card_title(h.card_id);
                    let day = v.created.get(..10).unwrap_or(&v.created).to_string();
                    let wins = h.single_mode_win_count;
                    rsx! {
                        div {
                            key: "{v.id}",
                            class: "vcard",
                            onclick: move |_| place(s, h.clone(), String::new(), replace),
                            div { class: "vc-h",
                                {uma_icon(v.horse.card_id, v.horse.chara_id, None, "md")}
                                div { class: "vc-name",
                                    b { class: "ell", "{chara_name(v.horse.chara_id)}" }
                                    div { class: "muted small ell", "{title}" }
                                    div { class: "muted small",
                                        "{thousands(v.rating.max(0) as usize)} · {wins} "
                                        if wins == 1 { "win" } else { "wins" }
                                        " · {day}"
                                    }
                                }
                                {rank_img(v.rank, "vc-rank")}
                            }
                            {vc_body(&v.horse, &need_v)}
                        }
                    }
                }
            }
        }
    }
}

/// Puts uma `h` (as trained or saved) at gate `replace`, or adds her: skills the race's data lacks are dropped.
fn place(mut s: S, mut h: HorseInput, trainer: String, replace: Option<usize>) {
    let Some(race) = s.race.cloned() else { return };
    h.skills.retain(|k| race.data.skills.contains_key(&k.skill_id));
    if let Some(i) = replace {
        edit_race(s, |inp| inp.horses[i] = h);
        if let Some(t) = s.trainers.write().get_mut(i) {
            *t = trainer;
        }
    } else {
        edit_race(s, |inp| inp.horses.push(h));
        s.trainers.write().push(trainer);
    }
    s.picker.set(false);
}

/// Umas saved by name as cards: search by name, uma or CM, a CM filter, the race's CM's first. Click adds (or
/// replaces) the uma as saved, trainer name included. Mounted only while shown, so every opening reads them fresh.
#[component]
fn MyUmaList(q: String, replace: Option<usize>) -> Element {
    let s = use_context::<S>();
    let mut list = use_signal(list_umas);
    // The uma whose Delete was clicked, asking to confirm; the CM filter (0 = all).
    let mut del = use_signal(|| None::<PathBuf>);
    let mut cm_f = use_signal(|| 0);
    let all = list.read();
    if all.is_empty() {
        return rsx! {
            p { class: "muted vets-empty", "No saved umas yet. Save uma, in an uma's edit drawer, keeps her stats, aptitudes and skills here, tagged with the CM when the course is on one." }
        };
    }
    let cur_cm = cm_preset_of(&s.input.read()).map(|k| cm_presets()[k].id);
    let mut cms: Vec<(i32, String)> = all.iter().filter_map(|u| u.cm.clone()).collect();
    cms.sort_by(|a, b| b.0.cmp(&a.0));
    cms.dedup_by_key(|c| c.0);
    let f = cm_f();
    let text = |u: &SavedUma| {
        let cm = u.cm.as_ref().map_or(String::new(), |(id, n)| format!("CM {id} {n}"));
        format!("{} {} {} {cm} {}", u.name, chara_name(u.horse.chara_id), card_title(u.horse.card_id), u.trainer).to_lowercase()
    };
    let mut hits: Vec<&SavedUma> = all.iter().filter(|u| f == 0 || u.cm.as_ref().is_some_and(|c| c.0 == f)).filter(|u| text(u).contains(&q)).collect();
    // The race's CM first (newest first within).
    hits.sort_by_key(|u| cur_cm.is_none() || u.cm.as_ref().map(|c| c.0) != cur_cm);
    rsx! {
        if !cms.is_empty() {
            div { class: "vet-bar",
                label { class: "vaf",
                    "CM"
                    select { onchange: move |e| cm_f.set(e.value().parse().unwrap_or(0)),
                        option { value: "0", "All" }
                        for (id, name) in cms {
                            option { value: "{id}", selected: f == id, "CM {id} - {name}" }
                        }
                    }
                }
            }
        }
        div { class: "vets",
            if hits.is_empty() { p { class: "muted", "No saved uma matches." } }
            for u in hits {
                {
                    let (h, trainer, p) = (u.horse.clone(), u.trainer.clone(), u.path.clone());
                    let asking = del.read().as_ref() == Some(&u.path);
                    let sub = [u.trainer.clone(), format!("saved {}", u.date)].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · ");
                    rsx! {
                        div {
                            key: "{u.path.display()}",
                            class: "vcard",
                            onclick: move |_| place(s, h.clone(), trainer.clone(), replace),
                            div { class: "vc-h",
                                {uma_icon(u.horse.card_id, u.horse.chara_id, None, "md")}
                                div { class: "vc-name",
                                    b { class: "ell", "{u.name}" }
                                    div { class: "muted small ell", "{chara_name(u.horse.chara_id)} {card_title(u.horse.card_id)}" }
                                    div { class: "muted small ell", "{sub}" }
                                }
                                div { class: "vc-side", onclick: move |e| e.stop_propagation(),
                                    if let Some((id, name)) = &u.cm { span { class: "chip cm", title: "Made for CM {id} - {name}", "CM {id}" } }
                                    if asking {
                                        button {
                                            class: "btn small danger",
                                            onclick: {
                                                let p = p.clone();
                                                move |_| {
                                                    let _ = std::fs::remove_file(&p);
                                                    del.set(None);
                                                    list.set(list_umas());
                                                }
                                            },
                                            "Delete"
                                        }
                                        button { class: "btn small ghost", onclick: move |_| del.set(None), "Keep" }
                                    } else {
                                        button { class: "btn icon", title: "Delete this saved uma", onclick: move |_| del.set(Some(p.clone())), {icon("trash")} }
                                    }
                                }
                            }
                            {vc_body(&u.horse, &[])}
                        }
                    }
                }
            }
        }
    }
}

/// A veteran or saved uma card's body: stats with their ranks, aptitudes (with SP), skills in the game's order
/// (`need` ones marked).
fn vc_body(h: &HorseInput, need: &[i32]) -> Element {
    let st = stats(h);
    let apts = vet_apts(h);
    let sp = skills_sp(&h.skills);
    let sk: Vec<(SkillInfo, i32)> = game_order(&h.skills).into_iter().map(|(_, k)| (skill_info(k.skill_id), k.level)).collect();
    let pref = (h.running_style - 1) as usize;
    rsx! {
                            div { class: "vc-stats",
                                for k in 0..5 {
                                    div {
                                        img { class: "vc-si", src: "/a/stat/{k}.png", alt: "{STATS_SHORT[k]}" }
                                        {rank_img(stat_rank(st[k]), "vc-sr")}
                                        b { "{st[k]}" }
                                    }
                                }
                            }
                            div { class: "vc-apts",
                                for (_, off, names) in APT_ROWS {
                                    div { class: "vc-apt-row",
                                        for (j, n) in names.iter().enumerate() {
                                            span {
                                                class: if off == 6 && j == pref { "pref" },
                                                "{n}"
                                                if (1..=8).contains(&apts[off + j]) {
                                                    img { src: "/a/apt/{apts[off + j]}.png", alt: "{grade(apts[off + j])}" }
                                                } else {
                                                    b { "{grade(apts[off + j])}" }
                                                }
                                            }
                                        }
                                        if off == 0 { span { "SP" b { "{thousands(sp.max(0) as usize)}" } } }
                                    }
                                }
                            }
                            div { class: "vc-skills",
                                for (k, lv) in sk.iter() {
                                    {skill_row(k, Some(*lv), if need.contains(&k.id) { "need" } else { "" })}
                                }
                            }
    }
}

// ---------- edit drawer ----------

#[component]
pub fn Drawer() -> Element {
    let mut s = use_context::<S>();
    let mut picking = s.skill_pick;
    // Closing the drawer closes the picker too.
    use_effect(move || {
        if s.editing.read().is_none() {
            picking.set(false);
        }
    });
    let Some(i) = s.editing.cloned() else { return rsx! {} };
    let Some(h) = s.input.read().horses.get(i).cloned() else { return rsx! {} };
    let input = s.input.read();
    let b = base_horse(s.baseline.cloned().as_deref(), &input, i).cloned().unwrap_or_else(|| h.clone());
    drop(input);
    let changed = !horse_same(&h, &b);
    let (hs, bs) = (stats(&h), stats(&b));
    let costumes: Vec<&CardInfo> = cards().iter().filter(|c| c.chara_id == h.chara_id).collect();
    let apt = |label: &'static str, cur: i32, base: i32, set: fn(&mut HorseInput, i32)| {
        rsx! {
            label { class: if cur != base { "apt chg" } else { "apt" },
                "{label}"
                select { onchange: move |e| edit(s, i, |h| set(h, e.value().parse().unwrap_or(cur))),
                    for g in (1..=8).rev() {
                        option { value: "{g}", selected: g == cur, "{GRADES[g as usize]}" }
                    }
                }
            }
        }
    };
    let trainer = s.trainers.read().get(i).cloned().unwrap_or_default();
    let cv = s.cview.cloned();
    let poor = poor_apts(&h, cv.as_deref());
    let ring = {
        let input = s.input.read();
        let player = s.race.read().as_ref().and_then(|r| player_now(r, &input));
        uma_colors(&input, player)[i].clone()
    };
    rsx! {
        div { class: "scrim", onclick: move |_| s.editing.set(None) }
        aside { class: "drawer from-left",
            div { class: "drawer-h",
                {horse_icon(&h, Some(&ring), "lg")}
                div {
                    h3 { "{chara_name(h.chara_id)}" }
                    div { class: "muted small", "{card_title(h.card_id)}" }
                    div { class: "muted small", if trainer.is_empty() { "Gate {i + 1}" } else { "Gate {i + 1} · {trainer}" } }
                }
                button { class: "btn ghost", title: "Keep this uma (stats, aptitudes, skills) to add again from My umas", onclick: move |_| s.uma_save.set(Some(i)), "Save uma" }
                button { class: "btn ghost", title: "Pick a different uma; stats, skills and team stay", onclick: move |_| s.picker.set(true), "Change uma" }
                button { class: "btn icon", title: "Close", onclick: move |_| s.editing.set(None), {icon("x")} }
            }
            div { class: "drawer-b",
                if costumes.len() > 1 {
                    h4 { "Costume" }
                    div { class: "costumes",
                        for c in costumes {
                            button {
                                key: "{c.card_id}",
                                class: if c.card_id == h.card_id { "on" },
                                title: "{c.title}",
                                onclick: move |_| edit(s, i, |h| set_card(h, c.card_id)),
                                {uma_icon(c.card_id, c.chara_id, None, "md")}
                            }
                        }
                    }
                }
                if let Some(p) = &poor {
                    div { class: "warn", "Poor aptitude on this course: {p}." }
                }
                h4 { "Stats" }
                div { class: "stats",
                    for k in 0..5 {
                        label { class: if hs[k] != bs[k] { "chg" },
                            span { "{STATS[k]}" }
                            // The stat's rank inside the box, following the value.
                            div { class: "stat-in",
                                {rank_img(stat_rank(hs[k]), "stat-rank")}
                                input {
                                    id: "stat-{k}",
                                    r#type: "number",
                                    min: 1,
                                    max: STAT_MAX,
                                    value: "{hs[k]}",
                                    oninput: move |e| {
                                        if let Ok(v) = e.value().parse::<i32>() {
                                            edit(s, i, |h| *stat_mut(h, k) = v.clamp(1, STAT_MAX));
                                        }
                                    },
                                    onchange: move |_| resync(&format!("stat-{k}"), stats(&s.input.peek().horses[i])[k]),
                                }
                            }
                            // The line is always there, so the inputs stay aligned when one changes.
                            small { if hs[k] != bs[k] { "was {bs[k]}" } else { "\u{a0}" } }
                        }
                    }
                }
                div { class: "two",
                    div { class: if h.team_id != b.team_id { "chg" },
                        span { "Team "
                            Help { "Teams replicate CM teams - umas in the same team won't affect one another with debuffs." }
                        }
                        select {
                            onchange: move |e| edit(s, i, |h| h.team_id = e.value().parse().unwrap_or(0)),
                            for (v, name) in [(0, "None"), (1, "Team 1"), (2, "Team 2"), (3, "Team 3")] {
                                option { value: "{v}", selected: h.team_id == v, "{name}" }
                            }
                        }
                    }
                    label { class: if h.running_style != b.running_style { "chg" },
                        span { "Running style" }
                        select {
                            // Runaway (Oonige) isn't picked here: the sim derives it from a Front runner's skills.
                            onchange: move |e| edit(s, i, |h| h.running_style = e.value().parse().unwrap_or(1)),
                            for (v, name) in (1..).zip(STYLES) {
                                option { value: "{v}", selected: h.running_style == v, "{name}" }
                            }
                        }
                    }
                    label { class: if h.motivation != b.motivation { "chg" },
                        span { "Mood" }
                        select { onchange: move |e| edit(s, i, |h| h.motivation = e.value().parse().unwrap_or(3)),
                            for m in (1..=5).rev() {
                                option { value: "{m}", selected: m == h.motivation, "{MOODS[m as usize]}" }
                            }
                        }
                    }
                }
                h4 { "Aptitudes" }
                div { class: "apt-grid",
                    {apt("Turf", h.proper_ground[0], b.proper_ground[0], |h, v| h.proper_ground[0] = v)}
                    {apt("Dirt", h.proper_ground[1], b.proper_ground[1], |h, v| h.proper_ground[1] = v)}
                    span {}
                    span {}
                    {apt("Short", h.proper_distance[0], b.proper_distance[0], |h, v| h.proper_distance[0] = v)}
                    {apt("Mile", h.proper_distance[1], b.proper_distance[1], |h, v| h.proper_distance[1] = v)}
                    {apt("Medium", h.proper_distance[2], b.proper_distance[2], |h, v| h.proper_distance[2] = v)}
                    {apt("Long", h.proper_distance[3], b.proper_distance[3], |h, v| h.proper_distance[3] = v)}
                    {apt("Front", h.proper_running_style[0], b.proper_running_style[0], |h, v| h.proper_running_style[0] = v)}
                    {apt("Pace", h.proper_running_style[1], b.proper_running_style[1], |h, v| h.proper_running_style[1] = v)}
                    {apt("Late", h.proper_running_style[2], b.proper_running_style[2], |h, v| h.proper_running_style[2] = v)}
                    {apt("End", h.proper_running_style[3], b.proper_running_style[3], |h, v| h.proper_running_style[3] = v)}
                }
                h4 { "Skills ({h.skills.len()})" span { class: "muted sp", title: "Skill points the skills cost (lower tiers included)", " · {thousands(skills_sp(&h.skills).max(0) as usize)} SP" } }
                {skill_list(s, i, &h, &b, true)}
            }
            div { class: "drawer-f",
                button {
                    class: "btn ghost",
                    disabled: !changed,
                    onclick: move |_| edit(s, i, |h| *h = b.clone()),
                    "Reset this uma"
                }
                button { class: "btn", onclick: move |_| s.editing.set(None), "Done" }
            }
        }
        // Outside the drawer: its slide-in transform would make a fixed modal relative to it.
        if picking() {
            skills::SkillPicker { to: skills::PickFor::Uma(i), open: picking }
        }
    }
}
