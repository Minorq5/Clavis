//! Экспорт заметки в PDF средствами WebView2: заметка открывается в скрытом окне
//! со стилями для печати и сохраняется движком Edge, поэтому кириллица, таблицы
//! и изображения выглядят так же, как на экране.

use std::path::PathBuf;

#[cfg(windows)]
pub fn print_to_pdf(app: &tauri::AppHandle, html: String, title: String, path: PathBuf) -> Result<(), String> {
    use std::sync::mpsc;
    use std::time::Duration;
    use tauri::{Emitter, Listener, WebviewUrl, WebviewWindowBuilder};
    use webview2_com::Microsoft::Web::WebView2::Win32::*;
    use webview2_com::PrintToPdfCompletedHandler;
    use windows_core_62::{Interface, HSTRING};

    let label = format!("print-{}", uuid::Uuid::new_v4().simple());
    let (ready_tx, ready_rx) = mpsc::channel::<()>();
    let (done_tx, done_rx) = mpsc::channel::<()>();
    let ready_id = app.once(format!("print:ready:{label}"), move |_| {
        let _ = ready_tx.send(());
    });
    let done_id = app.once(format!("print:rendered:{label}"), move |_| {
        let _ = done_tx.send(());
    });

    let win = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("index.html#view=print".into()))
        .visible(false)
        .inner_size(820.0, 1160.0)
        .skip_taskbar(true)
        .build()
        .map_err(|e| e.to_string())?;

    let cleanup = |app: &tauri::AppHandle| {
        app.unlisten(ready_id);
        app.unlisten(done_id);
    };

    if ready_rx.recv_timeout(Duration::from_secs(15)).is_err() {
        cleanup(app);
        let _ = win.destroy();
        return Err("timeout".into());
    }
    let _ = win.emit("print:html", serde_json::json!({ "html": html, "title": title }));
    if done_rx.recv_timeout(Duration::from_secs(20)).is_err() {
        cleanup(app);
        let _ = win.destroy();
        return Err("timeout".into());
    }

    let (res_tx, res_rx) = mpsc::channel::<bool>();
    let target = path.display().to_string();
    let call = win.with_webview(move |wv| unsafe {
        let run = || -> windows_core_62::Result<()> {
            let core = wv.controller().CoreWebView2()?;
            let core7: ICoreWebView2_7 = core.cast()?;
            let env6: ICoreWebView2Environment6 = wv.environment().cast()?;
            let settings = env6.CreatePrintSettings()?;
            settings.SetShouldPrintHeaderAndFooter(false)?;
            settings.SetShouldPrintBackgrounds(true)?;
            settings.SetMarginTop(0.55)?;
            settings.SetMarginBottom(0.55)?;
            settings.SetMarginLeft(0.6)?;
            settings.SetMarginRight(0.6)?;
            let tx = res_tx.clone();
            let handler = PrintToPdfCompletedHandler::create(Box::new(move |hr, ok| {
                let _ = tx.send(hr.is_ok() && ok);
                Ok(())
            }));
            core7.PrintToPdf(&HSTRING::from(target.as_str()), &settings, &handler)
        };
        if run().is_err() {
            let _ = res_tx.send(false);
        }
    });
    let ok = call.is_ok() && res_rx.recv_timeout(Duration::from_secs(30)).unwrap_or(false);
    cleanup(app);
    let _ = win.destroy();
    if ok {
        Ok(())
    } else {
        Err("pdf".into())
    }
}

#[cfg(not(windows))]
pub fn print_to_pdf(_: &tauri::AppHandle, _: String, _: String, _: PathBuf) -> Result<(), String> {
    Err("unsupported".into())
}
