//! Saved races: the Races modal and the background course index behind its search.

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

/// Opens the Races modal, picking up races saved since the list was read.
pub(super) fn open_saves(mut s: S) {
    refresh_saves(s);
    s.saves_open.set(true);
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
const CM_CATEGORY: &str = "Champions meeting";

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
    if !*s.saves_open.read() {
        return rsx! {};
    }
    let saves = s.saves.read();
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
    let cur = s.race.read().as_ref().and_then(|r| r.path.clone());
    let total = hits.len();
    let time = |t: &str| t.trim_end_matches('s').parse::<f32>().map(ft).unwrap_or_else(|_| t.to_string());
    let q = s.saves_q.cloned();
    rsx! {
        div { class: "scrim", onclick: move |_| s.saves_open.set(false) }
        div { class: "modal races",
            div { class: "drawer-h",
                h3 { "Races" }
                input {
                    r#type: "search",
                    placeholder: "Search by winner or course…",
                    autofocus: true,
                    value: "{q}",
                    oninput: move |e| s.saves_q.set(e.value()),
                }
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
            div { class: "races-sub muted small",
                if total > LIST_CAP { "Newest {LIST_CAP} of {thousands(total)} saves" } else { "{total} saves" }
                if indexed < saves.len() { " · reading courses: {thousands(indexed)} of {thousands(saves.len())}…" }
            }
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
                                                    b { "{e.winner}" }
                                                    if card != 0 { div { class: "muted small", "{card_title(card)}" } }
                                                }
                                            }
                                        }
                                        td { class: "num", "{time(&e.time)}" }
                                        td { class: "muted", "{m.map(|m| m.course.as_str()).unwrap_or(\"\")}" }
                                        td { class: "who",
                                            if let Some(m) = m.filter(|m| !m.team.is_empty()) {
                                                div { class: "whobox",
                                                    for &(card, chara) in &m.team { {uma_icon(card, chara, None, "sm")} }
                                                    div {
                                                        if let [(_, chara)] = m.team[..] { b { "{chara_name(chara)}" } }
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
