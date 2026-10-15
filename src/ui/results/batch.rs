//! Batch results: the per-uma stats table.

use super::*;
use super::detail::horse_detail;

/// Inline mini histogram of finishing places.
fn mini_hist(h: &[f32]) -> Element {
    let max = h.iter().cloned().fold(0.0001, f32::max);
    rsx! {
        div { class: "mini",
            for (k, v) in h.iter().enumerate() {
                div { title: "{k + 1}: {pct(*v)}", style: "height: {v / max * 100.0}%" }
            }
        }
    }
}

/// Team win rates in a row: each team's tag, its members' icons, and the share of races one of them won.
fn team_wr(res: &Res, cur: &Batch, base: Option<&Arc<Batch>>) -> Element {
    let ts = teams(&res.input, res.player);
    if ts.is_empty() {
        return rsx! {};
    }
    let wr = |b: &Batch, t: i32| -> f32 {
        res.input.horses.iter().zip(&b.horses).filter(|(h, _)| h.team_id == t).map(|(_, s)| s.win_rate).sum()
    };
    rsx! {
        div { class: "team-wr",
            span { class: "muted small", "Team win rate" }
            for t in ts {
                {
                    let i0 = res.input.horses.iter().position(|h| h.team_id == t).unwrap_or(0);
                    let now = wr(cur, t);
                    let was = base.map(|b| wr(b, t));
                    rsx! {
                        div { key: "{t}", class: "tw",
                            {team_tags(&res.input, res.player, i0)}
                            for (i, h) in res.input.horses.iter().enumerate().filter(|(_, h)| h.team_id == t) {
                                {horse_icon(h, Some(&res.colors[i]), "sm")}
                            }
                            b { "{pct(now)}" }
                            if let Some(was) = was.filter(|&w| pct(w) != pct(now)) { {pp_delta(was, now, false)} }
                        }
                    }
                }
            }
        }
    }
}

pub(super) fn batch_view(s: S, res: &Res, names: &[String], cur: &Arc<Batch>, base: Option<&Arc<Batch>>) -> Element {
    let mut focus = s.focus;
    let f = *focus.read();
    let diffs = base.map(|b| diff_batches(b, cur));
    let n = cur.horses.len();
    let ed = &res.ed_mask;
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| cur.horses[a].place.mean.total_cmp(&cur.horses[b].place.mean));
    let improved = diffs.as_ref().is_some_and(|d| d.iter().zip(ed).any(|(d, &m)| m && d.win_rate.1 > d.win_rate.0 + 0.005));
    rsx! {
        section { class: "card",
            div { class: "card-h",
                h3 { "{cur.seeds.len()} races" if base.is_some() { ": baseline → edited" } }
                span { class: "muted", "click an uma for details" }
                if improved { img { class: "great", src: "/a/great.webp", alt: "Great!" } }
            }
            {team_wr(res, cur, base)}
            div { class: "scroll-x",
                table { class: "grid results batch",
                    thead {
                        tr {
                            th { "Uma" }
                            th { class: "num", "Win" }
                            th { class: "num", "Avg place" }
                            th { class: "c-hist", "Places 1 to {n}" }
                            th { class: "num", title: "Share of races with a full spurt: on time and at max speed", "Last spurt" }
                            th { class: "num", title: "Share of races that kept HP to the goal; share that ran out, mean metres before the goal", "HP" }
                            for e in EVENTS.iter() { th { class: "num", title: "{e.hint}. Share of races it happened in, mean seconds when it did.", "{e.label}" } }
                        }
                    }
                    tbody {
                        for i in order {
                            {
                                let h = &cur.horses[i];
                                let d = diffs.as_ref().map(|d| &d[i]);
                                let card = res.input.horses[i].card_id;
                                rsx! {
                                    tr {
                                        key: "{i}",
                                        class: if f == Some(i) { "sel" },
                                        onclick: move |_| if f == Some(i) { focus.set(None) } else { focus_gate(s, i, card) },
                                        td { class: "who", {who(res, names, i)} }
                                        td { class: "num",
                                            b { "{pct(h.win_rate)}" }
                                            if let Some(d) = d {
                                                if pct(d.win_rate.1) != pct(d.win_rate.0) {
                                                    div { class: "was", "{pct(d.win_rate.0)} " {pp_delta(d.win_rate.0, d.win_rate.1, false)} }
                                                }
                                            }
                                        }
                                        td { class: "num",
                                            "{h.place.mean:.2}"
                                            if let Some(d) = d {
                                                if format!("{:.2}", d.mean_place.1) != format!("{:.2}", d.mean_place.0) {
                                                    div { class: "was", "{d.mean_place.0:.2} " {delta(d.mean_place.1 - d.mean_place.0, true, format!("{:.2}", (d.mean_place.1 - d.mean_place.0).abs()))} }
                                                }
                                            }
                                        }
                                        td { class: "c-hist", {mini_hist(&h.place_hist)} }
                                        td { class: "num",
                                            div { title: "Full spurt: on time and at max speed",
                                                b { "{pct(h.full_spurt_rate)}" }
                                                if let Some(d) = d {
                                                    if pct(d.full_spurt_rate.1) != pct(d.full_spurt_rate.0) {
                                                        " "
                                                        {pp_delta(d.full_spurt_rate.0, d.full_spurt_rate.1, false)}
                                                    }
                                                }
                                            }
                                        }
                                        td { class: "num",
                                            if h.hp_out_rate < 1.0 {
                                                div { class: "ok", "Survived" if h.hp_out_rate > 0.0 { " {pct(1.0 - h.hp_out_rate)}" } }
                                            }
                                            if h.hp_out_rate > 0.0 {
                                                div { class: if h.hp_out_rate < 1.0 { "small no" } else { "no" },
                                                    "Died" if h.hp_out_rate < 1.0 { " {pct(h.hp_out_rate)}" }
                                                    if !h.hp_out_distances.is_empty() { " (-{res.cv.distance - dist(h.hp_out_distances.clone()).mean:.0}m)" }
                                                }
                                            }
                                        }
                                        for e in EVENTS.iter() {
                                            {
                                                let (kind, more_good) = (e.kind, e.more_good);
                                                let (rate, secs) = h.states[kind as usize];
                                                rsx! {
                                                    td { class: "num",
                                                        if rate > 0.0 { b { "{pct(rate)}" } } else { span { class: "muted", "-" } }
                                                        if let Some(b) = base {
                                                            {
                                                                let (was, was_s) = b.horses[i].states[kind as usize];
                                                                state_arrow(was, rate, 0.0005, more_good, format!("was {} · {was_s:.1}s", pct(was)))
                                                            }
                                                        }
                                                        if rate > 0.0 { div { class: "small muted", "{secs:.1}s" } }
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
            }
        }
        if let Some(i) = f.filter(|&i| i < n) {
            {horse_detail(s, res, i, cur, base, diffs.as_ref().map(|d| &d[i]))}
        }
    }
}
