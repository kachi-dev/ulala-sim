// No console window behind the app in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod charts;
#[cfg(windows)]
mod chrome_win;
mod engine;
mod ui;

use dioxus::desktop::{Config, LogicalPosition, LogicalSize, WindowBuilder, icon_from_memory, tao::window::Icon};

fn main() {
    let saved = engine::load_window();
    let size = saved.map_or(LogicalSize::new(1480.0, 940.0), |w| LogicalSize::new(w.width, w.height));
    let pos = saved.and_then(|w| w.x.zip(w.y));
    let window = WindowBuilder::new()
        .with_title("Ulala")
        .with_inner_size(size)
        // Always explicit: debug builds of dioxus otherwise put the window back where the last run left it, which is
        // off-screen when that run closed minimized.
        .with_position(pos.map_or(LogicalPosition::new(0.0, 0.0), |(x, y)| LogicalPosition::new(x, y)))
        .with_min_inner_size(LogicalSize::new(960.0, 600.0))
        .with_window_icon(icon_from_memory::<Icon>(ui::ICON).ok())
        // Custom title bar (ui/chrome.rs); the shadow without the accent-coloured Win10 frame is chrome_win's.
        .with_decorations(false);
    #[cfg(windows)]
    let window = {
        use dioxus::desktop::tao::platform::windows::WindowBuilderExtWindows;
        window.with_undecorated_shadow(true)
    };
    let cfg = Config::new()
        .with_window(window)
        .with_menu(None)
        .with_background_color((27, 20, 34, 255))
        .with_on_window(move |window, _| {
            #[cfg(windows)]
            {
                use dioxus::desktop::tao::platform::windows::WindowExtWindows;
                chrome_win::apply(window.hwnd());
            }
            // Again now that the frame is client area, so the window comes back exactly as big as it was saved.
            window.set_inner_size(size);
            // No saved spot, or one no monitor shows any more (unplugged, resolution change): centered.
            let visible = window.outer_position().is_ok_and(|p| {
                window.available_monitors().any(|m| {
                    let (mp, ms) = (m.position(), m.size());
                    p.x >= mp.x - 100 && p.y >= mp.y - 10 && p.x < mp.x + ms.width as i32 - 100 && p.y < mp.y + ms.height as i32 - 60
                })
            });
            if pos.is_none() || !visible {
                if let Some(m) = window.current_monitor().or_else(|| window.primary_monitor()) {
                    let (mp, ms, ws) = (m.position(), m.size(), window.outer_size());
                    let x = mp.x + (ms.width as i32 - ws.width as i32).max(0) / 2;
                    let y = mp.y + (ms.height as i32 - ws.height as i32).max(0) / 2;
                    window.set_outer_position(dioxus::desktop::tao::dpi::PhysicalPosition::new(x, y));
                }
            }
            window.set_maximized(saved.is_some_and(|w| w.maximized));
        })
        .with_custom_head(format!(
            // Theme: dark unless the user picked light (persisted in the webview's localStorage).
            "<script>document.documentElement.dataset.theme = localStorage.getItem('theme') || 'dark'</script><style>{}</style>",
            include_str!("../assets/ui.css")
        ));
    dioxus::LaunchBuilder::desktop().with_cfg(cfg).launch(ui::App);
}
