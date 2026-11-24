//! Окна: всплывающее окно выбора и главное окно.

use crate::state::AppState;
use crate::input;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

pub const PICKER: &str = "picker";
pub const MAIN: &str = "main";

const POPUP_W: f64 = 760.0;
const POPUP_H: f64 = 470.0;
const PANEL_W: f64 = 420.0;

pub fn picker(app: &AppHandle) -> Option<WebviewWindow> {
    if let Some(w) = app.get_webview_window(PICKER) {
        return Some(w);
    }
    let w = WebviewWindowBuilder::new(app, PICKER, WebviewUrl::App("index.html#view=picker".into()))
        .title("Clavis")
        .inner_size(POPUP_W, POPUP_H)
        .decorations(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .focused(true)
        .shadow(true)
        .background_color(tauri::window::Color(21, 21, 20, 255))
        .build()
        .ok()?;
    let handle = app.clone();
    w.on_window_event(move |e| {
        if let tauri::WindowEvent::Focused(false) = e {
            let state = handle.state::<AppState>();
            // Сразу после показа Windows может на мгновение отдать фокус обратно.
            if crate::vault::now() - state.picker_shown_at.load(Ordering::Relaxed) > 250 {
                if let Some(w) = handle.get_webview_window(PICKER) {
                    let _ = w.hide();
                }
            }
        }
    });
    Some(w)
}

pub fn main_window(app: &AppHandle) -> Option<WebviewWindow> {
    if let Some(w) = app.get_webview_window(MAIN) {
        return Some(w);
    }
    let w = WebviewWindowBuilder::new(app, MAIN, WebviewUrl::App("index.html#view=main".into()))
        .title("Clavis")
        .inner_size(1180.0, 760.0)
        .min_inner_size(820.0, 540.0)
        .decorations(false)
        .resizable(true)
        .visible(false)
        .center()
        .shadow(true)
        .background_color(tauri::window::Color(21, 21, 20, 255))
        .build()
        .ok()?;
    Some(w)
}

pub fn show_main(app: &AppHandle, section: Option<&str>) {
    let state = app.state::<AppState>();
    state.touch();
    if let Some(p) = app.get_webview_window(PICKER) {
        let _ = p.hide();
    }
    if let Some(w) = main_window(app) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        if let Some(s) = section {
            let _ = w.emit("main:section", s);
        }
    }
}

pub fn toggle_picker(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.stage() != "ready" {
        show_main(app, None);
        return;
    }
    let Some(w) = picker(app) else { return };
    if w.is_visible().unwrap_or(false) {
        let _ = w.hide();
        input::focus(state.prev_window.load(Ordering::SeqCst));
        return;
    }
    show_picker_for(app, None);
}

/// Показывает окно выбора. `snippet` открывает форму полей для этого сниппета.
pub fn show_picker_for(app: &AppHandle, snippet: Option<String>) {
    let state = app.state::<AppState>();
    state.touch();
    let Some(w) = picker(app) else { return };

    let fg = input::foreground();
    let own = [PICKER, MAIN].iter().filter_map(|l| app.get_webview_window(l)).any(|win| win.hwnd().map(|h| h.0 as isize == fg).unwrap_or(false));
    if !own {
        state.prev_window.store(fg, Ordering::SeqCst);
    }

    let mode = state.settings.read().mode.clone();
    let anchor = input::anchor();
    let monitor = app
        .available_monitors()
        .ok()
        .and_then(|ms| {
            ms.into_iter().find(|m| {
                let p = m.position();
                let s = m.size();
                anchor.x >= p.x && anchor.x < p.x + s.width as i32 && anchor.y >= p.y && anchor.y < p.y + s.height as i32
            })
        })
        .or_else(|| app.primary_monitor().ok().flatten());

    if let Some(m) = monitor {
        let scale = m.scale_factor();
        let wa = m.work_area();
        let (ax, ay, aw, ah) = (wa.position.x, wa.position.y, wa.size.width as i32, wa.size.height as i32);
        if mode == "panel" {
            let width = (PANEL_W * scale) as i32;
            let _ = w.set_size(PhysicalSize::new(width as u32, ah as u32));
            let _ = w.set_position(PhysicalPosition::new(ax + aw - width, ay));
        } else {
            let width = ((POPUP_W * scale) as i32).min(aw - 16);
            let height = ((POPUP_H * scale) as i32).min(ah - 16);
            let _ = w.set_size(PhysicalSize::new(width as u32, height as u32));
            let mut x = anchor.x - 24;
            let mut y = anchor.y;
            // Не даём окну уйти за край экрана; если снизу не помещается, показываем над курсором.
            if y + height > ay + ah {
                y = (anchor.y - height - 28).max(ay + 8);
            }
            x = x.clamp(ax + 8, ax + aw - width - 8);
            y = y.clamp(ay + 8, ay + ah - height - 8);
            let _ = w.set_position(PhysicalPosition::new(x, y));
        }
    } else {
        let _ = w.set_size(LogicalSize::new(POPUP_W, POPUP_H));
        let _ = w.center();
    }

    state.picker_shown_at.store(crate::vault::now(), Ordering::Relaxed);
    let _ = w.show();
    let _ = w.set_focus();
    let _ = w.emit("picker:shown", serde_json::json!({ "mode": mode, "snippet": snippet }));
}

pub fn hide_picker(app: &AppHandle, refocus: bool) {
    if let Some(w) = app.get_webview_window(PICKER) {
        let _ = w.hide();
    }
    if refocus {
        let state = app.state::<AppState>();
        input::focus(state.prev_window.load(Ordering::SeqCst));
    }
}

pub const QUICK: &str = "quick";

/// Маленькое окно быстрой заметки: записать мысль и вернуться к работе.
pub fn toggle_quick(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.stage() != "ready" {
        show_main(app, None);
        return;
    }
    if let Some(w) = app.get_webview_window(QUICK) {
        if w.is_visible().unwrap_or(false) {
            let _ = w.hide();
            input::focus(state.prev_window.load(Ordering::SeqCst));
            return;
        }
    }
    state.prev_window.store(input::foreground(), Ordering::SeqCst);
    let w = match app.get_webview_window(QUICK) {
        Some(w) => w,
        None => {
            let Ok(w) = WebviewWindowBuilder::new(app, QUICK, WebviewUrl::App("index.html#view=quick".into()))
                .title("Clavis")
                .inner_size(560.0, 220.0)
                .decorations(false)
                .resizable(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .visible(false)
                .shadow(true)
                .background_color(tauri::window::Color(21, 21, 20, 255))
                .build()
            else {
                return;
            };
            let handle = app.clone();
            w.on_window_event(move |e| {
                if let tauri::WindowEvent::Focused(false) = e {
                    if let Some(w) = handle.get_webview_window(QUICK) {
                        let _ = w.hide();
                    }
                }
            });
            w
        }
    };
    let _ = w.center();
    let _ = w.show();
    let _ = w.set_focus();
    let _ = w.emit("quick:shown", ());
}

pub fn hide_quick(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(QUICK) {
        let _ = w.hide();
    }
    input::focus(app.state::<AppState>().prev_window.load(Ordering::SeqCst));
}
