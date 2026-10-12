//! Custom window chrome and app header: the window is undecorated (main.rs, chrome_win.rs), so this 40px strip
//! holds the logo, the race context (also the drag area), Races, theme, the window buttons, and the
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
    // Window size to remember: the restored size, kept while maximized.
    let saved = use_hook(load_window);
    let mut size = use_signal(|| saved);
    // Track maximize however it changes (buttons, Win+Up, snapping), and the size.
    use_wry_event_handler(move |event, _| {
        if let Event::WindowEvent { event: WindowEvent::Resized(_), .. } = event {
            let max = w.is_maximized();
            if *maximized.peek() != max {
                maximized.toggle();
            }
            let cur = *size.peek();
            let (width, height) = if max || w.is_minimized() {
                cur.map_or((0.0, 0.0), |c| (c.width, c.height))
            } else {
                w.inner_size().to_logical::<f64>(w.scale_factor()).into()
            };
            if width > 0.0 {
                size.set(Some(WindowSize { width, height, maximized: max }));
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
    let ready = s.race.read().is_some();
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
        if !maximized() {
            div { class: "window-edge" }
            ResizeEdges {}
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
