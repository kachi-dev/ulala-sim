//! Uma ranking tab: which costume (by its unique skill) and running style is best on this course. The selected uma is
//! the template (stats, skills, mood); each card's unique is swapped in, the style forced, aptitudes A.

use super::gains::gain_charts;
use super::*;

fn heat(v: f32, max: f32) -> String {
    let a = if max > 0.0 { (v / max).clamp(0.0, 1.0) } else { 0.0 };
    format!("background: color-mix(in srgb, var(--accent) {:.0}%, transparent)", a * 70.0)
}

/// Costume cell: icon, name and costume title, plus the unique skill's name when `unique`.
fn card_who(card: i32, unique: bool) -> Element {
    let ci = card_info(card);
    let chara = ci.as_ref().map_or(0, |c| c.chara_id);
    let u = ci.filter(|_| unique).map(|c| skill_info(c.unique_skill));
    rsx! {
        div { class: "whobox",
            {uma_icon(card, chara, None, "sm")}
            div {
                b { "{chara_name(chara)}" }
                div { class: "muted small", "{card_title(card)}" }
                if let Some(u) = u { div { class: "small uname", title: "{u.description}", "{u.name}" } }
            }
        }
    }
}

#[component]
pub fn RankingTab() -> Element {
    let s = use_context::<S>();
    let (mut r_styles, mut r_q) = (s.r_styles, s.r_q);
    let running = use_busy(s);
    let input = s.input.read();
    if input.horses.is_empty() {
        return rsx! { section { class: "card", p { class: "muted", "Add an uma in Setup first." } } };
    }
    let field_n = input.horses.len();
    let hi = (*s.r_horse.read()).min(field_n - 1);
    drop(input);
    let styles = r_styles.cloned();
    let ql = r_q.read().to_lowercase();
    let data = s.race.read().as_ref().map(|r| r.data.clone());
    // Only costumes whose unique the race's game data has (an older save predates newer umas).
    let picked: Vec<i32> = cards()
        .iter()
        .filter(|c| data.as_ref().is_some_and(|d| d.skills.contains_key(&c.unique_skill)))
        .filter(|c| ql.is_empty() || c.name.to_lowercase().contains(&ql) || c.title.to_lowercase().contains(&ql))
        .map(|c| c.card_id)
        .collect();
    let (mode, npc, size) = mode_seg(s, s.r_npc, field_n);
    let n = *s.r_n.read();
    let combos = picked.len() * styles.len();
    let work = combos * n * 2 * size;
    let eta = estimate(work as f32 / s.g_rate.read().max(1.0));
    let n_picked = picked.len();
    let styles_on = styles.clone();
    let start = move |_| {
        let (Some(race), Some(cv)) = (s.race.cloned(), s.cview.cloned()) else { return };
        if combos == 0 {
            return;
        }
        let (input, i) = npc_field(&race, &s.input.peek(), hi, npc, *s.npc_lobby.peek());
        let (cards, styles, rev, field) = (picked.clone(), styles.clone(), *s.rev.peek(), input.horses.clone());
        let label = format!("Ranking {} umas × {} styles × {n} seeds", cards.len(), styles.len());
        let run = move |w: &Work| uma_ranking(&race, &input, i, &cards, &styles, n, w);
        measure(s, label, combos * n, work, run, move |mut s, (items, spurt)| {
            s.r_sel.set(None);
            let low_spurt = low_spurts(&field, &spurt);
            s.r_res.set(Some(RankRes { items: Arc::new(items), cv, n, npc, low_spurt, asked: combos, rev }));
            reveal("ranking");
        });
    };
    rsx! {
        section { class: "card",
            div { class: "card-h",
                h3 { "Uma ranking" }
                span { class: "muted small",
                    "Which uma wins on this course by her unique skill: the chosen uma's stats and skills, each costume's unique swapped in, run at each style with A aptitude, unique on vs muted."
                }
            }
            div { class: "gsetup",
                {who_pick(s, s.r_horse)}
                {mode}
                div { class: "field",
                    span { "Styles" }
                    div { class: "seg",
                        for v in 1..=4 {
                            button {
                                class: if styles_on.contains(&v) { "on" },
                                onclick: move |_| {
                                    let mut st = r_styles.write();
                                    skills::toggle(&mut st, v);
                                    st.sort();
                                },
                                "{style_label(v)}"
                            }
                        }
                    }
                }
                label { class: "field",
                    span { "Umas" }
                    input { r#type: "search", placeholder: "Filter by name or costume…", value: "{r_q}", oninput: move |e| r_q.set(e.value()) }
                }
                label { class: "field",
                    span { "Samples" }
                    {seeds_input("rn", s.r_n, 10, 5000)}
                }
            }
            div { class: "runbar",
                button { class: "btn big", disabled: running() || combos == 0, onclick: start, "Measure {thousands(combos)} combinations" }
                span { class: "muted", "{thousands(n_picked)} costumes × {styles_on.len()} styles · {thousands(combos * n * 2)} races, {eta}" }
                Progress {}
            }
        }
        RankResults {}
    }
}

#[component]
fn RankResults() -> Element {
    let s = use_context::<S>();
    let (sort, mut sel, mut matrix) = (s.r_sort, s.r_sel, s.r_matrix);
    let stale = use_stale(s, move || s.r_res.read().as_ref().map(|r| r.rev));
    let Some(r) = s.r_res.cloned() else { return rsx! {} };
    let partial = r.items.len() < r.asked;
    let is_matrix = *matrix.read();
    let max = r.items.iter().map(|x| x.lengths.mean).fold(0.0f32, f32::max);
    let cells: HashMap<(i32, i32), &UmaRank> = r.items.iter().map(|x| ((x.card_id, x.style), x)).collect();
    let cell = |card: i32, style: i32| cells.get(&(card, style)).copied();
    // Matrix rows: costumes in best-first order.
    let mut seen = HashSet::new();
    let cards_order: Vec<i32> = r.items.iter().map(|x| x.card_id).filter(|&c| seen.insert(c)).collect();
    let (col, desc) = *sort.read();
    let mut rows: Vec<&UmaRank> = r.items.iter().collect();
    let key = |x: &UmaRank| match col {
        2 => x.lengths.p50,
        3 => x.rate,
        4 => x.style as f32,
        _ => x.lengths.mean,
    };
    rows.sort_by(|a, b| key(a).total_cmp(&key(b)));
    if desc {
        rows.reverse();
    }
    let selected = sel.cloned().and_then(|(c, st)| cell(c, st));
    rsx! {
        div { id: "ranking", class: "anchor" }
        if stale() {
            div { class: "stale", "The setup changed since this ranking. Measure again to update it." }
        }
        section { class: "card",
            div { class: "card-h",
                h3 { "{thousands(r.items.len())} combinations × {r.n} seeds" }
                span { class: "chip", if r.npc { "NPCs" } else { "This field" } }
                span { class: "muted small", "unique skill gain in lengths" }
                div { class: "grow" }
                div { class: "seg small",
                    button { class: if is_matrix { "on" }, onclick: move |_| matrix.set(true), "Matrix" }
                    button { class: if !is_matrix { "on" }, onclick: move |_| matrix.set(false), "Table" }
                }
            }
            {spurt_warn(&r.low_spurt)}
            if partial {
                p { class: "muted small", "Cancelled: showing the {r.items.len()} of {r.asked} combinations that finished all seeds." }
            }
            div { class: "scroll-x",
                if is_matrix {
                    table { class: "grid matrix",
                        thead {
                            tr {
                                th { "Uma" }
                                for l in &STYLES[1..] { th { class: "num", "{l}" } }
                            }
                        }
                        tbody {
                            for card in cards_order {
                                {
                                    let best = (1..=4).filter_map(|st| cell(card, st)).map(|x| x.lengths.mean).fold(f32::MIN, f32::max);
                                    rsx! {
                                        tr { key: "{card}",
                                            td { class: "who", {card_who(card, true)} }
                                            for st in 1..=4 {
                                                if let Some(x) = cell(card, st) {
                                                    {
                                                        let never = x.rate <= 0.0;
                                                        let class = if never { "num heat never" } else if x.lengths.mean == best && best > 0.005 { "num heat best" } else { "num heat" };
                                                        let tip = if never {
                                                            "Unique never activated: check its conditions (e.g. skills it needs to fire first)".to_string()
                                                        } else {
                                                            format!("median {:+.2}, activates {}", x.lengths.p50, pct(x.rate))
                                                        };
                                                        rsx! {
                                                            td {
                                                                class: "{class}",
                                                                style: if never { String::new() } else { heat(x.lengths.mean, max) },
                                                                title: "{tip}",
                                                                onclick: move |_| { sel.set(Some((card, st))); reveal("rdetail"); },
                                                                div { "{x.lengths.mean:+.2}" }
                                                                small { if never { "never fired" } else { "{x.rate * 100.0:.0}% fired" } }
                                                            }
                                                        }
                                                    }
                                                } else {
                                                    td { class: "num muted", "-" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    table { class: "grid",
                        thead {
                            tr {
                                th { "Uma" }
                                {sort_th(sort, 4, "Style", true, true)}
                                {sort_th(sort, 1, "Mean", true, false)}
                                {sort_th(sort, 2, "Median", true, false)}
                                th { class: "num", "p10 to p90" }
                                {sort_th(sort, 3, "Activates", true, false)}
                            }
                        }
                        tbody {
                            for x in rows {
                                {
                                    let (card, st) = (x.card_id, x.style);
                                    rsx! {
                                        tr { key: "{card}-{st}", onclick: move |_| { sel.set(Some((card, st))); reveal("rdetail"); },
                                            td { class: "who", {card_who(card, false)} }
                                            td { class: "num", "{style_label(st)}" }
                                            td { class: if x.lengths.mean > 0.005 { "num good" } else { "num" }, b { "{x.lengths.mean:+.2}" } }
                                            td { class: "num", "{x.lengths.p50:+.2}" }
                                            td { class: "num muted", "{x.lengths.p10:+.2} to {x.lengths.p90:+.2}" }
                                            td { class: "num", "{pct(x.rate)}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if let Some(x) = selected {
            {
                let ci = card_info(x.card_id);
                let chara = ci.as_ref().map_or(0, |c| c.chara_id);
                let unique = ci.map(|c| skill_info(c.unique_skill));
                rsx! {
                    // Inline (not a drawer), so the pinned course above shows where the unique activates.
                    section { class: "card detail", id: "rdetail",
                        div { class: "card-h",
                            {uma_icon(x.card_id, chara, None, "md")}
                            div {
                                h3 { "{chara_name(chara)}" }
                                div { class: "muted small", "{card_title(x.card_id)} · {style_label(x.style)}" }
                            }
                            if let Some(u) = &unique { {skill_row(u, None, "")} }
                            div { class: "grow" }
                            button { class: "btn icon", title: "Close", onclick: move |_| sel.set(None), {icon("x")} }
                        }
                        div { class: "kpis",
                            div { b { "{x.lengths.mean:+.2}" } span { "mean lengths" } }
                            div { b { "{x.lengths.p50:+.2}" } span { "median" } }
                            div { b { "{x.lengths.p10:+.2} to {x.lengths.p90:+.2}" } span { "p10 to p90" } }
                            div { b { "{pct(x.rate)}" } span { "unique activates" } }
                        }
                        {gain_charts(&x.samples, &x.by_distance, "var(--accent)", &r.cv, "the unique")}
                    }
                }
            }
        }
    }
}
