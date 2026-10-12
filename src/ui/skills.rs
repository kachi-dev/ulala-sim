//! Skill filters (effect, rarity, tag chips + text search) and the skill picker modal built on them. Shared by the
//! uma editor (add skills) and Skill gains (candidate set).

use super::*;

#[derive(Clone, Default, PartialEq)]
pub struct SkillFilter {
    pub q: String,
    pub effects: Vec<&'static str>,
    /// "White" / "Gold" / "Unique" / "Inherited".
    pub kinds: Vec<&'static str>,
    pub tags: Vec<&'static str>,
}

const KINDS: [&str; 4] = ["White", "Gold", "Unique", "Inherited"];

/// Groups of `SKILL_TAGS` (a skill must match one selected tag in every group that has a selection).
const GROUPS: [(&str, &[&str]); 5] = [
    ("Style", &["Front", "Pace", "Late", "End"]),
    ("Distance", &["Sprint", "Mile", "Medium", "Long"]),
    ("Surface", &["Turf", "Dirt"]),
    ("Phase", &["Early race", "Mid race", "Late race", "Last spurt"]),
    ("Location", &["Corner", "Straight", "Uphill", "Downhill"]),
];

fn group_of(tag: &str) -> &'static str {
    GROUPS.iter().find(|g| g.1.contains(&tag)).map_or("Other", |g| g.0)
}

pub(super) fn kind(k: &SkillInfo) -> &'static str {
    if k.inherited {
        "Inherited"
    } else if k.rarity >= 3 {
        "Unique"
    } else if k.rarity == 2 {
        "Gold"
    } else {
        "White"
    }
}

/// Learnable by an uma wearing `card`: everything except other characters' (non-inherited) uniques.
pub(super) fn learnable(k: &SkillInfo, card: i32) -> bool {
    k.rarity < 3 || k.inherited || k.owner_card.is_none_or(|c| c == card)
}

pub(super) fn matches(f: &SkillFilter, k: &SkillInfo) -> bool {
    let q = f.q.trim().to_lowercase();
    if !q.is_empty() && !k.name.to_lowercase().contains(&q) && !k.description.to_lowercase().contains(&q) {
        return false;
    }
    if !f.kinds.is_empty() && !f.kinds.contains(&kind(k)) {
        return false;
    }
    if !f.effects.is_empty() && !f.effects.iter().any(|e| k.effects.contains(e)) {
        return false;
    }
    let mut groups: Vec<&str> = f.tags.iter().map(|t| group_of(t)).collect();
    groups.sort_unstable();
    groups.dedup();
    groups.iter().all(|g| f.tags.iter().filter(|t| group_of(t) == *g).any(|t| k.tags.contains(t)))
}

pub(super) fn toggle<T: PartialEq>(v: &mut Vec<T>, x: T) {
    match v.iter().position(|y| *y == x) {
        Some(i) => {
            v.remove(i);
        }
        None => v.push(x),
    }
}

/// Search box + chip rows (`SKILL_EFFECTS`, rarity, `SKILL_TAGS` by group).
#[component]
pub fn SkillFilters(mut filter: Signal<SkillFilter>) -> Element {
    let f = filter.cloned();
    let mut rows: Vec<(&str, Vec<&'static str>)> = GROUPS.iter().map(|g| (g.0, g.1.to_vec())).collect();
    rows.push(("Other", SKILL_TAGS.iter().copied().filter(|t| group_of(t) == "Other").collect()));
    rows.retain(|r| !r.1.is_empty());
    let any = f != SkillFilter::default();
    rsx! {
        div { class: "sfilters",
            div { class: "sf-top",
                input {
                    r#type: "search",
                    placeholder: "Search names and effects…",
                    value: "{f.q}",
                    oninput: move |e| filter.write().q = e.value(),
                }
                if any {
                    button { class: "btn ghost small", onclick: move |_| filter.set(SkillFilter::default()), "Clear filters" }
                }
            }
            div { class: "sf-row",
                span { class: "sf-label", "Rarity" }
                for k in KINDS {
                    button { class: if f.kinds.contains(&k) { "chip-btn on" } else { "chip-btn" }, onclick: move |_| toggle(&mut filter.write().kinds, k), "{k}" }
                }
            }
            div { class: "sf-row",
                span { class: "sf-label", "Effect" }
                for &e in SKILL_EFFECTS {
                    button { class: if f.effects.contains(&e) { "chip-btn on" } else { "chip-btn" }, onclick: move |_| toggle(&mut filter.write().effects, e), "{e}" }
                }
            }
            for (label, list) in rows {
                div { class: "sf-row",
                    span { class: "sf-label", "{label}" }
                    for t in list {
                        button { class: if f.tags.contains(&t) { "chip-btn on" } else { "chip-btn" }, onclick: move |_| toggle(&mut filter.write().tags, t), "{t}" }
                    }
                }
            }
        }
    }
}

/// Max skills listed at once in the picker (filters narrow it).
const PICK_CAP: usize = 240;

/// Skill picker modal for horse `i`: filters, result count, grid of skills; click adds (stays open), owned marked.
#[component]
pub fn SkillPicker(i: usize, mut open: Signal<bool>) -> Element {
    let s = use_context::<S>();
    let all = s.skills.cloned();
    let f = s.pick_filter.cloned();
    let Some(h) = s.input.read().horses.get(i).cloned() else { return rsx! {} };
    let hits: Vec<&SkillInfo> = all.iter().filter(|k| learnable(k, h.card_id) && matches(&f, k)).collect();
    let n = hits.len();
    rsx! {
        div { class: "scrim top", onclick: move |_| open.set(false) }
        div { class: "modal picker-modal",
            div { class: "drawer-h",
                {horse_icon(&h, None, "md")}
                div {
                    h3 { "Add skills" }
                    div { class: "muted small", "{chara_name(h.chara_id)} · {h.skills.len()} skills · click to add, owned ones are marked" }
                }
                button { class: "btn", onclick: move |_| open.set(false), "Done" }
            }
            div { class: "pick-filters", SkillFilters { filter: s.pick_filter } }
            div { class: "muted small pick-count",
                if n > PICK_CAP { "{thousands(n)} skills match, showing the first {PICK_CAP}: narrow the filters" } else { "{n} skills match" }
            }
            div { class: "pick-grid",
                for k in hits.into_iter().take(PICK_CAP) {
                    {
                        let owned = h.skills.iter().any(|x| x.skill_id == k.id);
                        let new = learned(k);
                        rsx! {
                            button {
                                key: "{k.id}",
                                class: if owned { "pick-card owned" } else { "pick-card" },
                                disabled: owned,
                                title: "{k.description}",
                                onclick: move |_| add_skill(s, i, new),
                                {skill_row(k, None, "")}
                                div { class: "pick-desc", "{k.description}" }
                                div { class: "pick-meta",
                                    for t in k.effects.iter().chain(k.tags.iter()).take(5) { span { "{t}" } }
                                    if owned { span { class: "owned-tag", "✓ owned" } }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
