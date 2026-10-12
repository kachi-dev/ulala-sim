//! Skill Gains tab (umalator-style skill chart): one uma among NPCs or in her field, each candidate skill on vs muted
//! on the same seeds, measured in horse lengths.

use super::*;
use crate::charts::{CourseHist, GainChart, Histogram};

const CHART_ROWS: usize = 20;

fn rarity_color(r: i32) -> &'static str {
    match r {
        ..=1 => "#63bfe9",
        2 => "#e8ae1c",
        _ => "#f0699b",
    }
}

/// Detail charts of one measured gain (Uma ranking's too): where it activates with the gain at each activation, and
/// gain per seed.
pub(super) fn gain_charts(samples: &[f32], by_distance: &[(f32, f32)], color: &str, cv: &CourseView, what: &str) -> Element {
    rsx! {
        h4 { "Where {what} activates (bars) and lengths gained when it does (dots)" }
        CourseHist {
            distances: by_distance.iter().map(|p| p.0).collect::<Vec<f32>>(),
            course_distance: cv.distance,
            phases: cv.phases.to_vec(),
            corners: cv.corner_spans(),
            bin: 10.0,
            color: color.to_string(),
            gains: by_distance.to_vec(),
        }
        h4 { "Gain per seed (lengths)" }
        Histogram { values: samples.to_vec(), color: color.to_string(), unit: "L".to_string() }
    }
}

#[component]
pub fn GainsTab() -> Element {
    let s = use_context::<S>();
    let mut g_owned = s.g_owned;
    let running = use_busy(s);
    let input = s.input.read();
    if input.horses.is_empty() {
        return rsx! {
            section { class: "card", p { class: "muted", "Add an uma in Setup first." } }
        };
    }
    let field_n = input.horses.len();
    let hi = (*s.g_horse.read()).min(field_n - 1);
    let horse = input.horses[hi].clone();
    drop(input);
    let cv = s.cview.cloned();
    let poor = poor_apts(&horse, cv.as_deref());
    let (owned, n) = (*g_owned.read(), *s.g_n.read());
    let f = s.g_filter.cloned();
    let cands: Vec<i32> = if owned {
        horse.skills.iter().map(|k| k.skill_id).collect()
    } else {
        s.skills.read().iter().filter(|k| skills::learnable(k, horse.card_id) && skills::matches(&f, k)).map(|k| k.id).collect()
    };
    let (mode, npc, size) = mode_seg(s, s.g_npc, field_n);
    let n_c = cands.len();
    let races = n_c * n * 2;
    let work = races * size;
    let eta = estimate(work as f32 / s.g_rate.read().max(1.0));
    let start = move |_| {
        let (Some(race), Some(cv)) = (s.race.cloned(), s.cview.cloned()) else { return };
        if cands.is_empty() {
            return;
        }
        let (input, i) = npc_field(&race, &s.input.peek(), hi, npc, *s.npc_lobby.peek());
        let (cands, rev, field) = (cands.clone(), *s.rev.peek(), input.horses.clone());
        let (card, chara, asked) = (horse.card_id, horse.chara_id, cands.len());
        let label = format!("Measuring {} skills × {n} seeds", thousands(asked));
        let run = move |w: &Work| skill_gains(&race, &input, i, &cands, n, w);
        measure(s, label, asked * n, work, run, move |mut s, (items, spurt)| {
            s.g_sel.set(None);
            let low_spurt = low_spurts(&field, &spurt);
            s.g_res.set(Some(GainsRes { horse: hi, card, chara, items: Arc::new(items), cv, n, asked, npc, low_spurt, rev }));
            reveal("gains");
        });
    };
    rsx! {
        section { class: "card",
            div { class: "card-h",
                h3 { "Skill gains" }
            }
            div { class: "gsetup",
                {who_pick(s, s.g_horse)}
                {mode}
                label { class: "field",
                    span { "Samples" }
                    {seeds_input("gn", s.g_n, 10, 5000)}
                }
            }
            div { class: "card-h cands",
                span { class: "sf-label", "Skills" }
                div { class: "seg",
                    button { class: if !owned { "on" }, onclick: move |_| g_owned.set(false), "New Skills" }
                    button { class: if owned { "on" }, onclick: move |_| g_owned.set(true), "Current Skills" }
                }
                span { class: "muted small",
                    if owned {
                        "Evaluates effectiveness of uma's existing skills by simming with/without them."
                    } else {
                        "Includes uma's existing skillset in sim, evaluates effectiveness of new ones."
                    }
                }
            }
            if !owned {
                skills::SkillFilters { filter: s.g_filter }
            }
            if let Some(p) = &poor {
                div { class: "warn",
                    "Poor aptitude on this course: {p}. Her speed is heavily reduced and gains will look flat; pick a course that suits her, or raise the aptitude in Setup."
                }
            }
            div { class: "runbar",
                button { class: "btn big", disabled: running() || n_c == 0, onclick: start, "Measure {thousands(n_c)} skills" }
                span { class: "muted", "{thousands(n_c)} skills match · {thousands(races)} races, {eta}" }
                Progress {}
            }
        }
        GainsResults {}
    }
}

#[component]
fn GainsResults() -> Element {
    let s = use_context::<S>();
    let (sort, mut sel) = (s.g_sort, s.g_sel);
    let stale = use_stale(s, move || s.g_res.read().as_ref().map(|r| r.rev));
    // The open skill can go to the measured uma: still at her gate, without it yet.
    let can_add = use_memo(move || {
        let (r, id) = (s.g_res.read(), *s.g_sel.read());
        let (Some(r), Some(id)) = (r.as_ref(), id) else { return false };
        s.input.read().horses.get(r.horse).is_some_and(|h| h.card_id == r.card && !h.skills.iter().any(|x| x.skill_id == id))
    });
    let Some(r) = s.g_res.cloned() else { return rsx! {} };
    let (col, desc) = *sort.read();
    let mut rows: Vec<(SkillInfo, &SkillGain)> = r.items.iter().map(|g| (skill_info(g.skill_id), g)).collect();
    let key = |g: &SkillGain| match col {
        2 => g.lengths.p50,
        3 => g.lengths.min,
        4 => g.lengths.max,
        5 => g.rate,
        _ => g.lengths.mean,
    };
    if col == 0 {
        rows.sort_by(|a, b| a.0.name.cmp(&b.0.name));
    } else {
        rows.sort_by(|a, b| key(a.1).total_cmp(&key(b.1)));
    }
    if desc {
        rows.reverse();
    }
    // The chart always ranks by mean (its right column), whatever the table's sort.
    let chart: Vec<(String, String, [f32; 6], usize)> = r
        .items
        .iter()
        .take(CHART_ROWS)
        .map(|g| {
            let k = skill_info(g.skill_id);
            let name = if k.inherited { format!("{} (inherited)", k.name) } else { k.name };
            let l = &g.lengths;
            (name, rarity_color(k.rarity).to_string(), [l.p10, l.p25, l.p50, l.p75, l.p90, l.mean], g.samples.len())
        })
        .collect();
    let selected = sel.cloned().and_then(|id| r.items.iter().find(|g| g.skill_id == id));
    let partial = r.items.len() < r.asked;
    rsx! {
        div { id: "gains", class: "anchor" }
        if stale() {
            div { class: "stale", "The setup changed since this measurement. Measure again to update it." }
        }
        section { class: "card",
            div { class: "card-h",
                {uma_icon(r.card, r.chara, None, "md")}
                h3 { "{chara_name(r.chara)}: {thousands(r.items.len())} skills × {r.n} seeds" }
                span { class: "chip", if r.npc { "NPCs" } else { "This field" } }
                span { class: "muted", "lengths gained (1 length = 2.5 m)" }
            }
            {spurt_warn(&r.low_spurt)}
            if partial {
                p { class: "muted small", "Cancelled: showing the {r.items.len()} of {r.asked} skills that finished all seeds." }
            }
            if r.items.is_empty() {
                p { class: "muted", "Nothing measured." }
            } else {
                if r.items.len() > CHART_ROWS { p { class: "muted small", "Top {CHART_ROWS} by mean gain:" } }
                GainChart { rows: chart }
            }
        }
        if let Some(g) = selected {
            {
                let k = skill_info(g.skill_id);
                let (horse, new) = (r.horse, learned(&k));
                rsx! {
                    // Inline (not a drawer), so the pinned course above shows where it activates.
                    section { class: "card detail", id: "gdetail",
                        div { class: "card-h",
                            {skill_row(&k, None, "big")}
                            div { class: "grow" }
                            button { class: "btn icon", title: "Close", onclick: move |_| sel.set(None), {icon("x")} }
                        }
                        div { class: "kpis",
                            div { b { "{g.lengths.mean:+.2}" } span { "mean lengths" } }
                            div { b { "{g.lengths.p50:+.2}" } span { "median" } }
                            div { b { "{g.lengths.min:+.2} to {g.lengths.max:+.2}" } span { "min to max" } }
                            div { b { "{pct(g.rate)}" } span { "activates" } }
                            if can_add() {
                                button { class: "btn alt", onclick: move |_| add_skill(s, horse, new), "Add to {chara_name(r.chara)}" }
                            }
                        }
                        {skill_facts(g.skill_id, Some(r.cv.distance))}
                        {gain_charts(&g.samples, &g.by_distance, rarity_color(k.rarity), &r.cv, "it")}
                    }
                }
            }
        }
        if !r.items.is_empty() {
            section { class: "card",
                div { class: "scroll-x",
                    table { class: "grid gains",
                        thead {
                            tr {
                                {sort_th(sort, 0, "Skill", false, true)}
                                {sort_th(sort, 1, "Mean", true, false)}
                                {sort_th(sort, 2, "Median", true, false)}
                                {sort_th(sort, 3, "Min", true, false)}
                                {sort_th(sort, 4, "Max", true, false)}
                                {sort_th(sort, 5, "Activates", true, false)}
                            }
                        }
                        tbody {
                            for (k, g) in rows {
                                tr {
                                    key: "{g.skill_id}",
                                    class: if sel.read().as_ref() == Some(&g.skill_id) { "sel" },
                                    onclick: {
                                        let id = g.skill_id;
                                        move |_| {
                                            sel.set(Some(id));
                                            reveal("gdetail");
                                        }
                                    },
                                    td { {skill_row(&k, None, "")} }
                                    td { class: if g.lengths.mean > 0.005 { "num good" } else { "num" }, b { "{g.lengths.mean:+.2}" } }
                                    td { class: "num", "{g.lengths.p50:+.2}" }
                                    td { class: "num muted", "{g.lengths.min:+.2}" }
                                    td { class: "num muted", "{g.lengths.max:+.2}" }
                                    td { class: "num", "{pct(g.rate)}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
