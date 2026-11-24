mod clip;
mod commands;
mod crypto;
mod device;
mod expander;
mod extras;
mod input;
mod license;
mod pdf;
mod realtime;
mod settings;
mod shared;
mod state;
mod template;
mod ui;
mod vault;
mod winrt;

use state::AppState;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// Регистрирует сочетания окна Clavis и быстрой заметки. Ошибка первого — ошибка "hotkey", второго — "quick_hotkey".
pub(crate) fn register_hotkeys(app: &AppHandle, hotkey: &str, quick: &str) -> Result<(), String> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    gs.on_shortcut(hotkey, |app, _shortcut, event| {
        if event.state == ShortcutState::Pressed {
            ui::toggle_picker(app);
        }
    })
    .map_err(|_| "hotkey".to_string())?;
    if !quick.trim().is_empty() {
        gs.on_shortcut(quick, |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                ui::toggle_quick(app);
            }
        })
        .map_err(|_| "quick_hotkey".to_string())?;
    }
    Ok(())
}

pub(crate) fn set_autostart(app: &AppHandle, on: bool) {
    let al = app.autolaunch();
    let _ = if on { al.enable() } else { al.disable() };
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let lang = app.state::<AppState>().settings.read().lang.clone();
    let ru = lang == "ru";
    let open = MenuItem::with_id(app, "open", if ru { "Открыть Clavis" } else { "Open Clavis" }, true, None::<&str>)?;
    let history = MenuItem::with_id(app, "history", if ru { "История буфера" } else { "Clipboard history" }, true, None::<&str>)?;
    let lock = MenuItem::with_id(app, "lock", if ru { "Заблокировать" } else { "Lock" }, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", if ru { "Выйти" } else { "Quit" }, true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&open, &history, &lock, &sep, &quit])?;
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?;
    TrayIconBuilder::with_id("main")
        .icon(icon)
        .tooltip("Clavis")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id().as_ref() {
            "open" => ui::show_main(app, None),
            "history" => ui::toggle_picker(app),
            "lock" => app.state::<AppState>().lock(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                ui::show_main(tray.app_handle(), None);
            }
        })
        .build(app)?;
    Ok(())
}

pub fn run() {
    let hidden = std::env::args().any(|a| a == "--hidden");

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            ui::show_main(app, None);
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--hidden"])))
        .setup(move |app| {
            let handle = app.handle().clone();
            let dir = app.path().app_data_dir()?;
            let shared = shared::Shared::start(handle.clone());
            let state = AppState::init(dir, shared).map_err(|e| Box::<dyn std::error::Error>::from(e))?;
            app.manage(state);

            let st = handle.state::<AppState>();
            let (hotkey, quick, autostart) = {
                let s = st.settings.read();
                (s.hotkey.clone(), s.quick_hotkey.clone(), s.autostart)
            };
            match register_hotkeys(&handle, &hotkey, &quick) {
                Ok(()) => {}
                // Сочетание быстрой заметки занято: оставляем только основное.
                Err(e) if e == "quick_hotkey" => {
                    let _ = register_hotkeys(&handle, &hotkey, "");
                }
                // Основное сочетание занято другой программой: возвращаем стандартное.
                Err(_) => {
                    let _ = register_hotkeys(&handle, "Ctrl+Shift+V", "");
                }
            }
            set_autostart(&handle, autostart);
            build_tray(&handle)?;

            let h = handle.clone();
            clip::start_monitor(move || state::on_clipboard_change(&h));
            let h = handle.clone();
            expander::start(move |m| state::on_expansion(&h, m));
            state::start_idle_watch(handle.clone());
            extras::start_reminders(handle.clone());

            // Проверка устройства на сервере не задерживает запуск.
            let h = handle.clone();
            std::thread::spawn(move || {
                let st = h.state::<AppState>();
                let stage_before = st.stage();
                // Запрос готовится под замком, а отправляется без него: окно не ждёт сеть.
                let req = st.license.lock().startup_request();
                if let Some(req) = req {
                    if let Ok(v) = license::post(&req) {
                        st.license.lock().apply_startup(&v);
                    }
                }
                if st.stage() != stage_before {
                    use tauri::Emitter;
                    let _ = h.emit("status:changed", ());
                }
            });

            ui::picker(&handle);
            let stage = st.stage();
            if !(hidden && (stage == "locked" || stage == "ready")) {
                ui::show_main(&handle, None);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::status,
            commands::activate,
            commands::request_access,
            commands::set_pin,
            commands::confirm_recovery,
            commands::unlock,
            commands::recover,
            commands::lock,
            commands::history_list,
            commands::history_pin,
            commands::history_delete,
            commands::history_clear,
            commands::history_pause,
            commands::history_paused,
            commands::paste_text,
            commands::copy_text,
            commands::paste_item,
            commands::hide_picker,
            commands::open_main,
            commands::folders_list,
            commands::folder_save,
            commands::folder_delete,
            commands::snippets_list,
            commands::snippet_save,
            commands::snippet_delete,
            commands::snippets_export,
            commands::snippets_import,
            commands::notes_list,
            commands::note_get,
            commands::note_save,
            commands::note_delete,
            commands::note_restore,
            commands::note_purge,
            commands::trash_empty,
            commands::note_pick_image,
            commands::note_export_docx,
            commands::settings_get,
            commands::settings_set,
            commands::owner_requests,
            commands::owner_codes,
            commands::owner_devices,
            commands::owner_decline,
            commands::owner_revoke_code,
            commands::owner_revoke_device,
            commands::owner_unblock,
            commands::owner_issue_code,
            commands::shared_list,
            commands::shared_create,
            commands::shared_open,
            commands::shared_close,
            commands::shared_push,
            commands::shared_awareness,
            commands::shared_title,
            commands::shared_invite,
            commands::shared_join,
            commands::shared_members,
            commands::shared_remove,
            commands::shared_leave,
            commands::shared_image_put,
            commands::shared_image_get,
            commands::shared_online,
            extras::journal_list,
            extras::hello_status,
            extras::hello_enable,
            extras::hello_disable,
            extras::unlock_hello,
            extras::ocr_item,
            extras::paste_image,
            extras::consent_give,
            extras::consent_revoke,
            extras::consents,
            extras::translate,
            extras::shorten,
            extras::note_folders,
            extras::note_folder_save,
            extras::note_folder_delete,
            extras::note_meta_set,
            extras::shared_pin,
            extras::note_unlock,
            extras::note_set_secret,
            extras::note_lock_secret,
            extras::versions_list,
            extras::version_get,
            extras::reminders_set,
            extras::reminders_list,
            extras::templates_list,
            extras::template_save,
            extras::template_delete,
            extras::team_snippets,
            extras::erase_everything,
            extras::snippet_used,
            extras::snippet_uses,
            extras::history_image,
            extras::backup_export,
            extras::backup_import,
            extras::quick_note,
            extras::quick_hide,
            extras::open_note,
            extras::open_github,
            extras::export_text,
            extras::export_pdf,
        ])
        .run(tauri::generate_context!())
        .expect("не удалось запустить Clavis");
}
