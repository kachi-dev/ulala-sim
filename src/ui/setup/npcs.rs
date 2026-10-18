//! NPCs: the block above the field (fill toggle and what fills the empty gates) and the NPC setup drawer (stats,
//! skills on all NPCs, count and skills per style).

use super::*;
use skills::{PickFor, SkillPicker, npc_set};

fn umas(n: usize) -> String {
    if n == 1 { "1 uma".into() } else { format!("{n} umas") }
}

/// Skills of one NPC skillset as game-style rows (`none` when empty).
fn set_rows(ids: &[i32], none: &str) -> Element {
    rsx! {
        div { class: "npc-sk",
            if ids.is_empty() { span { class: "muted", "{none}" } }
            for &id in ids {
                {skill_row(&skill_info(id), None, "")}
            }
        }
    }
}

/// The Field card's block for the `n` empty slots: left empty, NPCs (per style with their skills, the skills on all of
/// them and their stats), or rivals from replays (`rival_block`).
pub(super) fn npc_block(mut s: S, n: usize) -> Element {
    let npcs = s.npcs.cloned();
    let c = lobby_counts(npcs.counts, n);
    let stats = npcs.stats.map(|v| v.to_string()).join(" / ");
    let mode = if !npcs.fill { 0 } else if npcs.rivals.on { 2 } else { 1 };
    rsx! {
        div { class: if mode > 0 { "npcs" } else { "npcs off" },
            div { class: "npcs-h",
                b { class: "small", "Empty slots" }
                div { class: "seg small",
                    for (m, label) in ["Off", "NPCs", "Replay rivals"].into_iter().enumerate() {
                        button { class: if mode == m { "on" }, onclick: move |_| {
                                let mut w = s.npcs.write();
                                w.fill = m > 0;
                                w.rivals.on = m == 2;
                            },
                            "{label}"
                        }
                    }
                }
                div { class: "grow" }
                span { class: "muted small", "{n} slot" if n != 1 { "s" } }
                if mode == 1 {
                    button { class: "btn alt", onclick: move |_| s.npc_open.set(true), "NPC setup" }
                }
            }
            if mode == 2 {
                {crate::ui::rivals::rival_block(s, n)}
            }
            if mode == 1 && n > 0 {
                div { class: "npcsum",
                    span { class: "hd", "Style" }
                    span { class: "hd", "NPCs" }
                    span { class: "hd", "Skills" }
                    for k in (0..LOBBY_STYLES.len()).filter(|&k| c[k] > 0) {
                        span { class: "style-t", "{LOBBY_STYLES[k]}" }
                        span { class: "cnt", "{c[k]}" {npc_faces(&npcs, n, k)} }
                        {set_rows(&npcs.by_style[k], "No style skills")}
                    }
                    span { class: "style-t all", "All NPCs" }
                    span { class: "cnt", "{n}" }
                    {set_rows(&npcs.skills, "No skills")}
                }
                div { class: "muted small", "Stats " b { class: "npc-stats", "{stats}" } }
            }
        }
    }
}

/// One NPC skillset, editable: rows with an X, "+ Add skill" opens the picker for it.
fn set_edit(mut s: S, k: Option<usize>, ids: &[i32], (mut to, mut picking): (Signal<Option<usize>>, Signal<bool>)) -> Element {
    rsx! {
        div { class: "skgrid",
            for (j, &id) in ids.iter().enumerate() {
                div { key: "{id}", class: "skedit",
                    {skill_row(&skill_info(id), None, "")}
                    button {
                        class: "btn icon",
                        title: "Remove skill",
                        onclick: move |_| {
                            npc_set(&mut s.npcs.write(), k).remove(j);
                        },
                        {icon("x")}
                    }
                }
            }
            button {
                class: "skadd",
                onclick: move |_| {
                    to.set(k);
                    picking.set(true);
                },
                "+ Add skill"
            }
        }
    }
}

#[component]
pub fn NpcDrawer() -> Element {
    let mut s = use_context::<S>();
    // The skill picker and its skillset (None = all NPCs); closing the drawer closes it.
    let (to, mut picking) = (use_signal(|| None), use_signal(|| false));
    use_effect(move || {
        if !*s.npc_open.read() {
            picking.set(false);
        }
    });
    let pick = (to, picking);
    if !*s.npc_open.read() {
        return rsx! {};
    }
    let npcs = s.npcs.cloned();
    let n_umas = s.input.read().horses.len();
    let size = *s.field_size.read();
    let total = size.saturating_sub(n_umas);
    let c = lobby_counts(npcs.counts, total);
    let preset = LOBBIES.iter().position(|p| lobby_counts(p.1, total) == c);
    // The field grows and shrinks with the NPCs, within FIELD_MIN (or the umas) and MAX_FIELD.
    let min = FIELD_MIN.max(n_umas);
    let mut step = move |k: usize, more: bool| {
        let mut c = c;
        if more { c[k] += 1 } else { c[k] -= 1 }
        s.npcs.write().counts = c;
        s.field_size.set(n_umas + c.iter().sum::<usize>());
    };
    rsx! {
        div { class: "scrim", onclick: move |_| s.npc_open.set(false) }
        aside { class: "drawer from-left",
            div { class: "drawer-h",
                div { h3 { "NPC setup" } }
                button { class: "btn icon", title: "Close", onclick: move |_| s.npc_open.set(false), {icon("x")} }
            }
            div { class: "drawer-b npc-drawer",
                div { class: "npc-blk",
                    h4 { "Stats" }
                    div { class: "stats",
                        for (j, v) in npcs.stats.into_iter().enumerate() {
                            label { key: "{j}",
                                span { class: "stat-l", img { src: "/a/stat/{j}.png", alt: "" } "{STATS[j]}" }
                                input {
                                    id: "npc-st-{j}",
                                    r#type: "number",
                                    min: 1,
                                    max: MAX_STAMINA,
                                    value: "{v}",
                                    oninput: move |e| {
                                        if let Ok(v) = e.value().parse::<i32>() {
                                            s.npcs.write().stats[j] = v.clamp(1, MAX_STAMINA);
                                        }
                                    },
                                    onchange: move |_| resync(&format!("npc-st-{j}"), s.npcs.peek().stats[j]),
                                }
                            }
                        }
                    }
                    div { class: "muted small", "Aptitude A for style, distance and surface · Mood Great" }
                }
                div { class: "npc-blk",
                    h4 { "Skills on all NPCs" }
                    {set_edit(s, None, &npcs.skills, pick)}
                }
                div { class: "npc-blk",
                    div { class: "npc-blk-h",
                        h4 { "By style" }
                        div { class: "grow" }
                        select {
                            onchange: move |e| {
                                if let Some(p) = e.value().parse::<usize>().ok().and_then(|i| LOBBIES.get(i)) {
                                    s.npcs.write().counts = p.1;
                                }
                            },
                            for (i, p) in LOBBIES.iter().enumerate() {
                                option { value: "{i}", selected: preset == Some(i), "{p.0}" }
                            }
                            option { value: "", disabled: true, selected: preset.is_none(), "Custom" }
                        }
                    }
                    for k in 0..LOBBY_STYLES.len() {
                        div { key: "{k}", class: if c[k] > 0 { "stylebox" } else { "stylebox zero" },
                            div { class: "stylebox-h",
                                span { class: "style-t", "{LOBBY_STYLES[k]}" }
                                {npc_faces(&npcs, total, k)}
                                div { class: "grow" }
                                span { class: "muted small", "NPCs" }
                                button { class: "btn icon", title: "One fewer", disabled: c[k] == 0 || size <= min, onclick: move |_| step(k, false), "−" }
                                b { class: "count", "{c[k]}" }
                                button { class: "btn icon", title: "One more", disabled: size >= MAX_FIELD, onclick: move |_| step(k, true), "+" }
                            }
                            if k == 0 {
                                div { class: "muted small", "Front runners with the Runaway skill, plus these:" }
                            }
                            {set_edit(s, Some(k), &npcs.by_style[k], pick)}
                        }
                    }
                    div { class: "muted small", "{umas(n_umas)} + {total} NPCs = field of {size} ({FIELD_MIN} to {MAX_FIELD})" }
                }
            }
            div { class: "drawer-f",
                button {
                    class: "btn ghost",
                    disabled: npcs == NpcSetup { fill: npcs.fill, rivals: npcs.rivals.clone(), ..Default::default() },
                    onclick: move |_| {
                        let (fill, rivals) = (s.npcs.peek().fill, s.npcs.peek().rivals.clone());
                        s.npcs.set(NpcSetup { fill, rivals, ..Default::default() });
                    },
                    "Reset NPCs"
                }
                button { class: "btn", onclick: move |_| s.npc_open.set(false), "Done" }
            }
        }
        if picking() {
            SkillPicker { to: PickFor::Npcs(to()), open: picking }
        }
    }
}
