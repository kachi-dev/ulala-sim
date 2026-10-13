//! UI shell: app state, the split workspace, splash and toast. The rest lives in the submodules below.

mod chrome;
mod gains;
mod jobs;
mod ranking;
mod results;
mod saves;
mod session;
mod setup;
mod skills;
mod stamina;
mod widgets;
mod wit;

use jobs::*;
use saves::*;
use session::*;
use widgets::*;

use crate::engine::*;
use dioxus::core::current_scope_id;
use dioxus::desktop::{use_asset_handler, wry::http::Response};
use dioxus::html::HasFileData;
use dioxus::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;
use std::time::Duration;

pub const ICON: &[u8] = include_bytes!("../../assets/img/icon.png");
/// Embedded assets, served at `/a/<name>` (works with plain `cargo run`, no `dx` bundling).
const ASSETS: &[(&str, &str, &[u8])] = &[
    ("icon.png", "image/png", ICON),
    ("chibi.webp", "image/webp", include_bytes!("../../assets/img/chibi.webp")),
    ("great.webp", "image/webp", include_bytes!("../../assets/img/great.webp")),
    ("cheer.webp", "image/webp", include_bytes!("../../assets/img/cheer.webp")),
    ("nunito.woff2", "font/woff2", include_bytes!("../../assets/font/nunito.woff2")),
];

const MOODS: [&str; 6] = ["?", "Awful", "Bad", "Normal", "Good", "Great"];
const GRADES: [&str; 9] = ["?", "G", "F", "E", "D", "C", "B", "A", "S"];
const STATS: [&str; 5] = ["Speed", "Stamina", "Power", "Guts", "Wit"];
const STATS_SHORT: [&str; 5] = ["Spd", "Sta", "Pow", "Guts", "Wit"];
const MAX_FIELD: usize = 18;
/// Stat input range (the game's cap is 2000; values above 1200 are halved by the sim, as in the game).
const STAT_MAX: i32 = 2000;

/// Tabs of the right-hand (analysis) pane; setup is always on the left.
#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Results,
    Gains,
    Ranking,
}

/// Subtabs of an uma's detail in the results (a single run has only Summary and Skills).
#[derive(Clone, Copy, PartialEq)]
enum DetailTab {
    Summary,
    Skills,
    Stamina,
    Wit,
}

/// Single-run result tabs: the table, or every uma's positions and skill activations.
#[derive(Clone, Copy, PartialEq)]
enum RunTab {
    Summary,
    More,
}

#[derive(Clone)]
enum Kind {
    Single { cur: Arc<Run>, base: Option<Arc<Run>> },
    Batch { cur: Arc<Batch>, base: Option<Arc<Batch>> },
}

/// A finished Race-tab run, with what it was run on.
#[derive(Clone)]
struct Res {
    kind: Kind,
    input: Arc<RaceInput>,
    trainers: Arc<Vec<String>>,
    /// Per-horse colours (team hues or gate colours) and the save owner's horse, as of the run.
    colors: Arc<Vec<String>>,
    player: Option<usize>,
    /// Per horse: differs from the baseline (only meaningful when `base` is Some).
    ed_mask: Vec<bool>,
    cv: Arc<CourseView>,
    rev: u64,
}

#[derive(Clone)]
struct GainsRes {
    horse: usize,
    card: i32,
    chara: i32,
    items: Arc<Vec<SkillGain>>,
    cv: Arc<CourseView>,
    n: usize,
    /// Candidates asked for (fewer come back when cancelled).
    asked: usize,
    npc: bool,
    /// NPC lobby and field size measured in (for "Improve fidelity", which re-measures one skill in the same field).
    lobby: usize,
    size: usize,
    /// Umas of the measured field with a low full-spurt rate (`low_spurts`).
    low_spurt: Vec<String>,
    rev: u64,
}

#[derive(Clone)]
struct RankRes {
    items: Arc<Vec<UmaRank>>,
    cv: Arc<CourseView>,
    n: usize,
    npc: bool,
    low_spurt: Vec<String>,
    /// Combinations asked for (fewer come back when cancelled).
    asked: usize,
    rev: u64,
}

#[derive(Clone)]
struct StamRes {
    /// Field index in the batch it was measured on.
    horse: usize,
    st: Arc<Stamina>,
    /// The field it ran in (NPCs included).
    field: Arc<Vec<HorseInput>>,
    /// The same uma and seeds on the pinned baseline, when it differs where it matters.
    base: Option<Arc<Stamina>>,
    cv: Arc<CourseView>,
    rev: u64,
}

#[derive(Clone, PartialEq)]
struct WitRes {
    /// Field index in the batch it was measured on.
    horse: usize,
    /// Wit on the baseline and as set up.
    old: i32,
    new: i32,
    wit: Arc<Wit>,
    rev: u64,
}

#[derive(Clone)]
struct Job {
    label: String,
    /// None = indeterminate.
    progress: Option<f32>,
    /// The running job's `Work`, when it can be cancelled.
    cancel: Option<Arc<Work>>,
}

#[derive(Clone, Copy)]
struct S {
    /// App's scope: background tasks live here so they survive tab switches.
    scope: ScopeId,
    tab: Signal<Tab>,
    /// Narrow windows: the setup pane is a strip until expanded.
    left_open: Signal<bool>,
    saves: Signal<Vec<SaveEntry>>,
    /// Races (load) modal and its filters (kept while it is closed).
    saves_open: Signal<bool>,
    saves_cat: Signal<String>,
    /// CM number within the Champions meeting category; 0 = all.
    saves_cm: Signal<i32>,
    saves_q: Signal<String>,
    /// Course / player info per save, filled in by a background scan (`index_saves`).
    meta: Signal<HashMap<PathBuf, SaveMeta>>,
    race: Signal<Option<Arc<Race>>>,
    skills: Signal<Arc<Vec<SkillInfo>>>,
    courses: Signal<Arc<Vec<CourseInfo>>>,
    cview: Signal<Option<Arc<CourseView>>>,
    /// The setup being edited.
    input: Signal<RaceInput>,
    trainers: Signal<Vec<String>>,
    /// What diffs compare against: a save's original input, or a pinned custom setup.
    baseline: Signal<Option<Arc<RaceInput>>>,
    /// Bumped on every edit; results remember the rev they were run with (stale notice).
    rev: Signal<u64>,
    /// None = not checked (yet).
    verify: Signal<Option<Option<String>>>,
    res: Signal<Option<Res>>,
    /// The selected uma (field index): picked in the setup's field, followed by Results, Skill gains and Uma ranking.
    sel: Signal<usize>,
    /// Results: the run gate shown in detail (batch) or focused (single run); None = all. See `select`.
    focus: Signal<Option<usize>>,
    /// (horse, skill) open in the batch uma detail: fine activation view + its region on the pinned course.
    skill_sel: Signal<Option<(usize, i32)>>,
    /// Uma detail subtab (kept when another uma is shown).
    detail_tab: Signal<DetailTab>,
    run_tab: Signal<RunTab>,
    editing: Signal<Option<usize>>,
    /// The skill picker over the edit drawer (the card's "Add skill" opens both).
    skill_pick: Signal<bool>,
    picker: Signal<bool>,
    /// Field as compact rows (else cards).
    compact: Signal<bool>,
    job: Signal<Option<Job>>,
    err: Signal<Option<String>>,
    n: Signal<usize>,
    // Skill Gains tab
    g_filter: Signal<skills::SkillFilter>,
    g_owned: Signal<bool>,
    /// Some(true) = NPC field, Some(false) = this field, None = automatic (NPCs for a one-uma field).
    g_npc: Signal<Option<bool>>,
    /// NPC lobby (index into `LOBBIES`), shared by Skill gains and Uma ranking.
    npc_lobby: Signal<usize>,
    /// The setup field's size: umas it takes, and what NPCs fill up to. `FIELD_MIN`, or an opened save's field size.
    field_size: Signal<usize>,
    /// Editor skill picker filters (kept between openings).
    pick_filter: Signal<skills::SkillFilter>,
    // Uma ranking tab
    r_npc: Signal<Option<bool>>,
    r_styles: Signal<Vec<i32>>,
    r_q: Signal<String>,
    r_n: Signal<usize>,
    r_res: Signal<Option<RankRes>>,
    r_sort: Signal<(ranking::RankCol, bool)>,
    r_sel: Signal<Option<(i32, i32)>>,
    r_matrix: Signal<bool>,
    // Stamina subtab of the batch uma detail
    /// Seeds for the stamina calculation (its own count, not the batch's).
    st_n: Signal<usize>,
    st_res: Signal<Option<StamRes>>,
    /// (owner, skill) open in the debuffs table.
    st_dsel: Signal<Option<(usize, i32)>>,
    // Wit subtab of the batch uma detail
    /// Seeds for the Wit calculation.
    w_n: Signal<usize>,
    w_res: Signal<Option<WitRes>>,
    /// Mechanic row open in the table.
    w_sel: Signal<Option<usize>>,
    g_n: Signal<usize>,
    g_res: Signal<Option<GainsRes>>,
    g_sort: Signal<(gains::GainCol, bool)>,
    g_sel: Signal<Option<i32>>,
    /// Measured horse-races per second per worker thread (for the time estimate).
    g_rate: Signal<f32>,
    /// Options: share of logical processors for long jobs, percent (`cpu_pref`).
    cpu: Signal<u8>,
}

#[component]
pub fn App() -> Element {
    use_asset_handler("a", |req, resp| {
        let name = req.uri().path().trim_start_matches("/a/").to_string();
        let found = match ASSETS.iter().find(|a| a.0 == name) {
            Some(&(_, mime, body)) => Some((mime, body.to_vec())),
            None => icon_bytes(&name).map(|b| ("image/png", b.to_vec())),
        };
        let r = match found {
            Some((mime, body)) => {
                Response::builder().header("Content-Type", mime).header("Cache-Control", "max-age=86400").body(body)
            }
            None => Response::builder().status(404).body(Vec::new()),
        };
        resp.respond(r.unwrap());
    });
    let s = S {
        scope: current_scope_id(),
        tab: use_signal(|| Tab::Results),
        left_open: use_signal(|| false),
        saves: use_signal(|| sorted_saves(list_saves(&saved_races_dir()))),
        saves_open: use_signal(|| false),
        saves_cat: use_signal(String::new),
        saves_cm: use_signal(|| 0),
        saves_q: use_signal(String::new),
        meta: use_signal(HashMap::new),
        race: use_signal(|| None),
        skills: use_signal(Default::default),
        courses: use_signal(Default::default),
        cview: use_signal(|| None),
        input: use_signal(RaceInput::default),
        trainers: use_signal(Vec::new),
        baseline: use_signal(|| None),
        rev: use_signal(|| 0),
        verify: use_signal(|| None),
        res: use_signal(|| None),
        sel: use_signal(|| 0),
        focus: use_signal(|| None),
        skill_sel: use_signal(|| None),
        detail_tab: use_signal(|| DetailTab::Summary),
        run_tab: use_signal(|| RunTab::Summary),
        editing: use_signal(|| None),
        skill_pick: use_signal(|| false),
        picker: use_signal(|| false),
        compact: use_signal(compact_pref),
        job: use_signal(|| None),
        err: use_signal(|| None),
        n: use_signal(|| 1000),
        g_filter: use_signal(|| skills::SkillFilter { kinds: vec![Rarity::Gold], ..Default::default() }),
        g_owned: use_signal(|| false),
        g_npc: use_signal(|| None),
        npc_lobby: use_signal(|| 2),
        field_size: use_signal(|| FIELD_MIN),
        pick_filter: use_signal(Default::default),
        r_npc: use_signal(|| None),
        r_styles: use_signal(|| vec![1, 2, 3, 4]),
        r_q: use_signal(String::new),
        r_n: use_signal(|| 100),
        r_res: use_signal(|| None),
        r_sort: use_signal(|| (ranking::RankCol::Mean, true)),
        r_sel: use_signal(|| None),
        r_matrix: use_signal(|| true),
        st_n: use_signal(|| 500),
        st_res: use_signal(|| None),
        st_dsel: use_signal(|| None),
        w_n: use_signal(|| 500),
        w_res: use_signal(|| None),
        w_sel: use_signal(|| None),
        g_n: use_signal(|| 200),
        g_res: use_signal(|| None),
        g_sort: use_signal(|| (gains::GainCol::Mean, true)),
        g_sel: use_signal(|| None),
        g_rate: use_signal(|| 500.0),
        cpu: use_signal(cpu_pref),
    };
    use_context_provider(|| s);
    // An effect, not a hook: starting a job writes signals, which the component body must not.
    use_effect(move || {
        // The built-in index page has its own <title>, which a head tag can't override.
        document::eval("document.title = 'Ulala'");
        index_in_background(s, s.saves.peek().clone());
        restore(s)
    });
    // The session survives a restart: saved on every change of the race, setup or baseline (not before one is up).
    use_effect(move || {
        let race = s.race.read();
        let Some(race) = race.as_ref() else { return };
        save_session(race.path.as_deref(), &s.input.read(), &s.trainers.read(), s.baseline.read().as_deref(), *s.field_size.read());
    });
    // Ctrl+O opens the Races modal, Esc closes overlays (a page-level listener: focus may be anywhere, or nowhere).
    use_future(move || async move {
        let mut keys = document::eval(
            "document.addEventListener('keydown', e => { if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'o') { e.preventDefault(); dioxus.send(1); } else if (e.key === 'Escape') { dioxus.send(2); } }); await new Promise(() => {});",
        );
        let mut s = s;
        while let Ok(k) = keys.recv::<i32>().await {
            if k == 1 {
                open_saves(s);
            } else {
                // Esc closes whatever overlay is open.
                s.saves_open.set(false);
                s.picker.set(false);
                s.editing.set(None);
                s.g_sel.set(None);
                s.r_sel.set(None);
                s.left_open.set(false);
            }
        }
    });
    let ready = s.race.read().is_some();
    rsx! {
        div { class: "app",
            ondragover: |e| e.prevent_default(),
            ondrop: move |e| {
                e.prevent_default();
                if let Some(f) = e.files().first() {
                    open(s, f.path());
                }
            },
            if !ready {
                main { class: "main", Splash {} }
            } else {
                Workspace {}
            }
            setup::Drawer {}
            setup::Picker {}
            SavesModal {}
            NoHorseAct {}
            Toast {}
            chrome::TitleBar {}
        }
    }
}

/// Setup on the left, a draggable divider, analysis tabs on the right.
#[component]
fn Workspace() -> Element {
    let mut s = use_context::<S>();
    let tab = *s.tab.read();
    let open = *s.left_open.read();
    let stale = use_stale(s, move || s.res.read().as_ref().map(|r| r.rev));
    // Divider drag / double-click reset, done in the page: a Rust round trip per pointer move would lag.
    use_hook(|| {
        document::eval(SPLIT_JS);
    });
    let tab_btn = move |t: Tab, label: &'static str, dot: bool| {
        rsx! {
            button { class: if tab == t { "rtab on" } else { "rtab" }, onclick: move |_| go(s, t),
                "{label}"
                if dot { span { class: "dot", title: "The setup changed since the last run" } }
            }
        }
    };
    rsx! {
        div { class: if open { "split open" } else { "split" }, id: "split",
            div { class: "strip",
                button { class: "btn icon", title: "Show setup", onclick: move |_| s.left_open.set(true),
                    svg { class: "ico", view_box: "0 0 24 24", path { d: "M9 6l6 6-6 6" } }
                }
                span { class: "strip-label", "Setup" }
            }
            aside { class: "left", id: "pane-setup",
                button { class: "btn icon left-close", title: "Hide setup", onclick: move |_| s.left_open.set(false),
                    svg { class: "ico", view_box: "0 0 24 24", path { d: "M15 6l-6 6 6 6" } }
                }
                setup::SetupTab {}
            }
            div { class: "divider", title: "Drag to resize, double-click to reset" }
            section { class: "right", id: "right",
                setup::TopCourse {}
                div { class: "hdivider", title: "Drag to resize the course, double-click to reset" }
                nav { class: "rtabs",
                    {tab_btn(Tab::Results, "Results", stale())}
                    {tab_btn(Tab::Gains, "Skill gains", false)}
                    {tab_btn(Tab::Ranking, "Uma ranking", false)}
                }
                // Always mounted: each pane keeps its scroll position and state across tab switches.
                div { class: "pane", id: "pane-results", hidden: tab != Tab::Results, results::ResultsTab {} }
                div { class: "pane", id: "pane-gains", hidden: tab != Tab::Gains, gains::GainsTab {} }
                div { class: "pane", id: "pane-ranking", hidden: tab != Tab::Ranking, ranking::RankingTab {} }
            }
        }
    }
}

/// Split view divider: pointer drag sets `--left-w` (clamped, saved in localStorage), double-click resets.
const SPLIT_JS: &str = r#"
(() => {
  if (window.__split) return; window.__split = true;
  const MIN_L = 380, MIN_R = 460, KEY = 'leftW';
  const apply = w => { const sp = document.getElementById('split'); if (!sp) return;
    if (w == null) { sp.style.removeProperty('--left-w'); return; }
    const max = sp.clientWidth - MIN_R; sp.style.setProperty('--left-w', Math.max(MIN_L, Math.min(max, w)) + 'px'); };
  const saved = () => { try { const v = localStorage.getItem(KEY); return v ? +v : null; } catch { return null; } };
  const wait = () => document.getElementById('split') ? apply(saved()) : requestAnimationFrame(wait); wait();
  document.addEventListener('pointerdown', e => {
    const d = e.target.closest && e.target.closest('.divider'); if (!d) return;
    e.preventDefault(); d.setPointerCapture(e.pointerId); document.body.classList.add('resizing');
    const sp = document.getElementById('split'), x0 = sp.getBoundingClientRect().left;
    const move = ev => apply(ev.clientX - x0);
    const up = () => { d.removeEventListener('pointermove', move); d.removeEventListener('pointerup', up); document.body.classList.remove('resizing');
      const w = parseFloat(getComputedStyle(sp).getPropertyValue('--left-w')); try { if (w) localStorage.setItem(KEY, w); } catch {} };
    d.addEventListener('pointermove', move); d.addEventListener('pointerup', up);
  });
  document.addEventListener('dblclick', e => { if (e.target.closest && e.target.closest('.divider')) { try { localStorage.removeItem(KEY); } catch {} apply(null); } });
  // Course height above the analysis tabs: same idea, vertical.
  const MIN_T = 70, TKEY = 'topH';
  const applyT = h => { const r = document.getElementById('right'); if (!r) return;
    if (h == null) { r.style.removeProperty('--top-h'); return; }
    r.style.setProperty('--top-h', Math.max(MIN_T, Math.min(r.clientHeight * 0.6, h)) + 'px'); };
  const savedT = () => { try { const v = localStorage.getItem(TKEY); return v ? +v : null; } catch { return null; } };
  const waitT = () => document.getElementById('right') ? applyT(savedT()) : requestAnimationFrame(waitT); waitT();
  document.addEventListener('pointerdown', e => {
    const d = e.target.closest && e.target.closest('.hdivider'); if (!d) return;
    e.preventDefault(); d.setPointerCapture(e.pointerId); document.body.classList.add('resizing-v');
    const r = document.getElementById('right'), y0 = r.getBoundingClientRect().top;
    const move = ev => applyT(ev.clientY - y0);
    const up = () => { d.removeEventListener('pointermove', move); d.removeEventListener('pointerup', up); document.body.classList.remove('resizing-v');
      const h = parseFloat(getComputedStyle(r).getPropertyValue('--top-h')); try { if (h) localStorage.setItem(TKEY, h); } catch {} };
    d.addEventListener('pointermove', move); d.addEventListener('pointerup', up);
  });
  document.addEventListener('dblclick', e => { if (e.target.closest && e.target.closest('.hdivider')) { try { localStorage.removeItem(TKEY); } catch {} applyT(null); } });
  // Subtab switch: hold the pane's scroll bottom so a shorter tab can't pull the page up; the hold
  // follows the view back up as you scroll and goes once the real content reaches it.
  const bottom = p => p.scrollTop + p.clientHeight;
  document.addEventListener('click', e => { const b = e.target.closest && e.target.closest('.dtabs button'), p = b && b.closest('.pane');
    if (p) p.style.setProperty('--hold', bottom(p) - 1 + 'px'); }, true);
  document.addEventListener('scroll', e => { const p = e.target; if (!p.classList || !p.classList.contains('pane') || !p.style.getPropertyValue('--hold')) return;
    const last = p.lastElementChild, natural = last ? last.offsetTop + last.offsetHeight : 0;
    if (bottom(p) <= natural) p.style.removeProperty('--hold');
    else if (bottom(p) - 1 < parseFloat(p.style.getPropertyValue('--hold'))) p.style.setProperty('--hold', bottom(p) - 1 + 'px'); }, true);
})();
"#;

fn go(mut s: S, t: Tab) {
    if *s.tab.peek() != t {
        s.tab.set(t);
    }
}

/// Brings element `id` into view once it has rendered (a pane id scrolls that pane to its top).
fn reveal(id: &str) {
    document::eval(&format!(
        "setTimeout(() => {{ const e = document.getElementById('{id}'); if (!e) return; if (e.classList.contains('main')) e.scrollTo({{top: 0, behavior: 'smooth'}}); else e.scrollIntoView({{behavior: 'smooth', block: 'start'}}); }}, 80)"
    ));
}

// ---------- splash / toast ----------

#[component]
fn Splash() -> Element {
    rsx! {
        div { class: "empty",
            img { src: "/a/chibi.webp", alt: "Haru Urara" }
            h2 { "Warming up!" }
            p { "Stretching, lacing up the shoes, loading the game data…" }
            Progress {}
        }
    }
}

/// First launch only: the game or horseACT's output folder wasn't found (see `engine::setup`).
#[component]
fn NoHorseAct() -> Element {
    let mut open = use_signal(|| first_run() && setup() != Setup::Ok);
    if !*open.read() {
        return rsx! {};
    }
    rsx! {
        div { class: "scrim top", onclick: move |_| open.set(false) }
        div { class: "modal notice",
            SetupHint { heading: true }
            p { class: "muted small", "You can still open race files with Ctrl+O or by dropping them on the window." }
            button { class: "btn", onclick: move |_| open.set(false), "OK" }
        }
    }
}

#[component]
fn Toast() -> Element {
    let mut s = use_context::<S>();
    let Some(e) = s.err.cloned() else { return rsx! {} };
    rsx! {
        div { class: "toast", onclick: move |_| s.err.set(None),
            "{e}"
            span { class: "x", "×" }
        }
    }
}
