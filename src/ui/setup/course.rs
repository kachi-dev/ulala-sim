//! Course and conditions card.

use super::*;

const INOUT: [&str; 5] = ["", "", " (inner)", " (outer)", " (outer to inner)"];

fn course_label(c: &CourseInfo) -> String {
    format!("{}m{}", c.distance, INOUT.get(c.inout as usize).unwrap_or(&""))
}

fn change_course(s: S, race: &Race, id: i32) {
    let mut cv = s.cview;
    edit_race(s, |inp| set_course(race, inp, id));
    cv.set(Some(Arc::new(course_view(race, id))));
}

/// Closest course to `distance` among `pool`.
fn nearest<'a>(pool: impl Iterator<Item = &'a CourseInfo>, distance: i32) -> Option<i32> {
    pool.min_by_key(|c| (c.distance - distance).abs()).map(|c| c.id)
}

/// Icon buttons for a race-level condition: (value, name, `icon` kind), the name as the tooltip and under the buttons
/// for the picked one.
fn seg(s: S, what: &str, cur: i32, opts: &[(i32, &'static str, &'static str)], set: fn(&mut RaceInput, i32)) -> Element {
    let picked = opts.iter().find(|o| o.0 == cur).map_or("", |o| o.1);
    rsx! {
        div { class: "condgrp",
            div { class: "seg icons",
                for &(v, label, ico) in opts {
                    button { class: if v == cur { "on" }, title: "{what}: {label}", onclick: move |_| edit_race(s, |inp| set(inp, v)), {icon(ico)} }
                }
            }
            div { class: "muted small", "{picked}" }
        }
    }
}

#[component]
pub(super) fn CourseCard() -> Element {
    let s = use_context::<S>();
    let (Some(race), Some(cv)) = (s.race.cloned(), s.cview.cloned()) else { return rsx! {} };
    let courses = s.courses.cloned();
    let input = s.input.read();
    let cur = courses.iter().find(|c| c.id == input.course_set_id).cloned().unwrap_or_else(|| cv.info.clone());
    let mut tracks: Vec<&str> = courses.iter().map(|c| c.track.as_str()).collect();
    tracks.dedup();
    let surfaces: Vec<i32> = [1, 2].into_iter().filter(|&g| courses.iter().any(|c| c.track == cur.track && c.surface == g)).collect();
    let opts: Vec<CourseInfo> = courses.iter().filter(|c| c.track == cur.track && c.surface == cur.surface).cloned().collect();
    let (ground, weather, season, time, seed) = (input.ground_condition, input.weather, input.season, input.time, input.random_seed);
    let (r1, r2, r3, r4) = (race.clone(), race.clone(), race.clone(), race.clone());
    let (c1, c2) = (courses.clone(), courses.clone());
    let cur_track = cur.track.clone();
    let known = |p: &CmPreset| courses.iter().any(|c| c.id == p.course_set_id);
    let on_preset = cm_preset_of(&input);
    rsx! {
        details { class: "card fold", open: true,
            summary { class: "card-h",
                h3 { "Course and conditions" }
                div { class: "grow" }
                select {
                    title: "Load a Champions Meeting's course and conditions",
                    onchange: move |e| {
                        let Some(p) = e.value().parse().ok().and_then(|i: usize| cm_presets().get(i)) else { return };
                        edit_race(s, |inp| {
                            // Kept by set_course when the sim knows the CM race instance.
                            inp.race_instance_id = p.race_instance_id;
                            inp.race_type = 5; // Champions
                            (inp.ground_condition, inp.weather, inp.season, inp.time) = (p.ground, p.weather, p.season, p.time);
                        });
                        change_course(s, &r4, p.course_set_id);
                    },
                    option { value: "", selected: on_preset.is_none(), "CM preset" }
                    for (i, p) in cm_presets().iter().enumerate().filter(|(_, p)| known(p)) {
                        option { value: "{i}", selected: on_preset == Some(i), "CM {p.id} - {p.name}" }
                    }
                }
            }
            div { class: "conds",
                label { class: "field",
                    span { "Track" }
                    select {
                        onchange: move |e| {
                            let t = e.value();
                            let pool = c1.iter().filter(|c| c.track == t);
                            let same = pool.clone().filter(|c| c.surface == cur.surface);
                            if let Some(id) = nearest(same, cur.distance).or_else(|| nearest(pool, cur.distance)) {
                                change_course(s, &r1, id);
                            }
                        },
                        for t in tracks {
                            option { value: "{t}", selected: t == cur_track, "{t}" }
                        }
                    }
                }
                div { class: "field",
                    span { "Surface" }
                    div { class: "seg",
                        for g in surfaces {
                            button {
                                class: if g == cur.surface { "on" },
                                onclick: {
                                    let (c2, r2, track) = (c2.clone(), r2.clone(), cur_track.clone());
                                    move |_| {
                                        if let Some(id) = nearest(c2.iter().filter(|c| c.track == track && c.surface == g), cur.distance) {
                                            change_course(s, &r2, id);
                                        }
                                    }
                                },
                                if g == 1 { "Turf" } else { "Dirt" }
                            }
                        }
                    }
                }
                label { class: "field",
                    span { "Distance" }
                    select {
                        onchange: move |e| {
                            if let Ok(id) = e.value().parse() {
                                change_course(s, &r3, id);
                            }
                        },
                        for c in opts {
                            option { value: "{c.id}", selected: c.id == cur.id, "{course_label(&c)}" }
                        }
                    }
                }
                label { class: "field",
                    span { "Seed" }
                    div { class: "seedbox",
                        input {
                            id: "seed",
                            r#type: "number",
                            value: "{seed}",
                            oninput: move |e| {
                                if let Ok(v) = e.value().parse::<i32>() {
                                    edit_race(s, |inp| inp.random_seed = v);
                                }
                            },
                            // Out of range or empty: show the seed that will actually be used.
                            onchange: move |_| resync("seed", s.input.peek().random_seed),
                        }
                        button {
                            class: "btn icon",
                            title: "Random seed",
                            onclick: move |_| {
                                let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
                                edit_race(s, |inp| inp.random_seed = (t.as_nanos() % 2_000_000_000) as i32);
                            },
                            {icon("dice")}
                        }
                    }
                }
                // A div: a label would forward caption clicks to the first button.
                div { class: "field",
                    span { "Ground · Weather · Season · Time of day" }
                    div { class: "condrow",
                        {seg(s, "Ground", ground, &[(1, "Firm", "firm"), (2, "Good", "good"), (3, "Soft", "soft"), (4, "Heavy", "heavy")], |i, v| i.ground_condition = v)}
                        {seg(s, "Weather", weather, &[(1, "Sunny", "sun"), (2, "Cloudy", "cloud"), (3, "Rainy", "rain"), (4, "Snowy", "snow")], |i, v| i.weather = v)}
                        {seg(s, "Season", season, &[(1, "Spring", "spring"), (2, "Summer", "summer"), (3, "Fall", "fall"), (4, "Winter", "winter"), (5, "Sakura", "sakura")], |i, v| i.season = v)}
                        {seg(s, "Time of day", time, &[(1, "Morning", "morning"), (2, "Day", "sun"), (3, "Evening", "evening"), (4, "Night", "night")], |i, v| i.time = v)}
                    }
                }
            }
        }
    }
}
