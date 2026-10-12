//! Uma picker and the per-uma edit drawer.

use super::*;
use super::field::skill_list;
use std::rc::Rc;

// ---------- uma picker ----------

#[component]
pub fn Picker() -> Element {
    let mut s = use_context::<S>();
    let mut q = use_signal(String::new);
    // Veterans tab, its sort and required skills (kept between openings).
    let mut vets = use_signal(|| false);
    let sort = use_signal(|| 0);
    let need = use_signal(Vec::new);
    let apt_min = use_signal(|| [0; 10]);
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
        div { class: if vets() { "modal over vets-modal" } else { "modal over" },
            div { class: "drawer-h",
                h3 { if replace.is_some() { "Change uma" } else { "Add an uma" } }
                div { class: "seg small",
                    button { class: if !vets() { "on" }, onclick: move |_| vets.set(false), "Characters" }
                    button { class: if vets() { "on" }, onclick: move |_| vets.set(true), "Veterans" }
                }
                input {
                    r#type: "search",
                    placeholder: "Search by name or costume…",
                    autofocus: true,
                    value: "{q}",
                    oninput: move |e| q.set(e.value()),
                }
                button { class: "btn icon", title: "Close", onclick: move |_| s.picker.set(false), {icon("x")} }
            }
            if vets() {
                VeteranList { q: ql, sort, need, apt_min, replace }
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

const VET_SORTS: [&str; 8] = ["Newest", "Oldest", "Rating", "Speed", "Stamina", "Power", "Guts", "Wit"];
/// Aptitude rows of the veteran cards and filters; `vet_apts` order.
const APT_ROWS: [(&str, usize, &[&str]); 3] =
    [("Surface", 0, &["Turf", "Dirt"]), ("Distance", 2, &["Sprint", "Mile", "Medium", "Long"]), ("Style", 6, &["Front", "Pace", "Late", "End"])];

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
    replace: Option<usize>,
) -> Element {
    let mut s = use_context::<S>();
    let list = use_hook(|| Rc::new(load_veterans()));
    let mut skill_q = use_signal(String::new);
    let mut show_apts = use_signal(|| false);
    let Ok(all) = &*list else {
        return rsx! {
            p { class: "muted vets-empty", "No veteran list in {saved_races_dir().display()}." }
            SetupHint { heading: false }
        };
    };
    // Datalist of every skill some veteran has (inherited uniques marked: they share the unique's name).
    let mut ids: Vec<i32> = all.iter().flat_map(|v| v.horse.skills.iter().map(|k| k.skill_id)).collect();
    ids.sort_unstable();
    ids.dedup();
    let mut opts: Vec<(i32, String)> =
        ids.into_iter().map(skill_info).map(|k| (k.id, if k.inherited { format!("{} (inherited)", k.name) } else { k.name })).collect();
    opts.sort_by(|a, b| a.1.cmp(&b.1));
    let opts = Rc::new(opts);
    let picks = opts.clone();
    let need_v = need.cloned();
    let am = apt_min.cloned();
    let n_apt = am.iter().filter(|&&g| g > 0).count();
    let has = |v: &Veteran, id: i32| v.horse.skills.iter().any(|k| k.skill_id == id);
    let mut hits: Vec<&Veteran> = all
        .iter()
        .filter(|v| chara_name(v.horse.chara_id).to_lowercase().contains(&q) || card_title(v.horse.card_id).to_lowercase().contains(&q))
        .filter(|v| need_v.iter().all(|&id| has(v, id)))
        .filter(|v| vet_apts(&v.horse).iter().zip(am).all(|(&a, m)| a >= m))
        .collect();
    match sort() {
        0 => hits.sort_by(|a, b| b.created.cmp(&a.created)),
        1 => hits.sort_by(|a, b| a.created.cmp(&b.created)),
        2 => hits.sort_by_key(|v| std::cmp::Reverse(v.rating)),
        k => hits.sort_by_key(|v| std::cmp::Reverse(stats(&v.horse)[k - 3])),
    }
    rsx! {
        div { class: "vet-bar",
            input {
                r#type: "search",
                list: "vet-skills",
                placeholder: "Filter by skill…",
                value: "{skill_q}",
                oninput: move |e| {
                    let v = e.value();
                    match picks.iter().find(|o| o.1 == v) {
                        Some(&(id, _)) => {
                            if !need.read().contains(&id) {
                                need.write().push(id);
                            }
                            skill_q.set(String::new());
                        }
                        None => skill_q.set(v),
                    }
                },
            }
            datalist { id: "vet-skills",
                for (id, name) in opts.iter() {
                    option { key: "{id}", value: "{name}" }
                }
            }
            button { class: if show_apts() || n_apt > 0 { "chip-btn on" } else { "chip-btn" }, onclick: move |_| show_apts.toggle(),
                if n_apt > 0 { "Aptitudes ({n_apt})" } else { "Aptitudes" }
            }
            select { onchange: move |e| sort.set(e.value().parse().unwrap_or(0)),
                for (i, l) in VET_SORTS.iter().enumerate() {
                    option { value: "{i}", selected: i == sort(), "{l}" }
                }
            }
            span { class: "muted small", "{hits.len()} of {all.len()}" }
        }
        if show_apts() {
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
                if n_apt > 0 {
                    button { class: "btn ghost small", onclick: move |_| apt_min.set([0; 10]), "Clear" }
                }
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
                    let st = stats(&h);
                    let apts = vet_apts(&h);
                    let sk: Vec<(SkillInfo, i32)> = game_order(&h.skills).into_iter().map(|(_, k)| (skill_info(k.skill_id), k.level)).collect();
                    let title = card_title(h.card_id);
                    let day = v.created.get(..10).unwrap_or(&v.created).to_string();
                    let wins = h.single_mode_win_count;
                    let pref = (h.running_style - 1) as usize;
                    rsx! {
                        div {
                            key: "{v.id}",
                            class: "vcard",
                            onclick: move |_| {
                                let Some(race) = s.race.cloned() else { return };
                                let mut h = h.clone();
                                h.skills.retain(|k| race.data.skills.contains_key(&k.skill_id));
                                if let Some(i) = replace {
                                    edit_race(s, |inp| inp.horses[i] = h);
                                    if let Some(t) = s.trainers.write().get_mut(i) {
                                        t.clear();
                                    }
                                } else {
                                    edit_race(s, |inp| inp.horses.push(h));
                                    s.trainers.write().push(String::new());
                                }
                                s.picker.set(false);
                            },
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
                                    }
                                }
                            }
                            div { class: "vc-skills",
                                for (k, lv) in sk.iter() {
                                    {skill_row(k, Some(*lv), if need_v.contains(&k.id) { "need" } else { "" })}
                                }
                            }
                        }
                    }
                }
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
    let b = s.baseline.cloned().filter(|b| comparable(b, &input)).map(|b| b.horses[i].clone()).unwrap_or_else(|| h.clone());
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
        aside { class: "drawer",
            div { class: "drawer-h",
                {horse_icon(&h, Some(&ring), "lg")}
                div {
                    h3 { "{chara_name(h.chara_id)}" }
                    div { class: "muted small", "{card_title(h.card_id)}" }
                    div { class: "muted small", if trainer.is_empty() { "Gate {i + 1}" } else { "Gate {i + 1} · {trainer}" } }
                }
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
                            for (v, name) in STYLES.iter().enumerate().skip(1) {
                                option { value: "{v}", selected: h.running_style == v as i32, "{name}" }
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
                h4 { "Skills ({h.skills.len()})" }
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
            skills::SkillPicker { i, open: picking }
        }
    }
}
