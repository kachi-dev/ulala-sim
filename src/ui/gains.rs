//! Skill Gains tab (umalator-style skill chart): one uma among NPCs or in her field, each candidate skill on vs muted
//! on the same seeds, measured in horse lengths.

use super::*;
use crate::charts::{GainBars, GainChart, Histogram};

const CHART_ROWS: usize = 20;

/// Sortable columns of the skill gains table.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum GainCol {
    Skill,
    Mean,
    Median,
    Min,
    Max,
    Activates,
}
/// Seeds for "Improve graph accuracy": one skill re-measured alone, for smoother detail charts.
const ACCURACY_N: usize = 2000;

/// Re-measures skill `id` of the current measurement over `ACCURACY_N` fresh seeds (a random base, not the race's)
/// in the same field and swaps it in.
fn improve_accuracy(s: S, id: i32) {
    let (Some(race), Some(r)) = (s.race.cloned(), s.g_res.cloned()) else { return };
    let (mut input, i) = npc_field(&race, &s.input.peek(), r.horse, r.npc, r.lobby, r.size);
    input.random_seed = random_seed();
    let work = ACCURACY_N * 2 * input.horses.len();
    let label = format!("Measuring {} × {} seeds", skill_info(id).name, thousands(ACCURACY_N));
    let run = move |w: &Work| skill_gains(&race, &input, i, &[id], r.blank, ACCURACY_N, w).0;
    measure(s, label, ACCURACY_N, work, run, move |mut s, mut fresh| {
        // Cancelled (no full sample set), or a new measurement replaced this one meanwhile: keep what's there.
        let Some(g) = fresh.pop() else { return };
        let mut res = s.g_res.write();
        let Some(r) = res.as_mut().filter(|x| x.rev == r.rev && Arc::ptr_eq(&x.items, &r.items)) else { return };
        let mut items = (*r.items).clone();
        if let Some(k) = items.iter().position(|x| x.skill_id == id) {
            items[k] = g;
        }
        items.sort_by(|a, b| b.lengths.mean.total_cmp(&a.lengths.mean));
        r.items = Arc::new(items);
    });
}


/// Detail charts of one measured gain (Uma ranking's too): lengths gained by where it activates, and gain per seed.
pub(super) fn gain_charts(samples: &[f32], by_distance: &[(f32, f32)], color: &str, cv: &CourseView, what: &str) -> Element {
    rsx! {
        h4 { "Lengths gained by where {what} activates (average per 50 m)" }
        GainBars {
            by_distance: by_distance.to_vec(),
            course_distance: cv.distance,
            phases: cv.phases.to_vec(),
            corners: cv.corner_spans(),
            straights: cv.straights.iter().map(|c| (c.start, c.end)).collect::<Vec<_>>(),
            bin: 50.0,
        }
        h4 { "Gain per seed (lengths)" }
        Histogram { values: samples.to_vec(), color: color.to_string(), unit: "L".to_string() }
    }
}

#[component]
pub fn GainsTab() -> Element {
    let s = use_context::<S>();
    let (mut g_owned, mut g_blank) = (s.g_owned, s.g_blank);
    let running = use_busy(s);
    let input = s.input.read();
    if input.horses.is_empty() {
        return rsx! {
            section { class: "card", p { class: "muted", "Add an uma in Setup first." } }
        };
    }
    let field_n = input.horses.len();
    let hi = (*s.sel.read()).min(field_n - 1);
    let horse = input.horses[hi].clone();
    drop(input);
    let cv = s.cview.cloned();
    let poor = poor_apts(&horse, cv.as_deref());
    let (owned, n) = (*g_owned.read(), *s.g_n.read());
    let blank = owned && *g_blank.read();
    let f = s.g_filter.cloned();
    // New skills: ones she doesn't have that can activate here (right course, conditions and style for her).
    let mut never = 0;
    let has = |id: i32| horse.skills.iter().any(|k| k.skill_id == id);
    let cands: Vec<i32> = if owned {
        horse.skills.iter().map(|k| k.skill_id).collect()
    } else {
        let ids: Vec<i32> = s.skills.read().iter().filter(|k| !has(k.id) && skills::learnable(k, horse.card_id) && skills::matches(&f, k)).map(|k| k.id).collect();
        let n = ids.len();
        let ok = match s.race.cloned() {
            Some(race) => can_activate(&race, &s.input.read(), hi, ids),
            None => ids,
        };
        never = n - ok.len();
        ok
    };
    let (mode, npc, size) = mode_seg(s, s.g_npc, field_n);
    let n_c = cands.len();
    let races = n_c * n * 2;
    let work = races * size;
    let eta = eta(s, work);
    let start = move |_| {
        let (Some(race), Some(cv)) = (s.race.cloned(), s.cview.cloned()) else { return };
        if cands.is_empty() {
            return;
        }
        let (lobby, size) = (*s.npc_lobby.peek(), *s.field_size.peek());
        let (input, i) = npc_field(&race, &s.input.peek(), hi, npc, lobby, size);
        let (cands, rev, field) = (cands.clone(), *s.rev.peek(), input.horses.clone());
        let (card, chara, asked) = (horse.card_id, horse.chara_id, cands.len());
        let label = format!("Measuring {} skills × {n} seeds", thousands(asked));
        let run = move |w: &Work| skill_gains(&race, &input, i, &cands, blank, n, w);
        measure(s, label, asked * n, work, run, move |mut s, (items, spurt)| {
            s.g_sel.set(None);
            let low_spurt = low_spurts(&field, &spurt);
            s.g_res.set(Some(GainsRes { horse: hi, card, chara, items: Arc::new(items), cv, n, asked, npc, blank, lobby, size, low_spurt, rev }));
            reveal("gains");
        });
    };
    rsx! {
        section { class: "card",
            div { class: "card-h",
                h3 { "Skill gains" }
            }
            div { class: "gsetup",
                {who_sel(s)}
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
            if owned {
                div { class: "card-h cands",
                    span { class: "sf-label", "Baseline" }
                    div { class: "seg",
                        button { class: if blank { "on" }, onclick: move |_| g_blank.set(true), "Blank" }
                        button { class: if !blank { "on" }, onclick: move |_| g_blank.set(false), "All skills" }
                    }
                    Help {
                        p {
                            b { "Blank" }
                            " - Comparison baseline is set to a sim with 0 skills: gives skills' raw length contribution. Think of this as how much a skill is worth on its own."
                        }
                        p {
                            b { "All skills" }
                            " - Comparison baseline includes all skills: gives skills' length contribution within current skillset. Think of this as how much a skill is currently contributing. For example, despite accel skills being highly valued, if you already have 3 this sim will highlight how much that 3rd accel is actually needed."
                        }
                    }
                }
            } else {
                skills::SkillFilters { filter: s.g_filter, fixed_course: true }
            }
            if let Some(p) = &poor {
                div { class: "warn",
                    "Poor aptitude on this course: {p}. Her speed is heavily reduced and gains will look flat; pick a course that suits her, or raise the aptitude in Setup."
                }
            }
            div { class: "runbar",
                button { class: "btn big", disabled: running() || n_c == 0, onclick: start, "Measure {thousands(n_c)} skills" }
                span { class: "muted",
                    "{thousands(n_c)} skills match"
                    if never > 0 { " ({thousands(never)} more can't activate on this course or style)" }
                    " · {thousands(races)} races, {eta}"
                }
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
    let running = use_busy(s);
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
        GainCol::Skill | GainCol::Mean => g.lengths.mean,
        GainCol::Median => g.lengths.p50,
        GainCol::Min => g.lengths.min,
        GainCol::Max => g.lengths.max,
        GainCol::Activates => g.rate,
    };
    if col == GainCol::Skill {
        rows.sort_by(|a, b| a.0.name.cmp(&b.0.name));
    } else {
        rows.sort_by(|a, b| key(a.1).total_cmp(&key(b.1)).then(a.1.lengths.mean.total_cmp(&b.1.lengths.mean)));
    }
    if desc {
        rows.reverse();
    }
    // Green/red only in the sorted column.
    let tone = |c: GainCol, v: f32, base: &'static str| match (c == col, v) {
        (true, v) if v > 0.005 => "num good",
        (true, v) if v < -0.005 => "num bad",
        _ => base,
    };
    // The chart always ranks by mean (its right column), whatever the table's sort.
    let mut swaps = Vec::new();
    let chart: Vec<(String, String, [f32; 6], usize)> = r
        .items
        .iter()
        .take(CHART_ROWS)
        .map(|g| {
            let k = skill_info(g.skill_id);
            let color = rarity_color(&k).to_string();
            let name = if k.inherited { format!("{} (inherited)", k.name) } else { k.name };
            let name = match (g.replaces, g.upgrade) {
                (Some(_), true) => format!("{name} (upgrade)"),
                (Some(_), false) => format!("{name} (downgrade)"),
                (None, _) => name,
            };
            if let Some(old) = g.replaces {
                swaps.push((name.clone(), skill_info(old).name));
            }
            let l = &g.lengths;
            (name, color, [l.p10, l.p25, l.p50, l.p75, l.p90, l.mean], g.samples.len())
        })
        .collect();
    let selected = sel.cloned().and_then(|id| r.items.iter().find(|g| g.skill_id == id));
    // The open skill's details, in a row under it.
    let detail = selected.map(|g| {
        let k = skill_info(g.skill_id);
        let (horse, new, old) = (r.horse, learned(&k), g.replaces);
        let id = g.skill_id;
        rsx! {
            div { class: "gdetail", id: "gdetail",
                if let Some(old) = old {
                    p { class: "swapline",
                        "Measured against "
                        b { "{skill_info(old).name}" }
                        ", which she has. Swapping it for {k.name} "
                        if g.lengths.mean < 0.0 { "loses " } else { "gains " }
                        b { "{g.lengths.mean.abs():.2} L" }
                        " on average."
                    }
                }
                div { class: "kpis",
                    div {
                        b { "{g.lengths.mean:+.2}" }
                        span {
                            "mean lengths"
                            if let Some(old) = old { " vs {skill_info(old).name}" }
                        }
                    }
                    div { b { "{g.lengths.p50:+.2}" } span { "median" } }
                    div { b { "{g.lengths.min:+.2} to {g.lengths.max:+.2}" } span { "min to max" } }
                    div { b { "{pct(g.rate)}" } span { "activates" } }
                    if can_add() {
                        if let Some(old) = old {
                            button {
                                class: "btn alt",
                                onclick: move |_| edit(s, horse, |h| {
                                    h.skills.retain(|x| x.skill_id != old);
                                    h.skills.push(new);
                                }),
                                "Swap in for {skill_info(old).name}"
                            }
                        } else {
                            button { class: "btn alt", onclick: move |_| add_skill(s, horse, new), "Add to {chara_name(r.chara)}" }
                        }
                    }
                    if g.samples.len() < ACCURACY_N {
                        button {
                            class: "btn ghost",
                            disabled: running() || stale(),
                            title: if stale() { "The setup changed since this measurement: measure again first" } else { "Run this skill alone over 2,000 seeds for smoother charts" },
                            onclick: move |_| improve_accuracy(s, id),
                            "Improve graph accuracy"
                        }
                    }
                }
                {skill_facts(g.skill_id, Some(r.cv.distance))}
                {gain_charts(&g.samples, &g.by_distance, rarity_color(&k), &r.cv, "it")}
            }
        }
    });
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
                if r.blank { span { class: "chip", "vs no skills" } }
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
                for (name, old) in swaps {
                    p { class: "chartnote", b { "{name}" } ": gain over {old}, which she has." }
                }
            }
        }
        if !r.items.is_empty() {
            section { class: "card",
                div { class: "scroll-x",
                    table { class: "grid gains",
                        thead {
                            tr {
                                {sort_th(sort, GainCol::Skill, "Skill", false, true)}
                                {sort_th(sort, GainCol::Mean, "Mean", true, false)}
                                {sort_th(sort, GainCol::Median, "Median", true, false)}
                                {sort_th(sort, GainCol::Min, "Min", true, false)}
                                {sort_th(sort, GainCol::Max, "Max", true, false)}
                                {sort_th(sort, GainCol::Activates, "Activates", true, false)}
                            }
                        }
                        tbody {
                            for (k, g) in rows {
                                tr {
                                    key: "{g.skill_id}",
                                    class: if sel.read().as_ref() == Some(&g.skill_id) { "sel" },
                                    // Opens its details right under it; clicking it again closes them.
                                    onclick: {
                                        let id = g.skill_id;
                                        move |_| {
                                            let open = *sel.peek() == Some(id);
                                            sel.set(if open { None } else { Some(id) });
                                        }
                                    },
                                    td {
                                        if let Some(old) = g.replaces {
                                            div { class: "skcell",
                                                {skill_row(&k, None, "")}
                                                span { class: "swapnote",
                                                    if g.upgrade { "upgrade of " } else { "downgrade of " }
                                                    b { "{skill_info(old).name}" }
                                                }
                                            }
                                        } else {
                                            {skill_row(&k, None, "")}
                                        }
                                    }
                                    td { class: tone(GainCol::Mean, g.lengths.mean, "num"), b { "{g.lengths.mean:+.2}" } }
                                    td { class: tone(GainCol::Median, g.lengths.p50, "num"), "{g.lengths.p50:+.2}" }
                                    td { class: tone(GainCol::Min, g.lengths.min, "num muted"), "{g.lengths.min:+.2}" }
                                    td { class: tone(GainCol::Max, g.lengths.max, "num muted"), "{g.lengths.max:+.2}" }
                                    td { class: "num", "{pct(g.rate)}" }
                                }
                                if sel.read().as_ref() == Some(&g.skill_id) {
                                    tr { key: "{g.skill_id}-detail", class: "expand",
                                        td { colspan: "6", {detail.clone()} }
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
