//! Custom window chrome and app header: the window is undecorated (main.rs, chrome_win.rs), so this 40px strip
//! holds the logo, the race context (also the drag area), Races, theme, options, the window buttons, and the
//! invisible resize edges.

use super::*;
use dioxus::desktop::tao::event::{Event, WindowEvent};
use dioxus::desktop::tao::window::ResizeDirection;
use dioxus::desktop::{use_window, use_wry_event_handler, window};
use dioxus::html::input_data::MouseButton;
use std::time::Instant;

/// Mouse-downs this close on the drag area count as a double-click (`drag()` swallows the native dblclick).
const DOUBLE_CLICK: Duration = Duration::from_millis(400);

#[component]
pub fn TitleBar() -> Element {
    let s = use_context::<S>();
    let w = use_window();
    let mut maximized = use_signal(|| w.is_maximized());
    let mut last_down = use_signal(|| None::<Instant>);
    // Window bounds to remember: the normal size and position, kept while maximized or minimized.
    let saved = use_hook(load_window);
    let mut size = use_signal(|| saved);
    // Track maximize however it changes (buttons, Win+Up, snapping), and the bounds.
    use_wry_event_handler(move |event, _| {
        if let Event::WindowEvent { event: WindowEvent::Resized(_) | WindowEvent::Moved(_), .. } = event {
            #[cfg(windows)]
            let (max, min) = {
                use dioxus::desktop::tao::platform::windows::WindowExtWindows;
                crate::chrome_win::state(w.hwnd())
            };
            #[cfg(not(windows))]
            let (max, min) = (w.is_maximized(), w.is_minimized());
            if *maximized.peek() != max {
                maximized.toggle();
            }
            let cur = *size.peek();
            let (width, height, x, y) = if max || min {
                cur.map_or((0.0, 0.0, None, None), |c| (c.width, c.height, c.x, c.y))
            } else {
                let (width, height) = w.inner_size().to_logical::<f64>(w.scale_factor()).into();
                let p = w.outer_position().ok().map(|p| p.to_logical::<f64>(w.scale_factor()));
                (width, height, p.map(|p| p.x), p.map(|p| p.y))
            };
            if width > 0.0 {
                // Minimized keeps whether it was maximized, so it comes back the way it was.
                let maximized = if min { cur.is_some_and(|c| c.maximized) } else { max };
                size.set(Some(WindowSize { width, height, maximized, x, y }));
            }
        }
    });
    // Saved a moment after it settles, not on every step of a drag.
    use_future(move || async move {
        let mut last = saved;
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let cur = *size.peek();
            if cur != last {
                if let Some(c) = &cur {
                    save_window(c);
                }
                last = cur;
            }
        }
    });
    // A newer GitHub release, checked once per launch (also clears out the exe the last update replaced).
    let mut update = use_signal(|| None::<String>);
    let mut updating = use_signal(|| false);
    use_future(move || async move {
        let h = std::thread::spawn(|| {
            let v = newer_release();
            clean_old_exe();
            v
        });
        while !h.is_finished() {
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        update.set(h.join().ok().flatten());
    });
    let mut s2 = s;
    let mut start_update = move |v: String| {
        updating.set(true);
        spawn(async move {
            let h = std::thread::spawn(move || install_release(&v));
            while !h.is_finished() {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            match h.join().unwrap_or_else(|_| Err("panic".into())) {
                Ok(()) => window().close(),
                Err(e) => {
                    updating.set(false);
                    s2.err.set(Some(format!("Update failed ({e}). The installer is on {RELEASES_URL}")));
                }
            }
        });
    };
    let ready = s.race.read().is_some();
    let mut options = use_signal(|| false);
    rsx! {
        header { class: "titlebar",
            img { class: "tb-logo", src: "/a/icon.png", alt: "Ulala", title: "Ulala: Urara never gives up!" }
            div {
                class: "titlebar-drag",
                onmousedown: move |e| {
                    if e.trigger_button() != Some(MouseButton::Primary) {
                        return;
                    }
                    let now = Instant::now();
                    if last_down().is_some_and(|t| now.duration_since(t) < DOUBLE_CLICK) {
                        last_down.set(None);
                        window().toggle_maximized();
                    } else {
                        last_down.set(Some(now));
                        window().drag();
                    }
                },
                if ready { setup::RaceContext {} } else { span { class: "tb-title", "Ulala" } }
            }
            if ready {
                button { class: "btn ghost tb-races", title: "Open a saved race (Ctrl+O)", onclick: move |_| open_saves(s), "Races" }
            }
            div { class: "tb-tools",
                if let Some(v) = update() {
                    span { class: "tb-ver old", title: "This version is out of date: v{v} is available", "v{VERSION}" }
                    button {
                        class: "btn small tb-update",
                        disabled: updating(),
                        onclick: move |_| start_update(v.clone()),
                        if updating() { "Updating…" } else { "Update to v{v}" }
                    }
                } else {
                    span { class: "tb-ver", title: "Ulala version (up to date)", "v{VERSION}" }
                }
                button {
                    class: "tb-btn",
                    title: "Toggle light / dark theme",
                    onclick: |_| {
                        document::eval(
                            "const r = document.documentElement; r.dataset.theme = r.dataset.theme === 'light' ? 'dark' : 'light'; localStorage.setItem('theme', r.dataset.theme);",
                        );
                    },
                    svg { class: "theme-ico", view_box: "0 0 24 24", circle { cx: "12", cy: "12", r: "8" } path { class: "fill", d: "M12 4a8 8 0 0 1 0 16z" } }
                }
                button { class: if options() { "tb-btn on" } else { "tb-btn" }, title: "Options", onclick: move |_| options.toggle(),
                    svg { class: "theme-ico", view_box: "-3 -3 30 30", style: "stroke-width: 2.5",
                        circle { cx: "12", cy: "12", r: "3" }
                        path { d: "M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" }
                    }
                }
            }
            div { class: "titlebar-buttons",
                button { class: "tb-btn", title: "Minimize", onclick: |_| window().set_minimized(true),
                    svg { view_box: "0 0 10 10", path { d: "M1 5.5h8" } }
                }
                button {
                    class: "tb-btn",
                    title: if maximized() { "Restore" } else { "Maximize" },
                    onclick: |_| window().toggle_maximized(),
                    if maximized() {
                        svg { view_box: "0 0 10 10", path { d: "M2.5 3.5h4v4h-4z M3.5 3.5V2.5h4v4h-1" } }
                    } else {
                        svg { view_box: "0 0 10 10", rect { x: "1.5", y: "1.5", width: "7", height: "7", rx: "1" } }
                    }
                }
                button { class: "tb-btn close", title: "Close", onclick: |_| window().close(),
                    svg { view_box: "0 0 10 10", path { d: "M2 2l6 6M8 2l-6 6" } }
                }
            }
        }
        if options() {
            div { class: "menu-catch", onclick: move |_| options.set(false) }
            Options {}
        }
        if !maximized() {
            div { class: "window-edge" }
            ResizeEdges {}
        }
    }
}

/// The options menu under the title bar's gear.
#[component]
fn Options() -> Element {
    let mut s = use_context::<S>();
    let cpu = *s.cpu.read();
    let all = threads_at(100);
    rsx! {
        div { class: "menu",
            h4 { "CPU for simulations" }
            div { class: "seg small",
                for pct in [25u8, 50, 75, 100] {
                    button {
                        class: if pct == cpu { "on" },
                        onclick: move |_| {
                            s.cpu.set(pct);
                            set_cpu_pref(pct);
                        },
                        "{pct}%"
                    }
                }
            }
            p { class: "muted", "{threads_at(cpu)} of {all} threads" }
        }
    }
}

/// Invisible handles on the window edges: the webview covers the whole client area, so the OS never sees the border
/// of an undecorated window.
#[component]
fn ResizeEdges() -> Element {
    let edges = [
        ("n", ResizeDirection::North),
        ("s", ResizeDirection::South),
        ("e", ResizeDirection::East),
        ("w", ResizeDirection::West),
        ("ne", ResizeDirection::NorthEast),
        ("nw", ResizeDirection::NorthWest),
        ("se", ResizeDirection::SouthEast),
        ("sw", ResizeDirection::SouthWest),
    ];
    rsx! {
        for (name, dir) in edges {
            div {
                key: "{name}",
                class: "resize-edge {name}",
                onmousedown: move |_| {
                    let _ = window().window.drag_resize_window(dir);
                },
            }
        }
    }
}
