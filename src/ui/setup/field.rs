//! The field card: uma cards or compact rows, with duplicate / remove.

use super::*;

/// Skill rows shown per card (the drawer lists all).
const SKILLS_SHOWN: usize = 8;

fn duplicate_horse(mut s: S, i: usize) {
    let h = s.input.read().horses[i].clone();
    // The selection stays on the same uma.
    if *s.sel.peek() > i {
        *s.sel.write() += 1;
    }
    edit_race(s, |inp| inp.horses.insert(i + 1, h));
    let mut t = s.trainers.write();
    let at = (i + 1).min(t.len());
    t.insert(at, String::new());
    // Gates after `i` shift: a focus there would land on another uma.
    s.focus.set(None);
}

/// Uma `i`'s team as a pill that opens a menu of the teams, each in the colour it would get (muted while the field
/// would have under two teams). A popover: it escapes the row's clipping and closes on a click outside or Esc.
fn team_pick(s: S, input: &RaceInput, player: Option<usize>, i: usize) -> Element {
    let cur = input.horses[i].team_id;
    let choice = |v: i32| {
        let mut t = input.clone();
        t.horses[i].team_id = v;
        let tag = team_tag(&t, player, i);
        let color = tag.as_ref().map_or("var(--muted)", |x| x.1);
        (v, tag.map_or_else(|| if v == 0 { "No team".to_string() } else { format!("Team {v}") }, |x| x.0), color)
    };
    let (_, label, color) = choice(cur);
    let opts: Vec<_> = (0..=3).map(choice).collect();
    rsx! {
        button {
            class: "team pick",
            style: "--team: {color}; anchor-name: --tm{i}",
            "popovertarget": "tm-{i}",
            onclick: |e| e.stop_propagation(),
            ondoubleclick: |e| e.stop_propagation(),
            "{label}"
            {icon("down")}
        }
        div {
            id: "tm-{i}",
            class: "team-menu",
            style: "position-anchor: --tm{i}",
            "popover": "auto",
            onclick: |e| e.stop_propagation(),
            ondoubleclick: |e| e.stop_propagation(),
            for (v, l, c) in opts {
                button {
                    class: if v == cur { "on" },
                    style: "--team: {c}",
                    "popovertarget": "tm-{i}",
                    "popovertargetaction": "hide",
                    onclick: move |_| edit(s, i, |h| h.team_id = v),
                    span { class: "team", "{l}" }
                }
            }
        }
    }
}

fn remove_horse(mut s: S, i: usize) {
    if *s.sel.peek() > i {
        *s.sel.write() -= 1;
    }
    edit_race(s, |inp| {
        inp.horses.remove(i);
    });
    let mut t = s.trainers.write();
    if i < t.len() {
        t.remove(i);
    }
    s.focus.set(None);
}

/// Edit / duplicate / remove buttons of one uma (cards and compact rows).
fn uma_actions(mut s: S, i: usize, n: usize) -> Element {
    rsx! {
        div { class: "uma-act", ondoubleclick: |e| e.stop_propagation(),
            button {
                class: "btn icon",
                title: "Edit (or double-click the uma)",
                onclick: move |e| {
                    e.stop_propagation();
                    s.editing.set(Some(i));
                },
                {icon("edit")}
            }
            button {
                class: "btn icon",
                title: "Duplicate",
                disabled: n >= *s.field_size.read(),
                onclick: move |e| {
                    e.stop_propagation();
                    duplicate_horse(s, i);
                },
                {icon("dup")}
            }
            button {
                class: "btn icon",
                title: "Remove",
                disabled: n <= 1,
                onclick: move |e| {
                    e.stop_propagation();
                    remove_horse(s, i);
                },
                {icon("x")}
            }
        }
    }
}

#[component]
pub(super) fn FieldCard() -> Element {
    let mut s = use_context::<S>();
    let Some(race) = s.race.cloned() else { return rsx! {} };
    let input = s.input.read();
    let base = s.baseline.cloned();
    let trainers = s.trainers.read();
    let cv = s.cview.cloned();
    let mask = edited_mask(base.as_deref(), &input);
    let n_edit = mask.iter().filter(|&&m| m).count();
    let n = input.horses.len();
    let comparable_now = base.as_deref().is_none_or(|b| comparable(b, &input));
    let can_reset = base.as_deref().is_some_and(|b| differs(b, &input));
    let compact = *s.compact.read();
    let sel = (*s.sel.read()).min(n.saturating_sub(1));
    let player = player_now(&race, &input);
    let cols = uma_colors(&input, player);
    let pair_base = base.as_deref().filter(|b| comparable(b, &input));
    let rows = input.horses.iter().enumerate().map(|(i, h)| {
        let b = pair_base.map(|b| &b.horses[i]);
        let (hs, bs) = (stats(h), stats(b.unwrap_or(h)));
        let apts = key_apts(h, cv.as_deref());
        let poor = poor_apts(h, cv.as_deref());
        let trainer = trainers.get(i).cloned().unwrap_or_default();
        let title = card_title(h.card_id);
        let sub = if trainer.is_empty() { title.clone() } else { format!("{title} · {trainer}") };
        let stat_cells = rsx! {
            for k in 0..5 {
                label { class: if hs[k] != bs[k] { "chg" }, title: if hs[k] != bs[k] { "was {bs[k]}" },
                    span { "{STATS_SHORT[k]}" }
                    // Edited in place: a click here doesn't select the uma, a double-click doesn't open the drawer.
                    input {
                        id: "fstat-{i}-{k}",
                        r#type: "number",
                        min: 1,
                        max: STAT_MAX,
                        value: "{hs[k]}",
                        onclick: |e| e.stop_propagation(),
                        ondoubleclick: |e| e.stop_propagation(),
                        oninput: move |e| {
                            if let Ok(v) = e.value().parse::<i32>() {
                                edit(s, i, |h| *stat_mut(h, k) = v.clamp(1, STAT_MAX));
                            }
                        },
                        onchange: move |_| resync(&format!("fstat-{i}-{k}"), stats(&s.input.peek().horses[i])[k]),
                    }
                }
            }
        };
        let tags = rsx! {
            span { title: "Running style and its aptitude", "{style_name(&race, h)} " b { "{grade(apts[2].1)}" } }
            span { title: "Surface aptitude", "{apts[0].0} " b { "{grade(apts[0].1)}" } }
            span { title: "Distance aptitude", "{apts[1].0} " b { "{grade(apts[1].1)}" } }
            span { "{mood_name(h.motivation)}" }
        };
        if compact {
            let unique = h.skills.iter().find(|k| skill_info(k.skill_id).is_unique()).copied();
            rsx! {
                div {
                    key: "{i}",
                    class: if sel == i { "umarow sel" } else { "umarow" },
                    style: "--ring: {cols[i]}",
                    onclick: move |_| select(s, i),
                    ondoubleclick: move |_| s.editing.set(Some(i)),
                    // On the top border, out of the layout: a tag in the name line would push the row taller.
                    if mask[i] { span { class: "tag edited-badge", "edited" } }
                    span { class: "gate", "{i + 1}" }
                    {horse_icon(h, Some(&cols[i]), "md")}
                    div { class: "uma-name",
                        div { class: "ell",
                            b { "{chara_name(h.chara_id)}" }
                        }
                        // Team tags on the second line: next to a long name they'd be cut by the ellipsis.
                        // Tags never truncate; the costume gives way first, then the trainer.
                        div { class: "muted small row2 fit", title: "{sub}",
                            {team_pick(s, &input, player, i)}
                            span { class: "costume", "{title}" }
                            if !trainer.is_empty() { span { class: "trainer", "{trainer}" } }
                        }
                    }
                    div { class: "uma-tags", {tags} }
                    div { class: "uma-stats", {stat_cells} }
                    div { class: "uma-sk",
                        // Icon + level only (a truncated name helps nobody); the full name is in the tooltip.
                        if let Some(k) = unique { {skill_row(&skill_info(k.skill_id), Some(k.level), "iconly")} }
                        span { class: "muted small", "{h.skills.len()} skills" }
                        if let Some(p) = &poor { span { class: "warn-dot", title: "Poor aptitude here: {p}", "!" } }
                    }
                    {uma_actions(s, i, n)}
                }
            }
        } else {
            rsx! {
                div {
                    key: "{i}",
                    class: if sel == i { "uma sel" } else { "uma" },
                    style: "--ring: {cols[i]}",
                    onclick: move |_| select(s, i),
                    ondoubleclick: move |_| s.editing.set(Some(i)),
                    // On the top border, out of the layout: a tag in the name line would push the row taller.
                    if mask[i] { span { class: "tag edited-badge", "edited" } }
                    div { class: "uma-h",
                        {horse_icon(h, Some(&cols[i]), "lg")}
                        div { class: "uma-name",
                            b { "{chara_name(h.chara_id)}" }
                            {team_pick(s, &input, player, i)}
                            div { class: "muted small", "{title}" }
                            div { class: "muted small", if trainer.is_empty() { "Gate {i + 1}" } else { "Gate {i + 1} · {trainer}" } }
                        }
                        div { class: "uma-tags", {tags} }
                        {uma_actions(s, i, n)}
                    }
                    if let Some(p) = &poor {
                        div { class: "warn", "Poor aptitude here: {p}. Speed and skill effects are heavily reduced." }
                    }
                    div { class: "uma-stats", {stat_cells} }
                    {skill_list(s, i, h, b.unwrap_or(h), false)}
                }
            }
        }
    });
    let is_save = race.path.is_some();
    let mut fs = s.field_size;
    let cap = *fs.read();
    rsx! {
        details { class: "card fold", open: true,
            summary { class: "card-h",
                h3 { "Field" }
                span { class: "muted fsize",
                    "{n} umas"
                    span { class: "small", "{FIELD_MIN}" }
                    div { class: "fs-slider", style: "--f: {(cap - FIELD_MIN) as f32 / (MAX_FIELD - FIELD_MIN) as f32}",
                        div { class: "fs-ticks",
                            for _ in FIELD_MIN..=MAX_FIELD {
                                i {}
                            }
                        }
                        input {
                            r#type: "range",
                            title: "Field size",
                            min: FIELD_MIN,
                            max: MAX_FIELD,
                            value: "{cap}",
                            oninput: move |e| {
                                if let Ok(v) = e.value().parse::<usize>() {
                                    fs.set(v.clamp(n.max(FIELD_MIN), MAX_FIELD));
                                }
                            },
                        }
                        b { "{cap}" }
                    }
                    span { class: "small", "{MAX_FIELD}" }
                }
                div { class: "seg small",
                    button { class: if !compact { "on" }, onclick: move |_| {
                            s.compact.set(false);
                            set_compact_pref(false);
                        }, "Cards" }
                    button { class: if compact { "on" }, onclick: move |_| {
                            s.compact.set(true);
                            set_compact_pref(true);
                        }, "Compact" }
                }
                div { class: "grow" }
                if !comparable_now {
                    span { class: "muted small", "Field size differs from the baseline, so runs are not compared." }
                } else if has_edits(base.as_deref(), &input) {
                    span { class: "muted small", if n_edit > 0 { "{n_edit} edited" } else { "conditions edited" } }
                }
                if can_reset {
                    button { class: "btn ghost", onclick: move |_| reset_to_baseline(s),
                        if is_save { "Reset to save" } else { "Reset to baseline" }
                    }
                }
                if !is_save {
                    if base.is_some() {
                        button { class: "btn ghost", onclick: move |_| s.baseline.set(None), "Unpin baseline" }
                    }
                    button {
                        class: "btn alt",
                        title: "Remember this setup; later runs show what your edits change compared with it.",
                        disabled: base.is_some() && !can_reset,
                        onclick: move |_| {
                            let cur = s.input.cloned();
                            s.baseline.set(Some(Arc::new(cur)));
                        },
                        "Pin as baseline"
                    }
                }
            }
            div { class: if compact { "umas compact" } else { "umas" },
                {rows}
                if n < cap {
                    button { class: if compact { "umarow add" } else { "uma add" }, onclick: move |_| s.picker.set(true),
                        span { class: "plus", "+" }
                        "Add uma"
                    }
                }
            }
        }
    }
}

/// Uma `i`'s skills (`h`) against its baseline `b`: rows that expand on click, an X to remove, removed ones to click
/// back, "+ Add skill". The card (`full` false) lists `SKILLS_SHOWN`; the drawer lists all with unique level inputs.
/// Clicks stop here, so it can sit in the clickable card.
pub(super) fn skill_list(mut s: S, i: usize, h: &HorseInput, b: &HorseInput, full: bool) -> Element {
    let shown = if full { usize::MAX } else { SKILLS_SHOWN };
    let removed = removed_skills(b, h);
    rsx! {
        div { class: "skgrid", ondoubleclick: |e| e.stop_propagation(),
            for (j, k) in game_order(&h.skills).into_iter().take(shown) {
                {
                    let base_k = b.skills.iter().find(|x| x.skill_id == k.skill_id).copied();
                    let lvl_input = full && skill_info(k.skill_id).is_unique();
                    rsx! {
                        div { key: "{k.skill_id}", class: "skedit",
                            SkillX {
                                id: k.skill_id,
                                level: if lvl_input { None } else { Some(k.level) },
                                extra: if base_k.is_none() { "added" } else { "" },
                            }
                            if lvl_input {
                                label { class: if base_k.is_some_and(|x| x.level != k.level) { "lvl chg" } else { "lvl" }, title: "Unique skill level",
                                    "Lv"
                                    input {
                                        id: "lvl-{j}",
                                        r#type: "number",
                                        min: 1,
                                        max: 6,
                                        value: "{k.level}",
                                        oninput: move |e| {
                                            if let Ok(v) = e.value().parse::<i32>() {
                                                edit(s, i, |h| h.skills[j].level = v.clamp(1, 6));
                                            }
                                        },
                                        onchange: move |_| resync(&format!("lvl-{j}"), s.input.peek().horses[i].skills[j].level),
                                    }
                                }
                            }
                            button {
                                class: "btn icon",
                                title: "Remove skill",
                                onclick: move |e| {
                                    e.stop_propagation();
                                    edit(s, i, |h| {
                                        h.skills.remove(j);
                                    });
                                },
                                {icon("x")}
                            }
                        }
                    }
                }
            }
            if h.skills.len() > shown {
                span { class: "more", "+{h.skills.len() - shown} more" }
            }
            for k in removed {
                div {
                    key: "{k.skill_id}",
                    class: "restore",
                    title: "Removed; click to restore",
                    onclick: move |e| {
                        e.stop_propagation();
                        add_skill(s, i, k);
                    },
                    {skill_row(&skill_info(k.skill_id), None, "removed")}
                }
            }
            button {
                class: "skadd",
                onclick: move |e| {
                    e.stop_propagation();
                    s.editing.set(Some(i));
                    s.skill_pick.set(true);
                },
                "+ Add skill"
            }
        }
    }
}

/// Skills in `base` that `h` no longer has (by id: a level change is not a removal).
fn removed_skills(base: &HorseInput, h: &HorseInput) -> Vec<SkillInput> {
    base.skills.iter().filter(|k| !h.skills.iter().any(|x| x.skill_id == k.skill_id)).copied().collect()
}
