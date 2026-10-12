//! The field card: uma cards or compact rows, with duplicate / remove.

use super::*;

/// Skill rows shown per card (the drawer lists all).
const SKILLS_SHOWN: usize = 8;

fn duplicate_horse(mut s: S, i: usize) {
    let h = s.input.read().horses[i].clone();
    edit_race(s, |inp| inp.horses.insert(i + 1, h));
    let mut t = s.trainers.write();
    let at = (i + 1).min(t.len());
    t.insert(at, String::new());
    // Gates after `i` shift: a focus there would land on another uma.
    s.focus.set(None);
}

fn remove_horse(mut s: S, i: usize) {
    edit_race(s, |inp| {
        inp.horses.remove(i);
    });
    let mut t = s.trainers.write();
    if i < t.len() {
        t.remove(i);
    }
    s.focus.set(None);
}

/// Duplicate / remove buttons of one uma (cards and compact rows).
fn uma_actions(s: S, i: usize, n: usize) -> Element {
    rsx! {
        div { class: "uma-act",
            button {
                class: "btn icon",
                title: "Duplicate",
                disabled: n >= MAX_FIELD,
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
    let player = player_now(&race, &input);
    let cols = uma_colors(&input, player);
    let pair_base = base.as_deref().filter(|b| comparable(b, &input));
    let rows = input.horses.iter().enumerate().map(|(i, h)| {
        let b = pair_base.map(|b| &b.horses[i]);
        let (hs, bs) = (stats(h), stats(b.unwrap_or(h)));
        let removed = b.map(|b| removed_skills(b, h)).unwrap_or_default();
        let added = |k: &SkillInput| b.is_some_and(|b| !b.skills.iter().any(|x| x.skill_id == k.skill_id));
        let apts = key_apts(h, cv.as_deref());
        let poor = poor_apts(h, cv.as_deref());
        let trainer = trainers.get(i).cloned().unwrap_or_default();
        let title = card_title(h.card_id);
        let sub = if trainer.is_empty() { title.clone() } else { format!("{title} · {trainer}") };
        let stat_cells = rsx! {
            for k in 0..5 {
                div { class: if hs[k] != bs[k] { "chg" }, title: if hs[k] != bs[k] { "was {bs[k]}" },
                    span { "{STATS_SHORT[k]}" }
                    b { "{hs[k]}" }
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
            let unique = h.skills.iter().find(|k| skill_info(k.skill_id).rarity >= 3).copied();
            rsx! {
                div {
                    key: "{i}",
                    class: if mask[i] { "umarow edited" } else { "umarow" },
                    style: "--ring: {cols[i]}",
                    onclick: move |_| s.editing.set(Some(i)),
                    span { class: "gate", "{i + 1}" }
                    {horse_icon(h, Some(&cols[i]), "md")}
                    div { class: "uma-name",
                        div { class: "ell",
                            b { "{chara_name(h.chara_id)}" }
                            if mask[i] { span { class: "tag", "edited" } }
                        }
                        // Team tags on the second line: next to a long name they'd be cut by the ellipsis.
                        // Tags never truncate; the costume gives way first, then the trainer.
                        div { class: "muted small row2 fit", title: "{sub}",
                            {team_tags(&input, player, i)}
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
                    class: if mask[i] { "uma edited" } else { "uma" },
                    style: "--ring: {cols[i]}",
                    onclick: move |_| s.editing.set(Some(i)),
                    div { class: "uma-h",
                        {horse_icon(h, Some(&cols[i]), "lg")}
                        div { class: "uma-name",
                            b { "{chara_name(h.chara_id)}" }
                            {team_tags(&input, player, i)}
                            if mask[i] { span { class: "tag", "edited" } }
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
                    div { class: "uma-skills",
                        for (_, k) in game_order(&h.skills).into_iter().take(SKILLS_SHOWN) {
                            SkillX { id: k.skill_id, level: Some(k.level), extra: if added(&k) { "added" } else { "" } }
                        }
                        if h.skills.len() > SKILLS_SHOWN {
                            span { class: "more", "+{h.skills.len() - SKILLS_SHOWN} more" }
                        }
                        for k in removed {
                            {skill_row(&skill_info(k.skill_id), None, "removed")}
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
        }
    });
    let is_save = race.path.is_some();
    rsx! {
        details { class: "card fold", open: true,
            summary { class: "card-h",
                h3 { "Field" }
                span { class: "muted", "{n} of {MAX_FIELD} · click an uma to edit" }
                div { class: "seg small",
                    button { class: if !compact { "on" }, onclick: move |_| s.compact.set(false), "Cards" }
                    button { class: if compact { "on" }, onclick: move |_| s.compact.set(true), "Compact" }
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
                if n < MAX_FIELD {
                    button { class: if compact { "umarow add" } else { "uma add" }, onclick: move |_| s.picker.set(true),
                        span { class: "plus", "+" }
                        "Add uma"
                    }
                }
            }
        }
    }
}

/// Skills in `base` that `h` no longer has (by id: a level change is not a removal).
pub(super) fn removed_skills(base: &HorseInput, h: &HorseInput) -> Vec<SkillInput> {
    base.skills.iter().filter(|k| !h.skills.iter().any(|x| x.skill_id == k.skill_id)).copied().collect()
}
