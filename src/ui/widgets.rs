//! Shared widgets and display helpers: icons, team colours, skill rows, measurement setup controls, aptitudes,
//! formatting.

use super::*;
use crate::charts::horse_color;
use std::path::Path;
use std::sync::OnceLock;

// ---------- icons ----------

/// assets/icons (chara/, card/, skill/): next to the sources in dev, else next to the exe.
fn icons_dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let dev = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/icons");
        let exe = std::env::current_exe().ok().and_then(|e| Some(e.parent()?.join("assets/icons")));
        if dev.is_dir() { dev } else { exe.unwrap_or(dev) }
    })
}

/// rank: letter ranks by `rank_label` index - 1 (G … US9), apt: aptitude letters by grade (1 = G), stat: Speed … Wit.
const ICON_KINDS: [&str; 6] = ["chara", "card", "skill", "rank", "apt", "stat"];

/// `<kind>/<digits>.png` with a known kind, else None (the handler serves nothing else from disk).
pub(super) fn icon_path(rel: &str) -> Option<PathBuf> {
    let (kind, file) = rel.split_once('/')?;
    let stem = file.strip_suffix(".png")?;
    let ok = ICON_KINDS.contains(&kind) && !stem.is_empty() && stem.bytes().all(|b| b.is_ascii_digit());
    ok.then(|| icons_dir().join(kind).join(file))
}

/// `<kind>/<file>` exists. The folders are listed once instead of a file check per icon per render.
fn has_icon(rel: &str) -> bool {
    static FILES: OnceLock<HashSet<String>> = OnceLock::new();
    FILES
        .get_or_init(|| {
            let list = |k: &str| std::fs::read_dir(icons_dir().join(k)).into_iter().flatten().flatten();
            ICON_KINDS.iter().flat_map(|&k| list(k).map(move |e| format!("{k}/{}", e.file_name().to_string_lossy()))).collect()
        })
        .contains(rel)
}

/// Costume title ("[Bestest Prize ♪]"), "" when unknown.
pub(super) fn card_title(card: i32) -> String {
    card_info(card).map(|c| c.title).unwrap_or_default()
}

/// Uma portrait: the costume icon (square, gold frame) when there is one, else the round chara icon, else the
/// initial. `ring` is the horse's identity colour.
pub(super) fn uma_icon(card: i32, chara: i32, ring: Option<&str>, size: &str) -> Element {
    let has_ring = ring.is_some();
    let ring = ring.map(|c| format!("--ring: {c}")).unwrap_or_default();
    let (card_rel, chara_rel) = (format!("card/{card}.png"), format!("chara/{chara}.png"));
    let name = chara_name(chara);
    let tip = format!("{name} {}", card_title(card));
    let (src, shape) = if has_icon(&card_rel) {
        (Some(card_rel), "costume")
    } else if has_icon(&chara_rel) {
        (Some(chara_rel), "")
    } else {
        (None, "")
    };
    let initial = name.chars().next().unwrap_or('?');
    let ringc = if has_ring { "ring" } else { "" };
    rsx! {
        span { class: "av {size} {shape} {ringc}", style: "{ring}", title: "{tip}",
            if let Some(src) = src { img { src: "/a/{src}", alt: "" } } else { "{initial}" }
        }
    }
}

pub(super) fn horse_icon(h: &HorseInput, ring: Option<&str>, size: &str) -> Element {
    uma_icon(h.card_id, h.chara_id, ring, size)
}

// ---------- teams & colours ----------

/// Hue families per team (team race): the player's team gets Urara pink, others blue, green, orange; teammates get
/// distinct shades of their team's hue so their lines stay apart.
const TEAM_HUES: [[&str; 6]; 4] = [
    ["#f0699b", "#b3245f", "#ff9ec4", "#e0457e", "#ffc2da", "#8f1c4c"],
    ["#2a78d6", "#62c3ef", "#17488f", "#8fb4ff", "#0e86ad", "#4a5fc8"],
    ["#14ac77", "#8ccf3a", "#0b6b4a", "#4fd1a5", "#5f8f1a", "#00897b"],
    ["#e07b28", "#f2b33d", "#a8501a", "#ffcf87", "#c96a00", "#ff9f5a"],
];

/// Team ids in display order (the player's first) when the field has more than one team, else empty.
fn teams(input: &RaceInput, player: Option<usize>) -> Vec<i32> {
    let mut ids: Vec<i32> = input.horses.iter().map(|h| h.team_id).filter(|&t| t != 0).collect();
    ids.sort();
    ids.dedup();
    if ids.len() < 2 {
        return vec![];
    }
    if let Some(pt) = player.and_then(|p| input.horses.get(p)).map(|h| h.team_id).filter(|&t| t != 0) {
        ids.retain(|&t| t != pt);
        ids.insert(0, pt);
    }
    ids
}

/// Per-horse colours: by team when there are teams, else by gate (`horse_color`).
pub(super) fn uma_colors(input: &RaceInput, player: Option<usize>) -> Vec<String> {
    let ts = teams(input, player);
    let mut seen = vec![0usize; ts.len()];
    input
        .horses
        .iter()
        .enumerate()
        .map(|(i, h)| match ts.iter().position(|&t| t == h.team_id) {
            Some(k) if k < TEAM_HUES.len() => {
                let c = TEAM_HUES[k][seen[k] % 6];
                seen[k] += 1;
                c.to_string()
            }
            _ => horse_color(i).to_string(),
        })
        .collect()
}

/// "Your team" / "Team 2" and the team's main colour, when the field has teams.
fn team_tag(input: &RaceInput, player: Option<usize>, i: usize) -> Option<(String, &'static str)> {
    let ts = teams(input, player);
    let t = input.horses.get(i)?.team_id;
    let k = ts.iter().position(|&x| x == t)?;
    let mine = player.and_then(|p| input.horses.get(p)).is_some_and(|h| h.team_id == t);
    Some((if mine { "Your team".to_string() } else { format!("Team {t}") }, TEAM_HUES[k.min(3)][0]))
}

/// The save owner's uma, while the field still lines up with the save.
pub(super) fn player_now(race: &Race, input: &RaceInput) -> Option<usize> {
    race.player.filter(|_| input.horses.len() == race.input.horses.len())
}

/// Team / "you" tags next to a name.
pub(super) fn team_tags(input: &RaceInput, player: Option<usize>, i: usize) -> Element {
    let tag = team_tag(input, player, i);
    rsx! {
        if let Some((label, c)) = tag { span { class: "team", style: "--team: {c}", "{label}" } }
        if player == Some(i) { span { class: "you", "You" } }
    }
}

/// Line under an uma's name: its trainer, else the costume title when the name repeats in the field.
pub(super) fn subline(horses: &[HorseInput], trainers: &[String], i: usize) -> Option<String> {
    let dup = horses.iter().filter(|h| h.chara_id == horses[i].chara_id).count() > 1;
    trainers.get(i).filter(|t| !t.is_empty()).cloned().or_else(|| dup.then(|| card_title(horses[i].card_id)))
}

fn rarity_class(r: i32) -> &'static str {
    match r {
        ..=1 => "r1",
        2 => "r2",
        _ => "r3",
    }
}


/// Game-style skill row: icon + name on a rarity tint, unique level on the right.
pub(super) fn skill_row(k: &SkillInfo, level: Option<i32>, extra: &str) -> Element {
    let rel = format!("skill/{}.png", k.icon_id);
    let icon = k.icon_id != 0 && has_icon(&rel);
    let inh = k.inherited;
    let tip = format!("{}{}: {}", k.name, if inh { " (inherited)" } else { "" }, k.description);
    rsx! {
        div { class: "skr {rarity_class(k.rarity)} {extra}", title: "{tip}",
            if icon { img { src: "/a/{rel}", alt: "" } } else { span { class: "ski" } }
            span { class: "skn", "{k.name}" }
            if inh { span { class: "inh", "inherited" } }
            if let Some(l) = level.filter(|_| k.rarity >= 3) { span { class: "lv", "Lv{l}" } }
        }
    }
}

/// A skill row that expands on click to its conditions, effects and timing. The click stops here, so it can sit in a
/// clickable row.
#[component]
pub(super) fn SkillX(id: i32, #[props(default)] level: Option<i32>, #[props(default, into)] extra: String) -> Element {
    let s = use_context::<S>();
    let mut open = use_signal(|| false);
    rsx! {
        div { class: if open() { "skx open" } else { "skx" },
            div {
                onclick: move |e| {
                    e.stop_propagation();
                    open.toggle();
                },
                {skill_row(&skill_info(id), level, &extra)}
            }
            if open() {
                {skill_facts(id, s.cview.read().as_ref().map(|c| c.distance))}
            }
        }
    }
}

/// A skill's description, conditions (order rates as places in a 9-uma field), effects, duration and cooldown
/// (also scaled to a `dist` m course), and its raw condition strings.
pub(super) fn skill_facts(id: i32, dist: Option<f32>) -> Element {
    let k = skill_info(id);
    let parts = skill_parts(id);
    let secs = |base: f32| match dist {
        Some(d) => format!("{base:.1}s base, {:.1}s here", base * d / 1000.0),
        None => format!("{base:.1}s base"),
    };
    let duration = |p: &SkillPart| match p.duration {
        d if d < 0.0 => "whole race".to_string(),
        0.0 => "instant".to_string(),
        d => format!("{}{}", secs(d), if p.duration_varies { " (varies)" } else { "" }),
    };
    rsx! {
        div { class: "skfacts",
            if !k.description.is_empty() { div { class: "skf-desc", "{k.description}" } }
            dl {
                dt { "Wit check" }
                dd { if wit_checked(id) { "Yes" } else { "No" } }
            }
            for (j, p) in parts.iter().enumerate() {
                if parts.len() > 1 { div { class: "skf-part", "Part {j + 1}" } }
                dl {
                    if !p.pre.is_empty() {
                        dt { "Pre" }
                        dd { for (g, t) in p.pre.iter().enumerate() { div { if g > 0 { span { class: "or", "or " } } "{t}" } } }
                    }
                    if !p.cond.is_empty() {
                        dt { "When" }
                        dd { for (g, t) in p.cond.iter().enumerate() { div { if g > 0 { span { class: "or", "or " } } "{t}" } } }
                    }
                    if !p.effects.is_empty() {
                        dt { "Effect" }
                        dd { for e in p.effects.iter() { div { "{e}" } } }
                        dt { "Duration" }
                        dd { "{duration(p)}" }
                    }
                    // 500 s = once a race.
                    if p.cooldown > 0.0 && p.cooldown < 500.0 {
                        dt { "Cooldown" }
                        dd { "{secs(p.cooldown)}" }
                    }
                }
                div { class: "skf-raw", "{p.raw}" }
            }
        }
    }
}

/// Small line icons (the text glyphs they replace were too small to read).
pub(super) fn icon(kind: &str) -> Element {
    let d = match kind {
        "dup" => "M8 8h11v11H8zM5 16V5h11",
        "dice" => "M5 5h14v14H5zM9 9h.01M15 15h.01M15 9h.01M9 15h.01M12 12h.01",
        "x" => "M6 6l12 12M18 6L6 18",
        // Race conditions (CourseCard). Ground: firm ground, then 1 to 3 drops for Good / Soft / Heavy.
        "firm" => "M3 16h18M6 16l-2 4M11 16l-2 4M16 16l-2 4M21 16l-2 4",
        "good" => "M3 19h18M12 5c-2 3-3 4.5-3 6.5a3 3 0 0 0 6 0c0-2-1-3.5-3-6.5z",
        "soft" => "M3 19h18M8 6c-1.5 2.2-2.5 3.5-2.5 5a2.5 2.5 0 0 0 5 0c0-1.5-1-2.8-2.5-5zM16 6c-1.5 2.2-2.5 3.5-2.5 5a2.5 2.5 0 0 0 5 0c0-1.5-1-2.8-2.5-5z",
        "heavy" => "M3 20h18M6 9c-1.2 1.8-2 2.8-2 4a2 2 0 0 0 4 0c0-1.2-.8-2.2-2-4zM12 4c-1.2 1.8-2 2.8-2 4a2 2 0 0 0 4 0c0-1.2-.8-2.2-2-4zM18 9c-1.2 1.8-2 2.8-2 4a2 2 0 0 0 4 0c0-1.2-.8-2.2-2-4z",
        "sun" => "M12 8a4 4 0 1 0 0 8a4 4 0 1 0 0-8M12 2v2M12 20v2M2 12h2M20 12h2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4",
        "cloud" => "M7 18h10a4 4 0 0 0 0-8a6 6 0 0 0-11.5 1.5A3.3 3.3 0 0 0 7 18z",
        "rain" => "M7 14h10a4 4 0 0 0 0-8a6 6 0 0 0-11.5 1.5A3.3 3.3 0 0 0 7 14zM8 17l-1 3M12 17l-1 3M16 17l-1 3",
        "snow" => "M12 2v20M3.3 7l17.4 10M3.3 17l17.4-10M9 3.5l3 2 3-2M9 20.5l3-2 3 2",
        "spring" => "M12 21v-9M12 12c0-4-3-7-8-7c0 4 3 7 8 7zM12 14c0-3 2.5-6 7-6c0 3.5-2.5 6-7 6z",
        "summer" => "M2 19c2-2 4-2 6 0s4 2 6 0 4-2 6 0M12 5a4 4 0 1 0 0 8a4 4 0 1 0 0-8",
        "fall" => "M5 19C5 10 11 4 20 4c0 9-6 15-15 15zM5 19l9-9",
        "winter" => "M12 3l-6 8h4l-5 6h14l-5-6h4zM12 17v4",
        "sakura" => "M12 3c2 2 2 5 0 7c-2-2-2-5 0-7zM20.5 9.2c-1 2.7-3.6 4-6.3 3.2c1-2.7 3.6-4 6.3-3.2zM17.3 19.5c-2.8-.4-4.4-2.9-3.9-5.6c2.8.4 4.4 2.9 3.9 5.6zM6.7 19.5c-.5-2.7 1.1-5.2 3.9-5.6c.5 2.7-1.1 5.2-3.9 5.6zM3.5 9.2c2.7-.8 5.3.5 6.3 3.2c-2.7.8-5.3-.5-6.3-3.2z",
        "morning" => "M3 19h18M7 19a5 5 0 0 1 10 0M12 3v6M9 6l3-3 3 3M4.9 11.9l1.4 1.4M19.1 11.9l-1.4 1.4",
        "evening" => "M3 19h18M7 19a5 5 0 0 1 10 0M12 3v6M9 6l3 3 3-3M4.9 11.9l1.4 1.4M19.1 11.9l-1.4 1.4",
        "night" => "M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5z",
        _ => "",
    };
    rsx! { svg { class: "ico", view_box: "0 0 24 24", path { d: "{d}" } } }
}

/// Sortable column header for `sort` = (column, descending): clicking the sorted column flips it, another column
/// starts descending (ascending when `asc_first`).
pub(super) fn sort_th(mut sort: Signal<(usize, bool)>, c: usize, label: &str, num: bool, asc_first: bool) -> Element {
    let (col, desc) = *sort.read();
    let arrow = if c == col { if desc { " ▼" } else { " ▲" } } else { "" };
    rsx! {
        th {
            class: if num { "num sort" } else { "sort" },
            onclick: move |_| sort.set((c, if c == col { !desc } else { !asc_first })),
            "{label}{arrow}"
        }
    }
}

// ---------- measurement setup (Skill gains, Uma ranking) ----------

/// "about 40 s" / "about 3 min".
pub(super) fn estimate(secs: f32) -> String {
    if secs < 60.0 { format!("about {:.0} s", secs.max(1.0)) } else { format!("about {:.0} min", (secs / 60.0).ceil()) }
}

/// Field icons to choose which uma is measured.
pub(super) fn who_pick(s: S, mut sel: Signal<usize>) -> Element {
    let input = s.input.read();
    let race = s.race.cloned();
    let cols = uma_colors(&input, race.as_deref().and_then(|r| player_now(r, &input)));
    let hi = (*sel.read()).min(input.horses.len().saturating_sub(1));
    let tip = |h: &HorseInput| format!("{} {}", chara_name(h.chara_id), card_title(h.card_id));
    let cur = input.horses.get(hi).map(|h| format!("{}, gate {}", tip(h), hi + 1));
    rsx! {
        div { class: "field",
            span { "Uma" }
            div { class: "who-pick",
                for (i, h) in input.horses.iter().enumerate() {
                    button { key: "{i}", class: if i == hi { "on" }, title: "{tip(h)}", onclick: move |_| sel.set(i),
                        {horse_icon(h, Some(&cols[i]), "md")}
                    }
                }
            }
            if let Some(c) = cur { b { "{c}" } }
        }
    }
}

/// NPCs / This field toggle (None = automatic: NPCs only for a one-uma field), plus the NPC lobby when NPCs run.
/// Returns the element, the mode and the field size measured in.
pub(super) fn mode_seg(s: S, mut sel: Signal<Option<bool>>, field: usize) -> (Element, bool, usize) {
    let mut lobby = s.npc_lobby;
    let npc = sel.read().unwrap_or(field <= 1);
    let size = if npc { FIELD_MIN } else { field.max(FIELD_MIN) };
    let li = *lobby.read();
    let (name, mix) = LOBBIES[li.min(LOBBIES.len() - 1)];
    let mix = ["Runaway", "Front", "Pace", "Late", "End"].iter().zip(mix).filter(|x| x.1 > 0).map(|(l, k)| format!("{k} {l}")).collect::<Vec<_>>().join(", ");
    let el = rsx! {
        div { class: "field",
            span { "Run" }
            div { class: "seg",
                button { class: if npc { "on" }, onclick: move |_| sel.set(Some(true)), "NPCs" }
                button { class: if !npc { "on" }, disabled: field <= 1, onclick: move |_| sel.set(Some(false)), "This field" }
            }
            small { class: "muted mode-note",
                if npc {
                    "With 8 NPCs, from gate 5: a real field for position conditions, without your field's noise."
                } else if field < FIELD_MIN {
                    "With the whole field, topped up to {FIELD_MIN} with NPCs: includes positioning and blocking knock-on effects; noisier."
                } else {
                    "With the whole field: includes positioning and blocking knock-on effects; noisier."
                }
            }
        }
        if size > field.max(1) || npc {
            div { class: "field lobby",
                span { "NPC lobby" }
                input {
                    r#type: "range",
                    min: 0,
                    max: LOBBIES.len() - 1,
                    value: "{li}",
                    oninput: move |e| lobby.set(e.value().parse().unwrap_or(2)),
                }
                div { class: "lobby-ends muted small", span { "Back-heavy" } span { "Front-heavy" } }
                small { b { "{name}" } " · {mix}" }
            }
        }
    };
    (el, npc, size)
}

/// Umas of `field` under an 80% full-spurt rate in a measurement: "Name 62%", NPCs "Name (NPC Front) 62%".
pub(super) fn low_spurts(field: &[HorseInput], rates: &[f32]) -> Vec<String> {
    let low = field.iter().zip(rates).filter(|x| *x.1 < 0.8);
    low.map(|(h, r)| {
        let npc = if h.viewer_id == 0 { format!(" (NPC {})", style_label(h.running_style)) } else { String::new() };
        format!("{}{npc} {:.0}%", chara_name(h.chara_id), r * 100.0)
    })
    .collect()
}

/// Warning for `low_spurts`' umas (nothing when empty).
pub(super) fn spurt_warn(low: &[String]) -> Element {
    if low.is_empty() {
        return rsx! {};
    }
    rsx! {
        div { class: "warn",
            "Low full-spurt rate: {low.join(\", \")}. Running out of stamina changes the pace and positions, which can skew these results."
        }
    }
}

/// Seed count input bound to `sig` (`id`: for the resync after an out-of-range entry).
pub(super) fn seeds_input(id: &'static str, mut sig: Signal<usize>, min: usize, max: usize) -> Element {
    let n = *sig.read();
    rsx! {
        input {
            id: "{id}",
            r#type: "number",
            min: "{min}",
            max: "{max}",
            step: 50,
            value: "{n}",
            title: "Number of seeds",
            oninput: move |e| {
                if let Ok(v) = e.value().parse::<usize>() {
                    sig.set(v.clamp(1, max));
                }
            },
            // `min` only on commit: clamping while typing would rewrite "3" to "10" before the "0" lands.
            onchange: move |_| {
                let v = (*sig.peek()).max(min);
                sig.set(v);
                resync(id, v)
            },
        }
    }
}

// ---------- aptitudes ----------

/// The aptitudes that matter for `h` on this course: (label, grade) for surface, distance and running style.
pub(super) fn key_apts(h: &HorseInput, cv: Option<&CourseView>) -> [(&'static str, i32); 3] {
    let (surf, dist) = cv
        .map(|c| ((c.info.surface - 1).clamp(0, 1) as usize, distance_type(c.info.distance)))
        .unwrap_or((0, 2));
    let st = (h.running_style - 1).clamp(0, 3) as usize;
    [
        (["Turf", "Dirt"][surf], h.proper_ground[surf]),
        (["Short", "Mile", "Medium", "Long"][dist], h.proper_distance[dist]),
        (STYLES[st + 1], h.proper_running_style[st]),
    ]
}

/// "Turf G, Medium G" for aptitudes of E or worse here (they flatten speed and skill gains), None when fine.
pub(super) fn poor_apts(h: &HorseInput, cv: Option<&CourseView>) -> Option<String> {
    let bad: Vec<String> = key_apts(h, cv).iter().filter(|a| a.1 <= 3).map(|a| format!("{} {}", a.0, grade(a.1))).collect();
    (!bad.is_empty()).then(|| bad.join(", "))
}

// ---------- formatting ----------

pub(super) fn ft(t: f32) -> String {
    let m = (t / 60.0).floor();
    format!("{}:{:05.2}", m, t - m * 60.0)
}
pub(super) fn pct(x: f32) -> String {
    format!("{:.1}%", x * 100.0)
}
pub(super) fn thousands(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}
pub(super) fn style_label(st: i32) -> &'static str {
    STYLES.get(st as usize).copied().unwrap_or("?")
}
pub(super) fn style_name(race: &Race, h: &HorseInput) -> String {
    let s = style_label(h.running_style);
    if runaway(race, h) { format!("{s} (Runaway)") } else { s.to_string() }
}
pub(super) fn mood_name(m: i32) -> &'static str {
    MOODS.get(m as usize).copied().unwrap_or("?")
}
pub(super) fn grade(g: i32) -> &'static str {
    GRADES.get(g as usize).copied().unwrap_or("?")
}
pub(super) fn date(d: &str) -> String {
    if d.len() == 8 { format!("{}-{}-{}", &d[..4], &d[4..6], &d[6..]) } else { d.to_string() }
}

/// `▲`/`▼` with good/bad colouring. `d` is the change; `lower_better` flips the colour. A change that rounds to
/// zero in `txt` shows as "±0".
pub(super) fn delta(d: f32, lower_better: bool, txt: String) -> Element {
    if txt.chars().filter(|c| c.is_ascii_digit()).all(|c| c == '0') {
        return rsx! { span { class: "d flat", "±0" } };
    }
    let good = (d < 0.0) == lower_better;
    let arrow = if d > 0.0 { "▲" } else { "▼" };
    rsx! { span { class: if good { "d good" } else { "d bad" }, "{arrow} {txt}" } }
}

/// Finish-time change, worded: "▼ 0.176s faster".
pub(super) fn time_delta(d: f32) -> Element {
    delta(d, true, format!("{:.3}s {}", d.abs(), if d < 0.0 { "faster" } else { "slower" }))
}
