//! Windows: borderless window that keeps the DWM drop shadow but not the 1px accent-coloured frame (the technique
//! Chromium uses for frameless windows). The whole window becomes client area (WM_NCCALCSIZE → 0) and the DWM frame
//! is extended 1px into it so DWM still draws the shadow; resizing is done by the CSS edge handles (ui/chrome.rs).

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::DwmExtendFrameIntoClientArea;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, IsZoomed, SIZE_RESTORED, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE,
    SWP_NOZORDER, SendMessageW, SetWindowPos, WM_NCCALCSIZE, WM_SIZE,
};

const SUBCLASS_ID: usize = 0x756c_616c; // "ulal"

pub fn apply(hwnd: isize) {
    let hwnd = HWND(hwnd as *mut _);
    // SAFETY: `hwnd` is the live top-level window owned by tao on this (UI) thread. The subclass proc is a plain fn
    // with no captured state. It is never removed (docs ask for RemoveWindowSubclass before destruction), which is
    // fine here: this is the app's only window and the process ends with it.
    unsafe {
        if !SetWindowSubclass(hwnd, Some(subclass), SUBCLASS_ID, 0).as_bool() {
            return;
        }
        let margins = MARGINS { cxLeftWidth: 1, cxRightWidth: 1, cyTopHeight: 1, cyBottomHeight: 1 };
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);
        // Re-run WM_NCCALCSIZE through the new subclass.
        let flags = SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOOWNERZORDER | SWP_NOACTIVATE;
        let _ = SetWindowPos(hwnd, None, 0, 0, 0, 0, flags);
        // The client area just grew by the old frame, but Windows sends no WM_SIZE when only the frame changes, so the
        // webview would keep its old bounds (black strips right/bottom).
        let mut rc = RECT::default();
        if GetClientRect(hwnd, &mut rc).is_ok() {
            let (w, h) = ((rc.right - rc.left) as u32 & 0xffff, (rc.bottom - rc.top) as u32 & 0xffff);
            SendMessageW(hwnd, WM_SIZE, Some(WPARAM(SIZE_RESTORED as usize)), Some(LPARAM((w | (h << 16)) as isize)));
        }
    }
}

unsafe extern "system" fn subclass(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, _data: usize) -> LRESULT {
    // Whole window = client area. Maximized windows go to tao, which clamps them to the work area.
    // SAFETY: called by comctl32 with valid message arguments for `hwnd`.
    unsafe {
        if msg == WM_NCCALCSIZE && wparam.0 != 0 && !IsZoomed(hwnd).as_bool() {
            return LRESULT(0);
        }
        DefSubclassProc(hwnd, msg, wparam, lparam)
    }
}
