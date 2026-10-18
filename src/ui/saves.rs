//! Saved races and setups: the Races modal (horseACT's saved races, or the setups saved by name), the Save setup
//! dialog, and the background course index behind the races search.

use super::*;
use std::rc::Rc;

/// Fills `s.meta` for `saves` from `index_saves` on a worker thread, a batch every 100 ms (not a `job`: runs stay
/// available).
/// One run at a time: each rewrites the whole index cache, so a second would drop the first's entries. A skipped
/// refresh is picked up by the next one (`open_saves` asks for every save without meta).
pub(super) fn index_in_background(s: S, saves: Vec<SaveEntry>) {
    static RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if saves.is_empty() || RUNNING.swap(true, Relaxed) {
        return;
    }
    let (tx, rx) = std::sync::mpsc::channel::<(PathBuf, SaveMeta)>();
    let h = std::thread::spawn(move || {
        index_saves(&saves, |i, m| {
            let _ = tx.send((saves[i].path.clone(), m));
        })
    });
    bg(s, async move {
        let mut s = s;
        loop {
            let finished = h.is_finished();
            let batch: Vec<(PathBuf, SaveMeta)> = rx.try_iter().collect();
            if !batch.is_empty() {
                s.meta.write().extend(batch);
            }
            if finished {
                RUNNING.store(false, Relaxed);
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    });
}

/// Opens the Races modal, picking up races and setups saved since the lists were read.
pub(super) fn open_saves(mut s: S) {
    refresh_saves(s);
    s.setups.set(list_setups());
    s.saves_open.set(true);
}

/// What the save dialog does.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum SaveMode {
    /// Save under a name (the first save, or Save as…).
    New,
    /// Give the saved race being worked on a new name.
    Rename,
}

/// Saves the current setup as `name` (replacing a saved race of that name); `rename` = the file it was saved as, gone
/// once it's saved under the new name.
fn save_setup(mut s: S, name: &str, rename: Option<PathBuf>) {
    let Some(path) = setup_path(name) else { return };
    let source = s.race.peek().as_ref().and_then(|r| r.path.clone());
    let (input, trainers, baseline, npcs) = (s.input.peek(), s.trainers.peek(), s.baseline.peek(), s.npcs.peek());
    let cm = cm_preset_of(&input).map(|i| (cm_presets()[i].id, cm_presets()[i].name.clone()));
    let r = write_setup(&path, name, source.as_deref(), &input, &trainers, baseline.as_deref(), *s.field_size.peek(), &npcs, cm);
    drop((input, trainers, baseline, npcs));
    match r {
        Ok(()) => {
            if let Some(old) = rename.filter(|o| *o != path) {
                let _ = std::fs::remove_file(old);
            }
            s.saved_race.set(read_setup(&path));
            s.setups.set(list_setups());
            s.save_open.set(None);
        }
        Err(e) => s.err.set(Some(e)),
    }
}

/// Save (Ctrl+S): back into the saved race being worked on; else asks for a name.
pub(super) fn quick_save(mut s: S) {
    if s.race.peek().is_none() {
        return;
    }
    match s.saved_race.cloned() {
        Some(st) => save_setup(s, &st.name, None),
        None => s.save_open.set(Some(SaveMode::New)),
    }
}

/// The setup differs from saved race `st`: conditions, seed, umas, field size, NPCs or the pinned baseline.
fn race_edited(s: S, st: &SavedSetup) -> bool {
    let (input, base) = (s.input.read(), s.baseline.read());
    let base_differs = match (&st.baseline, base.as_deref()) {
        (Some(a), Some(b)) => differs(a, b),
        (a, b) => a.is_some() != b.is_some(),
    };
    differs(&st.input, &input) || st.field_size != *s.field_size.read() || st.npcs != *s.npcs.read() || base_differs
}

/// The save dialog: Save race / Save as / Rename, the current setup (course, conditions, CM preset, seed, umas, NPCs,
/// field size and the pinned baseline) under a name, listed in Load race > My races.
#[component]
pub(super) fn SaveSetupModal() -> Element {
    let mut s = use_context::<S>();
    let mut name = use_signal(String::new);
    // Each opening starts from the saved race's name, else the race's title.
    use_effect(move || {
        if s.save_open.read().is_some() {
            let race = s.race.peek().clone();
            let n = s.saved_race.peek().as_ref().map(|st| st.name.clone()).or_else(|| race.map(|r| race_title(&r, &s.input.peek())));
            name.set(n.unwrap_or_default());
        }
    });
    let Some(mode) = *s.save_open.read() else { return rsx! {} };
    let Some(race) = s.race.cloned() else { return rsx! {} };
    let input = s.input.read();
    let n = name.read().trim().to_string();
    let path = setup_path(&n);
    let renaming = s.saved_race.read().as_ref().filter(|_| mode == SaveMode::Rename).map(|st| st.path.clone());
    // Saving over another saved race (renaming to its own name isn't).
    let exists = path.as_ref().is_some_and(|p| p.exists() && renaming.as_ref() != Some(p));
    let (umas, size) = (input.horses.len(), *s.field_size.read());
    let (fill, rivals) = { let n = s.npcs.read(); (n.fill, n.rivals.on) };
    let npcs = if fill { size.saturating_sub(umas) } else { 0 };
    let field = if npcs > 0 { format!("{umas} umas + {npcs} {}", if rivals { "rivals" } else { "NPCs" }) } else { format!("{umas} umas") };
    let base = if s.baseline.read().is_some() { " · baseline pinned" } else { "" };
    let (title, course) = (race_title(&race, &input), course_label(input.course_set_id));
    let at = if title == course || course.is_empty() { title } else { format!("{title}, {course}") };
    let summary = format!("{at} · {field} · seed {}{base}", input.random_seed);
    let ok = path.is_some();
    let heading = match (mode, s.saved_race.read().is_some()) {
        (SaveMode::Rename, _) => "Rename race",
        (SaveMode::New, true) => "Save as",
        _ => "Save race",
    };
    let go = move || save_setup(s, &name.peek(), renaming.clone());
    let go2 = go.clone();
    rsx! {
        div { class: "scrim top", onclick: move |_| s.save_open.set(None) }
        div { class: "modal notice save-setup",
            h3 { "{heading}" }
            label { class: "field",
                span { "Name" }
                input {
                    id: "setup-name",
                    autofocus: true,
                    value: "{name}",
                    oninput: move |e| name.set(e.value()),
                    onkeydown: move |e| {
                        if e.key() == Key::Enter && setup_path(&name.peek()).is_some() {
                            go2();
                        }
                    },
                }
            }
            p { class: "small", "{summary}" }
            if exists {
                p { class: "warn", "A race named “{n}” is saved: saving replaces it." }
            }
            div { class: "btns",
                button { class: "btn ghost", onclick: move |_| s.save_open.set(None), "Cancel" }
                button {
                    class: "btn",
                    disabled: !ok,
                    onclick: move |_| go(),
                    if mode == SaveMode::Rename { "Rename" } else if exists { "Replace" } else { "Save" }
                }
            }
        }
    }
}

/// Top of the setup pane: which race this is (a saved race, a replay or a custom race), whether it changed since, and
/// Save / Save as / Rename / Load race.
#[component]
pub(super) fn RaceHeader() -> Element {
    let mut s = use_context::<S>();
    let mut menu = use_signal(|| false);
    let Some(race) = s.race.cloned() else { return rsx! {} };
    let saved = s.saved_race.cloned();
    let input = s.input.read();
    let cm = cm_preset_of(&input).map(|i| format!("CM {} {}", cm_presets()[i].id, cm_presets()[i].name));
    // (name, what it is, when, edited since)
    let (name, kind, when, edited) = match (&saved, &race.path) {
        (Some(st), _) => (st.name.clone(), "My race".to_string(), format!("saved {}", st.date), race_edited(s, st)),
        (None, Some(p)) => {
            let e = save_entry(p);
            let t = e.time.trim_end_matches('s').parse::<f32>().map(ft).unwrap_or_default();
            let name = if t.is_empty() { e.winner } else { format!("{}, {t}", e.winner) };
            (name, format!("Replay · {}", e.category), format!("raced {}", date(&race.date)), differs(&race.input, &input))
        }
        (None, None) => ("Custom race".to_string(), "Not saved".to_string(), String::new(), false),
    };
    let sub: Vec<String> = [Some(kind), cm, (!when.is_empty()).then_some(when)].into_iter().flatten().collect();
    let sub = sub.join(" · ");
    let can_rename = saved.is_some();
    rsx! {
        section { class: "card race-head",
            div { class: "rh-main",
                div { class: "rh-name",
                    b { title: "{name}", "{name}" }
                    if edited { span { class: "chip edited", title: "Changed since it was saved", "edited" } }
                }
                div { class: "muted small rh-sub", title: "{sub}", "{sub}" }
            }
            div { class: "rh-acts",
                div { class: "savebtn", onmouseleave: move |_| menu.set(false),
                    button { class: "btn", title: if can_rename { "Save over “{name}” (Ctrl+S)" } else { "Save under a name (Ctrl+S)" }, onclick: move |_| quick_save(s), "Save" }
                    button { class: "btn caret", title: "Save as, rename", onclick: move |_| menu.toggle(), {icon("down")} }
                    if menu() {
                        div { class: "menu",
                            button { onclick: move |_| { menu.set(false); s.save_open.set(Some(SaveMode::New)); }, "Save as…" }
                            if can_rename {
                                button { onclick: move |_| { menu.set(false); s.save_open.set(Some(SaveMode::Rename)); }, "Rename…" }
                            }
                        }
                    }
                }
                button { class: "btn alt", title: "Load a saved race or a replay (Ctrl+O)", onclick: move |_| open_saves(s), "Load race" }
            }
        }
    }
}

/// Save uma: the uma at gate `s.uma_save` (stats, aptitudes, style, mood, skills, trainer name) under a name, listed
/// in the uma picker's My umas; tagged with the race's CM preset unless unticked. Always a new saved uma: names can
/// repeat.
#[component]
pub(super) fn SaveUmaModal() -> Element {
    let mut s = use_context::<S>();
    let mut name = use_signal(String::new);
    let mut tag = use_signal(|| true);
    // Each opening starts from the uma's name, CM tag on.
    use_effect(move || {
        if let Some(i) = *s.uma_save.read() {
            name.set(s.input.peek().horses.get(i).map_or(String::new(), |h| chara_name(h.chara_id)));
            tag.set(true);
        }
    });
    let Some(i) = *s.uma_save.read() else { return rsx! {} };
    let input = s.input.read();
    let Some(h) = input.horses.get(i).cloned() else { return rsx! {} };
    let trainer = s.trainers.read().get(i).cloned().unwrap_or_default();
    let cm = cm_preset_of(&input).map(|k| (cm_presets()[k].id, cm_presets()[k].name.clone()));
    drop(input);
    let n = name.read().trim().to_string();
    let ok = uma_path(&n, &h).is_some();
    // Exact copies of her already saved (same costume, stats, aptitudes and skills).
    let hash = uma_hash(&h);
    let mut names: Vec<String> = list_umas().into_iter().filter(|u| u.hash == hash).map(|u| u.name).collect();
    names.sort();
    // "“A”, “B” ×2": each name once, with how many copies when more than one.
    let mut dups: Vec<String> = vec![];
    for n in names.chunk_by(|a, b| a == b) {
        dups.push(if n.len() > 1 { format!("“{}” ×{}", n[0], n.len()) } else { format!("“{}”", n[0]) });
    }
    let st = stats(&h);
    let mut save = {
        let (h, trainer, cm) = (h.clone(), trainer.clone(), cm.clone());
        move || {
            let Some(path) = uma_path(&name.peek(), &h) else { return };
            match write_uma(&path, &name.peek(), &h, &trainer, cm.clone().filter(|_| tag())) {
                Ok(()) => s.uma_save.set(None),
                Err(e) => s.err.set(Some(e)),
            }
        }
    };
    let mut save2 = save.clone();
    rsx! {
        div { class: "scrim top", onclick: move |_| s.uma_save.set(None) }
        div { class: "modal notice save-setup",
            h3 { "Save uma" }
            div { class: "whobox",
                {horse_icon(&h, None, "md")}
                div {
                    b { "{chara_name(h.chara_id)}" }
                    div { class: "muted small", "{card_title(h.card_id)} · {st[0]} / {st[1]} / {st[2]} / {st[3]} / {st[4]} · {h.skills.len()} skills" }
                }
            }
            label { class: "field",
                span { "Name" }
                input {
                    id: "uma-name",
                    autofocus: true,
                    value: "{name}",
                    oninput: move |e| name.set(e.value()),
                    onkeydown: move |e| {
                        if e.key() == Key::Enter && !name.peek().trim().is_empty() {
                            save2();
                        }
                    },
                }
            }
            if let Some((id, cname)) = &cm {
                label { class: "check",
                    input { r#type: "checkbox", checked: tag(), onchange: move |_| tag.toggle() }
                    "Made for CM {id} - {cname}"
                }
            }
            if !dups.is_empty() {
                p { class: "warn", "Already saved, exactly the same: {dups.join(\", \")}." }
            }
            div { class: "btns",
                button { class: "btn ghost", onclick: move |_| s.uma_save.set(None), "Cancel" }
                button { class: "btn", disabled: !ok, onclick: move |_| save(), "Save" }
            }
        }
    }
}

/// What the search matches in a saved race: its name, course, CM and uma names.
fn setup_text(st: &SavedSetup, course: &str) -> String {
    let umas: Vec<String> = st.input.horses.iter().map(horse_name).collect();
    let cm = st.cm.as_ref().map_or(String::new(), |(id, name)| format!("CM {id} {name}"));
    format!("{} {course} {cm} {}", st.name, umas.join(" ")).to_lowercase()
}

/// Re-reads the saves folder.
// ponytail: folder scan on the UI thread, a short hitch with thousands of saves; move to a worker if it shows.
fn refresh_saves(mut s: S) {
    let fresh = sorted_saves(list_saves(&saved_races_dir()));
    let new: Vec<SaveEntry> = fresh.iter().filter(|e| !s.meta.peek().contains_key(&e.path)).cloned().collect();
    s.saves.set(fresh);
    index_in_background(s, new);
}

/// Why saves aren't found automatically, with the fix: set the game folder, or a link to install horseACT. Nothing
/// when all is well.
#[component]
pub(super) fn SetupHint(heading: bool) -> Element {
    let mut s = use_context::<S>();
    let mut st = use_signal(setup);
    let pick = move |_| {
        spawn(async move {
            let Some(f) = rfd::AsyncFileDialog::new().set_title("Umamusume: Pretty Derby folder").pick_folder().await else { return };
            match set_game_dir(f.path()) {
                Ok(new) => {
                    st.set(new);
                    refresh_saves(s);
                }
                Err(e) => s.err.set(Some(e)),
            }
        });
    };
    let set = rsx! { button { class: "btn alt small", onclick: pick, "Set game folder" } };
    match st() {
        Setup::Ok => rsx! {},
        Setup::NoGame => rsx! {
            div { class: "setup-hint",
                if heading { h3 { "Umamusume not found" } }
                p { "The game isn't in your Steam libraries, so horseACT's saved races and veteran lists can't be found. Set the folder with {GAME_EXE}." }
                {set}
            }
        },
        Setup::NoHorseAct => rsx! {
            div { class: "setup-hint",
                if heading { h3 { "horseACT not found" } }
                p {
                    "horseACT isn't installed in the detected game folder, so saved races and veteran lists can't be found. "
                    a { href: "https://github.com/ayaliz/horseACT#installation", "Install horseACT" }
                }
                if let Some(d) = game_dir() { p { class: "small", "Detected game directory: {d.display()}" } }
                {set}
            }
        },
    }
}

/// horseACT's folder for Champions Meeting saves.
pub(super) const CM_CATEGORY: &str = "Champions meeting";

/// Max rows rendered (search narrows it).
const LIST_CAP: usize = 300;

/// Newest first: by the date in the name, then by file time (saves from one day otherwise group by winner).
pub(super) fn sorted_saves(mut v: Vec<SaveEntry>) -> Vec<SaveEntry> {
    let mtime = |e: &SaveEntry| std::fs::metadata(&e.path).and_then(|m| m.modified()).ok();
    v.sort_by_cached_key(|e| std::cmp::Reverse((e.date.clone(), mtime(e))));
    v
}

#[component]
pub(super) fn SavesModal() -> Element {
    let mut s = use_context::<S>();
    // Winner name → a chara id for the icon (the file name only carries the name).
    let ids = use_hook(|| Rc::new(cards().iter().map(|c| (c.name.as_str(), c.chara_id)).collect::<HashMap<_, _>>()));
    let running = use_busy(s);
    // The setup whose Delete was clicked, asking to confirm.
    let mut del = use_signal(|| None::<PathBuf>);
    if !*s.saves_open.read() {
        return rsx! {};
    }
    let mine = *s.saves_setups.read();
    let saves = s.saves.read();
    let setups = s.setups.read();
    let mut cats: Vec<&str> = saves.iter().map(|e| e.category.as_str()).collect();
    cats.sort();
    cats.dedup();
    let (cat_v, ql) = (s.saves_cat.cloned(), s.saves_q.read().to_lowercase());
    let meta = s.meta.read();
    let cm_of = |e: &SaveEntry| meta.get(&e.path).filter(|_| e.category == CM_CATEGORY).and_then(|m| cm_of_save(m.course_id, &e.date));
    let cm_v = if cat_v == CM_CATEGORY { s.saves_cm.cloned() } else { 0 };
    // The CMs these saves are from, newest first.
    let cms: Vec<&CmPreset> =
        if cat_v == CM_CATEGORY { cm_presets().iter().filter(|p| saves.iter().any(|e| cm_of(e).is_some_and(|q| q.id == p.id))).collect() } else { vec![] };
    let indexed = saves.iter().filter(|e| meta.contains_key(&e.path)).count();
    let hits: Vec<&SaveEntry> = saves
        .iter()
        .filter(|e| cat_v.is_empty() || e.category == cat_v)
        .filter(|e| cm_v == 0 || cm_of(e).is_some_and(|p| p.id == cm_v))
        .filter(|e| e.winner.to_lowercase().contains(&ql) || meta.get(&e.path).is_some_and(|m| m.course.to_lowercase().contains(&ql)))
        .collect();
    let mine_hits: Vec<(&SavedSetup, String)> =
        setups.iter().map(|st| (st, course_label(st.input.course_set_id))).filter(|(st, c)| setup_text(st, c).contains(&ql)).collect();
    let n_mine = mine_hits.len();
    let cur = s.race.read().as_ref().and_then(|r| r.path.clone());
    let cur_setup = s.saved_race.read().as_ref().map(|st| st.path.clone());
    let total = hits.len();
    let time = |t: &str| t.trim_end_matches('s').parse::<f32>().map(ft).unwrap_or_else(|_| t.to_string());
    let q = s.saves_q.cloned();
    let npcs_of = |st: &SavedSetup| if st.npcs.fill && !st.npcs.rivals.on { st.field_size.saturating_sub(st.input.horses.len()) } else { 0 };
    rsx! {
        div { class: "scrim", onclick: move |_| s.saves_open.set(false) }
        div { class: "modal races",
            div { class: "drawer-h",
                h3 { "Load race" }
                div { class: "seg",
                    button { class: if !mine { "on" }, onclick: move |_| s.saves_setups.set(false), "Replays " span { class: "n", "{thousands(saves.len())}" } }
                    button { class: if mine { "on" }, onclick: move |_| s.saves_setups.set(true), "My races " span { class: "n", "{setups.len()}" } }
                }
                div { class: "grow" }
                label { class: "btn alt open",
                    "Open a file…"
                    input {
                        r#type: "file",
                        accept: ".json",
                        onchange: move |e| {
                            if let Some(f) = e.files().first() {
                                open(s, f.path());
                            }
                        },
                    }
                }
                button {
                    class: "btn ghost",
                    disabled: running(),
                    onclick: move |_| {
                        s.saves_open.set(false);
                        s.left_open.set(true);
                        new_custom(s);
                    },
                    "New custom race"
                }
                button { class: "btn icon", title: "Close (Esc)", onclick: move |_| s.saves_open.set(false), {icon("x")} }
            }
            div { class: "races-filters",
                input {
                    r#type: "search",
                    placeholder: if mine { "Search by name, course, CM or uma…" } else { "Search by winner or course…" },
                    autofocus: true,
                    value: "{q}",
                    oninput: move |e| s.saves_q.set(e.value()),
                }
                if !mine {
                    select {
                        onchange: move |e| {
                            s.saves_cat.set(e.value());
                            s.saves_cm.set(0);
                        },
                        option { value: "", "All categories" }
                        for c in cats {
                            option { value: "{c}", selected: c == cat_v, "{c}" }
                        }
                    }
                    if !cms.is_empty() {
                        select { onchange: move |e| s.saves_cm.set(e.value().parse().unwrap_or(0)),
                            option { value: "0", "All CMs" }
                            for p in cms {
                                option { value: "{p.id}", selected: p.id == cm_v, "CM {p.id} - {p.name}" }
                            }
                        }
                    }
                }
                div { class: "grow" }
                span { class: "muted small",
                    if mine {
                        "{n_mine} race" if n_mine != 1 { "s" }
                    } else {
                        if total > LIST_CAP { "Newest {LIST_CAP} of {thousands(total)} replays" } else { "{total} replays" }
                        if indexed < saves.len() { " · reading courses: {thousands(indexed)} of {thousands(saves.len())}…" }
                    }
                }
            }
            if mine {
                if setups.is_empty() {
                    p { class: "muted races-empty", "No saved races yet. Save Race (next to Load Race) keeps the current course, umas and NPCs under a name, to come back to here." }
                }
                div { class: "races-list", hidden: n_mine == 0,
                    table { class: "grid saves",
                        thead {
                            tr {
                                th { "Race" }
                                th { "Course" }
                                th { "Umas" }
                                th { class: "num", "NPCs" }
                                th { class: "num", "Seed" }
                                th { "Saved" }
                                th {}
                            }
                        }
                        tbody {
                            for (st, course) in mine_hits {
                                {
                                    let (p, open_st) = (st.path.clone(), st.clone());
                                    let asking = del.read().as_ref() == Some(&st.path);
                                    let n_npc = npcs_of(st);
                                    rsx! {
                                        tr {
                                            key: "{st.path.display()}",
                                            class: if cur_setup.as_ref() == Some(&st.path) { "sel" },
                                            title: "{st.path.display()}",
                                            onclick: move |_| open_setup(s, open_st.clone()),
                                            td {
                                                b { "{st.name}" }
                                                div { class: "muted small", if st.source.is_some() { "On a replay" } else { "Custom race" } }
                                            }
                                            td {
                                                span { class: "muted", "{course}" }
                                                if let Some((id, name)) = &st.cm { div { class: "muted small", "CM {id} - {name}" } }
                                            }
                                            td { class: "who",
                                                div { class: "whobox mobs",
                                                    for h in st.input.horses.iter().take(9) { {horse_icon(h, None, "sm")} }
                                                    if st.input.horses.len() > 9 { span { class: "muted small", "+{st.input.horses.len() - 9}" } }
                                                }
                                            }
                                            td { class: "num", if n_npc > 0 { "{n_npc}" } else { span { class: "muted", "-" } } }
                                            td { class: "num muted", "{st.input.random_seed}" }
                                            td { class: "muted", "{st.date}" }
                                            td { class: "acts",
                                                onclick: move |e| e.stop_propagation(),
                                                if asking {
                                                    span { class: "small", "Delete? " }
                                                    button {
                                                        class: "btn small danger",
                                                        onclick: {
                                                            let p = p.clone();
                                                            move |_| {
                                                                let _ = std::fs::remove_file(&p);
                                                                // Deleting the saved race being worked on: it's no longer one.
                                                                if s.saved_race.peek().as_ref().is_some_and(|st| st.path == p) {
                                                                    s.saved_race.set(None);
                                                                }
                                                                del.set(None);
                                                                s.setups.set(list_setups());
                                                            }
                                                        },
                                                        "Delete"
                                                    }
                                                    button { class: "btn small ghost", onclick: move |_| del.set(None), "Keep" }
                                                } else {
                                                    button { class: "btn icon", title: "Delete this saved race", onclick: move |_| del.set(Some(p.clone())), {icon("trash")} }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                SetupHint { heading: false }
                if total == 0 {
                    p { class: "muted races-empty", "No saved races here. You can still open any race file, or drop one on the window." }
                }
                div { class: "races-list",
                    table { class: "grid saves",
                        thead {
                            tr {
                                th { "Winner" }
                                th { class: "num", "Time" }
                                th { "Course" }
                                th { "Your uma" }
                                th { "Category" }
                                th { "Date" }
                            }
                        }
                        tbody {
                            for e in hits.into_iter().take(LIST_CAP) {
                                tr {
                                    key: "{e.path.display()}",
                                    class: if cur.as_ref() == Some(&e.path) { "sel" },
                                    title: "{e.path.display()}",
                                    onclick: {
                                        let p = e.path.clone();
                                        move |_| open(s, p.clone())
                                    },
                                    {
                                        let m = meta.get(&e.path);
                                        // Before the save is indexed: the chara from the winner's name in the file name.
                                        let (card, chara) = m.map(|m| m.winner).filter(|w| w.1 != 0).or_else(|| ids.get(e.winner.as_str()).map(|&id| (0, id))).unwrap_or_default();
                                        rsx! {
                                            td { class: "who",
                                                div { class: "whobox",
                                                    if chara != 0 { {uma_icon(card, chara, None, "sm")} }
                                                    div {
                                                        b { if e.winner.is_empty() && chara != 0 { "{chara_name(chara)}" } else { "{e.winner}" } }
                                                        if card != 0 { div { class: "muted small", "{card_title(card)}" } }
                                                    }
                                                }
                                            }
                                            td { class: "num", "{time(&e.time)}" }
                                            td { class: "muted", "{m.map(|m| m.course.as_str()).unwrap_or(\"\")}" }
                                            td { class: "who",
                                                if let Some(m) = m.filter(|m| !m.team.is_empty()) {
                                                    div { class: "whobox",
                                                        for t in &m.team { {uma_icon(t.card, t.chara, None, "sm")} }
                                                        div {
                                                            if let [t] = &m.team[..] { b { "{chara_name(t.chara)}" } }
                                                            if m.won { span { class: "you", "Won" } }
                                                            if !m.player_trainer.is_empty() { div { class: "muted small", "{m.player_trainer}" } }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    td {
                                        span { class: "cat", "{e.category}" }
                                        if let Some(p) = cm_of(e) { div { class: "muted small", "CM {p.id} - {p.name}" } }
                                    }
                                    td { class: "muted", "{e.date}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
