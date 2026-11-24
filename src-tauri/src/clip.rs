//! Буфер обмена Windows: слежение, чтение и запись.

use image::{imageops::FilterType, DynamicImage, ImageFormat};
use std::io::Cursor;

pub enum Content {
    Text { text: String, html: Option<String> },
    Image { png: Vec<u8>, width: u32, height: u32, thumb: String },
    Files(Vec<String>),
}

pub struct Capture {
    pub content: Content,
    pub source: Option<String>,
}

const MAX_IMAGE_BYTES: usize = 40 * 1024 * 1024;
const MAX_TEXT_CHARS: usize = 2_000_000;

pub fn thumbnail(img: &DynamicImage) -> String {
    use base64::Engine;
    let t = img.resize(320, 200, FilterType::Triangle).to_rgb8();
    let mut out = Cursor::new(Vec::new());
    let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 78);
    if enc.encode_image(&t).is_err() {
        return String::new();
    }
    format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(out.into_inner()))
}

pub fn png_from_image(img: &DynamicImage) -> Option<Vec<u8>> {
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, ImageFormat::Png).ok()?;
    Some(out.into_inner())
}

#[cfg(windows)]
pub use imp::*;

#[cfg(windows)]
mod imp {
    use super::*;
    use clipboard_win::{formats, raw, Clipboard};
    use windows::core::PWSTR;
    use windows::Win32::Foundation::{CloseHandle, HWND};
    use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION};
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    /// Запускает поток, который вызывает `on_change` при каждом изменении буфера.
    pub fn start_monitor(on_change: impl Fn() + Send + 'static) {
        std::thread::Builder::new()
            .name("clipboard-monitor".into())
            .spawn(move || {
                let Ok(mut mon) = clipboard_win::Monitor::new() else { return };
                while let Ok(changed) = mon.recv() {
                    if changed {
                        on_change();
                    }
                }
            })
            .ok();
    }

    pub fn seq() -> u32 {
        raw::seq_num().map(|n| n.get()).unwrap_or(0)
    }

    fn fmt(name: &str) -> Option<u32> {
        raw::register_format(name).map(|n| n.get())
    }

    /// Менеджеры паролей и другие программы помечают секреты специальными форматами.
    fn marked_private() -> bool {
        for name in ["ExcludeClipboardContentFromMonitorProcessing", "Clipboard Viewer Ignore"] {
            if let Some(f) = fmt(name) {
                if raw::is_format_avail(f) {
                    return true;
                }
            }
        }
        for name in ["CanIncludeInClipboardHistory", "CanUploadToCloudClipboard"] {
            if let Some(f) = fmt(name) {
                if raw::is_format_avail(f) {
                    let mut v = Vec::new();
                    if raw::get_vec(f, &mut v).is_ok() && v.len() >= 4 && v[..4] == [0, 0, 0, 0] {
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn process_name(hwnd: HWND) -> Option<String> {
        unsafe {
            if hwnd.0.is_null() {
                return None;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == 0 {
                return None;
            }
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            let r = QueryFullProcessImageNameW(h, PROCESS_NAME_FORMAT(0), PWSTR(buf.as_mut_ptr()), &mut len);
            let _ = CloseHandle(h);
            r.ok()?;
            let path = String::from_utf16_lossy(&buf[..len as usize]);
            path.rsplit('\\').next().map(String::from)
        }
    }

    fn source_app() -> Option<String> {
        let owner = raw::get_owner().map(|p| HWND(p.as_ptr()));
        owner.and_then(process_name).or_else(|| unsafe { process_name(GetForegroundWindow()) })
    }

    /// Читает буфер. None, если содержимое помечено как приватное, пустое или слишком большое.
    pub fn read(ignore_private: bool) -> Option<Capture> {
        let _clip = Clipboard::new_attempts(10).ok()?;
        if ignore_private && marked_private() {
            return None;
        }
        let source = source_app();

        if raw::is_format_avail(formats::CF_HDROP) {
            let mut files: Vec<String> = Vec::new();
            if raw::get_file_list(&mut files).is_ok() && !files.is_empty() {
                return Some(Capture { content: Content::Files(files), source });
            }
        }

        if raw::is_format_avail(formats::CF_UNICODETEXT) {
            let mut v = Vec::new();
            if raw::get_string(&mut v).is_ok() {
                let text = String::from_utf8_lossy(&v).trim_end_matches('\0').to_string();
                if !text.trim().is_empty() && text.chars().count() <= MAX_TEXT_CHARS {
                    let html = fmt("HTML Format").filter(|f| raw::is_format_avail(*f)).and_then(|f| {
                        let mut h = Vec::new();
                        raw::get_html(f, &mut h).ok()?;
                        let s = String::from_utf8(h).ok()?;
                        (s.len() < 4 * 1024 * 1024).then_some(s)
                    });
                    return Some(Capture { content: Content::Text { text, html }, source });
                }
            }
        }

        // Сначала PNG: в нём сохраняется прозрачность.
        let img = fmt("PNG")
            .filter(|f| raw::is_format_avail(*f))
            .and_then(|f| {
                let mut v = Vec::new();
                raw::get_vec(f, &mut v).ok()?;
                (v.len() <= MAX_IMAGE_BYTES).then(|| image::load_from_memory_with_format(&v, ImageFormat::Png).ok()).flatten()
            })
            .or_else(|| {
                if !raw::is_format_avail(formats::CF_BITMAP) && !raw::is_format_avail(formats::CF_DIB) {
                    return None;
                }
                let mut v = Vec::new();
                raw::get_bitmap(&mut v).ok()?;
                (v.len() <= MAX_IMAGE_BYTES).then(|| image::load_from_memory_with_format(&v, ImageFormat::Bmp).ok()).flatten()
            });
        if let Some(img) = img {
            let png = png_from_image(&img)?;
            let thumb = thumbnail(&img);
            return Some(Capture { content: Content::Image { width: img.width(), height: img.height(), png, thumb }, source });
        }
        None
    }

    pub fn write_text(text: &str, html: Option<&str>) -> bool {
        let Ok(_clip) = Clipboard::new_attempts(10) else { return false };
        if raw::empty().is_err() {
            return false;
        }
        let ok = raw::set_string_with(text, clipboard_win::options::NoClear).is_ok();
        if let (Some(h), Some(f)) = (html, fmt("HTML Format")) {
            let _ = raw::set_html(f, h);
        }
        ok
    }

    pub fn write_image(png: &[u8]) -> bool {
        let Ok(img) = image::load_from_memory_with_format(png, ImageFormat::Png) else { return false };
        let mut bmp = Cursor::new(Vec::new());
        if DynamicImage::ImageRgba8(img.to_rgba8()).write_to(&mut bmp, ImageFormat::Bmp).is_err() {
            return false;
        }
        let Ok(_clip) = Clipboard::new_attempts(10) else { return false };
        if raw::empty().is_err() {
            return false;
        }
        let ok = raw::set_bitmap_with(bmp.get_ref(), clipboard_win::options::NoClear).is_ok();
        if let Some(f) = fmt("PNG") {
            let _ = raw::set_without_clear(f, png);
        }
        ok
    }

    pub fn write_files(files: &[String]) -> bool {
        let Ok(_clip) = Clipboard::new_attempts(10) else { return false };
        raw::set_file_list(files).is_ok()
    }

    pub fn read_text() -> Option<String> {
        clipboard_win::get_clipboard_string().ok()
    }
}

#[cfg(not(windows))]
pub fn start_monitor(_: impl Fn() + Send + 'static) {}
#[cfg(not(windows))]
pub fn seq() -> u32 { 0 }
#[cfg(not(windows))]
pub fn read(_: bool) -> Option<Capture> { None }
#[cfg(not(windows))]
pub fn write_text(_: &str, _: Option<&str>) -> bool { false }
#[cfg(not(windows))]
pub fn write_image(_: &[u8]) -> bool { false }
#[cfg(not(windows))]
pub fn write_files(_: &[String]) -> bool { false }
#[cfg(not(windows))]
pub fn read_text() -> Option<String> { None }
