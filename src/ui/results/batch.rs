//! Batch results: the per-uma stats table.

use super::*;
use super::detail::horse_detail;

/// Team win rates in a row: each team's tag, its members' icons, and the share of races one of them won.
fn team_wr(res: &Res, cur: &Batch, base: Option<&Arc<Batch>>) -> Element {
    let ts = teams(&res.input, res.player);
    if ts.is_empty() {
        return rsx! {};
    }
    // Team `t`'s win rate in `b`, run on field `inp`.
    let wr = |inp: &RaceInput, b: &Batch, t: i32| -> f32 {
        inp.horses.iter().zip(&b.horses).filter(|(h, _)| h.team_id == t).map(|(_, s)| s.win_rate).sum()
    };
    rsx! {
        div { class: "team-wr",
            span { class: "muted small", "Team win rate" }
            for t in ts {
                {
                    let i0 = res.input.horses.iter().position(|h| h.team_id == t).unwrap_or(0);
                    let now = wr(&res.input, cur, t);
                    let was = base.zip(res.base_input.as_deref()).map(|(b, inp)| wr(inp, b, t));
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

/// With rivals: races the umas won and the rivals won, and the pool (`rival_stats`).
fn rival_kpis(r: &Rivals, cur: &Batch, st: &[RivalStat]) -> Element {
    let races = cur.seeds.len();
    let won = (0..races).filter(|&k| (0..r.from).any(|h| cur.horses[h].places.get(k) == Some(&1))).count();
    let drawn: usize = st.iter().map(|x| x.drawn).sum();
    let singles = r.pool.iter().all(|t| t.horses.len() == 1);
    let what = if singles { "umas" } else { "teams" };
    let per = drawn as f32 / races.max(1) as f32;
    let each = drawn as f32 / r.pool.len().max(1) as f32;
    rsx! {
        div { class: "kpis",
            div {
                span { "Your umas" }
                b { "{won} of {races}" }
                div { class: "cap", "{pct(won as f32 / races.max(1) as f32)} win rate" }
            }
            div {
                span { "Rivals" }
                b { "{races - won} of {races}" }
                div { class: "cap", "races won by a rival" }
            }
            div {
                span { "Rival pool" }
                b { "{r.pool.len()} {what}" }
                div { class: "cap", if each < 10.0 { "{per:.0} per race, each in about {each:.1} races" } else { "{per:.0} per race, each in about {each:.0} races" } }
            }
        }
    }
}

/// With rivals: each drawn team's record, most wins first, in a scroll box; clicking one opens its umas' cards under
/// it (`s.rival_open`).
fn rival_table(s: S, r: &Rivals, st: &[RivalStat]) -> Element {
    const SHOWN: usize = 200;
    let singles = r.pool.iter().all(|t| t.horses.len() == 1);
    let mut open = s.rival_open;
    let opened = *open.read();
    let cols = if singles { 5 } else { 6 };
    rsx! {
        section { class: "card",
            div { class: "card-h",
                h3 { if singles { "Rival umas" } else { "Rival teams" } }
                span { class: "muted", "{st.len()} drawn · most wins first" }
            }
            div { class: "rv-scroll",
                table { class: "grid results rv-res",
                    thead {
                        tr {
                            th { if singles { "Uma" } else { "Team" } }
                            th { class: "num", "Races drawn" }
                            th { class: "num", "Win rate" span { class: "sub", "of the races drawn" } }
                            th { class: "num", "Ahead of you" span { class: "sub", "best place beat yours" } }
                            if !singles { th { "Top uma" span { class: "sub", "most wins in the team" } } }
                            th { "From" }
                        }
                    }
                    tbody {
                        for x in st.iter().take(SHOWN) {
                            {
                                let t = &r.pool[x.team];
                                let d = x.drawn.max(1) as f32;
                                let top = (0..t.horses.len()).max_by_key(|&m| x.uma_wins[m]).filter(|&m| x.uma_wins[m] > 0);
                                let team = x.team;
                                rsx! {
                                    tr {
                                        key: "{x.team}",
                                        class: if opened == Some(team) { "sel" },
                                        onclick: move |_| open.set(if opened == Some(team) { None } else { Some(team) }),
                                        td { class: "who",
                                            div { class: "whobox",
                                                span { class: "mobs", for h in &t.horses { {horse_icon(h, None, "sm")} } }
                                                div {
                                                    b { "{t.trainer}" }
                                                    if singles { div { class: "muted small", "{horse_name(&t.horses[0])}" } }
                                                }
                                            }
                                        }
                                        td { class: "num", "{x.drawn}" }
                                        td { class: "num", b { "{pct(x.wins as f32 / d)}" } div { class: "small muted", "{x.wins} wins" } }
                                        td { class: "num", b { "{pct(x.ahead as f32 / d)}" } div { class: "small muted", "{x.ahead} of {x.drawn}" } }
                                        if !singles {
                                            td { class: "who",
                                                if let Some(m) = top {
                                                    div { class: "whobox",
                                                        {horse_icon(&t.horses[m], None, "sm")}
                                                        div {
                                                            b { "{horse_name(&t.horses[m])}" }
                                                            div { class: "muted small", "won {x.uma_wins[m]} of {x.drawn}" }
                                                        }
                                                    }
                                                } else {
                                                    span { class: "muted", "-" }
                                                }
                                            }
                                        }
                                        td { class: "muted small", "{t.category} · {t.seen} replay" if t.seen != 1 { "s" } }
                                    }
                                    if opened == Some(team) {
                                        tr { key: "{x.team}-cards", class: "expand",
                                            td { colspan: "{cols}",
                                                div { class: "rv-cards",
                                                    for h in &t.horses {
                                                        div { class: "rv-card",
                                                            div { class: "vc-h",
                                                                {horse_icon(h, None, "md")}
                                                                div { class: "vc-name",
                                                                    b { class: "ell", "{horse_name(h)}" }
                                                                    div { class: "muted small ell", "{card_title(h.card_id)}" }
                                                                    div { class: "muted small", "{t.trainer}" }
                                                                }
                                                            }
                                                            {setup::vc_body(h, &[])}
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
            if st.len() > SHOWN { p { class: "muted small", "Showing {SHOWN} of {st.len()}" } }
        }
    }
}

pub(super) fn batch_view(s: S, res: &Res, names: &[String], cur: &Arc<Batch>, base: Option<&Arc<Batch>>) -> Element {
    let mut focus = s.focus;
    let f = *focus.read();
    let diffs = base.map(|b| diff_batches(b, cur, &res.pairs));
    let n = cur.horses.len();
    let ed = &res.ed_mask;
    // With rivals the gates after the umas hold a different uma every race: they get their own table.
    let rivals = res.rivals.as_ref().map(|r| (r, rival_stats(r, cur)));
    let mut order: Vec<usize> = (0..rivals.as_ref().map_or(n, |r| r.0.from)).collect();
    order.sort_by(|&a, &b| cur.horses[a].place.mean.total_cmp(&cur.horses[b].place.mean));
    let improved = diffs.as_ref().is_some_and(|d| d.iter().zip(ed).any(|(d, &m)| m && d.as_ref().is_some_and(|d| d.win_rate.1 > d.win_rate.0 + 0.005)));
    rsx! {
        section { class: "card",
            div { class: "card-h",
                h3 { "{cur.seeds.len()} races" if base.is_some() { ": baseline → edited" } }
                span { class: "muted", "click an uma for details" }
                if improved { img { class: "great", src: "/a/great.webp", alt: "Great!" } }
            }
            if let Some((r, st)) = &rivals { {rival_kpis(r, cur, st)} } else { {team_wr(res, cur, base)} }
            div { class: "scroll-x",
                table { class: "grid results batch",
                    thead {
                        tr {
                            th { "Uma" }
                            th { "Style" }
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
                                let d = diffs.as_ref().and_then(|d| d[i].as_ref());
                                let card = res.input.horses[i].card_id;
                                rsx! {
                                    tr {
                                        key: "{i}",
                                        class: if f == Some(i) { "sel" },
                                        onclick: move |_| if f == Some(i) { focus.set(None) } else { focus_gate(s, i, card) },
                                        td { class: "who", {who(res, names, i)} }
                                        td { {style_badge(res.input.horses[i].running_style)} }
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
                                        td { class: "num", {hp_cell(h, res.cv.distance)} }
                                        for e in EVENTS.iter() {
                                            {
                                                let (kind, more_good) = (e.kind, e.more_good);
                                                let (rate, secs) = h.states[kind as usize];
                                                rsx! {
                                                    td { class: "num",
                                                        if rate > 0.0 { b { "{pct(rate)}" } } else { span { class: "muted", "-" } }
                                                        if let Some(b) = base.zip(res.base_of(i)).map(|(b, j)| &b.horses[j]) {
                                                            {
                                                                let (was, was_s) = b.states[kind as usize];
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
        if let Some((r, st)) = &rivals { {rival_table(s, r, st)} }
        if let Some(i) = f.filter(|&i| i < n) {
            {horse_detail(s, res, i, cur, base, diffs.as_ref().and_then(|d| d[i].as_ref()))}
        }
    }
}
