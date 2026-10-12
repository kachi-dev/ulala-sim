//! Background jobs: one at a time, on a worker thread, with progress (and Cancel) in `s.job`.

use super::*;
use dioxus::core::Runtime;
use std::time::Instant;

/// Marks the app busy right away (before the task first runs), so a double click can't start two jobs.
pub(super) fn claim(mut s: S) -> bool {
    if s.job.peek().is_some() {
        return false;
    }
    s.job.set(Some(Job { label: "Starting".into(), progress: None, cancel: None }));
    s.err.set(None); // a new job supersedes the last one's error
    true
}

/// Runs `f` on a worker thread, publishing progress (`done / total`, 0 = indeterminate) to `s.job` until it ends.
/// `cancellable` shows a Cancel button that sets `cancel` of the `Work` passed to `f`. Returns the result and whether
/// it was cancelled.
pub(super) async fn job<T: Send + 'static>(mut s: S, label: &str, total: usize, cancellable: bool, f: impl FnOnce(&Work) -> T + Send + 'static) -> Result<(T, bool), String> {
    let w = Arc::new(Work::default());
    let h = std::thread::spawn({
        let w = w.clone();
        move || f(&w)
    });
    let state = |n: usize| Job {
        label: label.to_string(),
        progress: (total > 0).then(|| (n as f32 / total as f32).min(1.0)),
        cancel: cancellable.then(|| w.clone()),
    };
    s.job.set(Some(state(0)));
    while !h.is_finished() {
        tokio::time::sleep(Duration::from_millis(50)).await;
        s.job.set(Some(state(w.done.load(Relaxed))));
    }
    s.job.set(None);
    let cancelled = w.cancel.load(Relaxed);
    h.join().map(|v| (v, cancelled)).map_err(|e| {
        let msg = e.downcast_ref::<&str>().map(|m| m.to_string()).or_else(|| e.downcast_ref::<String>().cloned());
        format!("{label} failed: {}", msg.unwrap_or_else(|| "panic".into()))
    })
}

/// Spawns on the App scope (a task spawned from a tab would die when the tab unmounts).
pub(super) fn bg(s: S, fut: impl Future<Output = ()> + 'static) {
    Runtime::current().spawn(s.scope, fut);
}

/// A job is running. Memoised: `job` rewrites `s.job` 20 times a second, this only flips twice.
pub(super) fn use_busy(s: S) -> Memo<bool> {
    use_memo(move || s.job.read().is_some())
}

/// Results (run at setup `rev()`) are older than the setup. Memoised, so readers don't re-render on every edit.
pub(super) fn use_stale(s: S, rev: impl Fn() -> Option<u64> + 'static) -> Memo<bool> {
    use_memo(move || rev().is_some_and(|r| r != *s.rev.read()))
}

/// Skill gains / Uma ranking: claims the app, runs `f` as a cancellable job and hands its result to `done`. `work`
/// (in horse-races) over the time taken refines `g_rate` for the next estimate.
pub(super) fn measure<T: Send + 'static>(
    s: S,
    label: String,
    total: usize,
    work: usize,
    f: impl FnOnce(&Work) -> T + Send + 'static,
    done: impl FnOnce(S, T) + 'static,
) {
    if !claim(s) {
        return;
    }
    bg(s, async move {
        let mut s = s;
        let t0 = Instant::now();
        match job(s, &label, total, true, f).await {
            Ok((v, cancelled)) => {
                let secs = t0.elapsed().as_secs_f32();
                if !cancelled && secs > 0.5 {
                    s.g_rate.set(work as f32 / secs);
                }
                done(s, v);
            }
            Err(e) => s.err.set(Some(e)),
        }
    });
}

#[component]
pub(super) fn Progress() -> Element {
    let s = use_context::<S>();
    let Some(j) = s.job.cloned() else { return rsx! {} };
    rsx! {
        div { class: "progress",
            img { src: "/a/cheer.webp", alt: "" }
            div {
                div { class: "plabel",
                    "{j.label}…"
                    if let Some(p) = j.progress { b { " {p * 100.0:.0}%" } }
                }
                div { class: if j.progress.is_some() { "bar" } else { "bar indet" },
                    div { style: "width: {j.progress.unwrap_or(1.0) * 100.0}%" }
                }
            }
            if let Some(w) = j.cancel {
                button {
                    class: "btn ghost",
                    disabled: w.cancel.load(Relaxed),
                    onclick: move |_| w.cancel.store(true, Relaxed),
                    if w.cancel.load(Relaxed) { "Stopping…" } else { "Cancel" }
                }
            }
        }
    }
}
