//! Службы Windows: распознавание текста и Windows Hello.

#[derive(Debug, thiserror::Error)]
pub enum WinRtError {
    #[error("unavailable")]
    Unavailable,
    #[error("cancelled")]
    Cancelled,
    #[error("{0}")]
    Other(String),
}

#[cfg(windows)]
mod imp {
    use super::WinRtError;
    use windows::core::HSTRING;
    use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapDecoder, BitmapPixelFormat, SoftwareBitmap};
    use windows::Media::Ocr::OcrEngine;
    use windows::Security::Credentials::{KeyCredentialCreationOption, KeyCredentialManager, KeyCredentialStatus};
    use windows::Security::Cryptography::CryptographicBuffer;
    use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};

    fn other(e: windows::core::Error) -> WinRtError {
        WinRtError::Other(e.message().to_string())
    }

    /// Распознаёт текст на изображении средствами Windows (без интернета).
    /// Языки берутся из настроек Windows; русский распознаётся, если установлен его языковой пакет.
    pub fn ocr(png: &[u8]) -> Result<String, WinRtError> {
        let stream = InMemoryRandomAccessStream::new().map_err(other)?;
        let writer = DataWriter::CreateDataWriter(&stream).map_err(other)?;
        writer.WriteBytes(png).map_err(other)?;
        writer.StoreAsync().map_err(other)?.get().map_err(other)?;
        writer.FlushAsync().map_err(other)?.get().map_err(other)?;
        writer.DetachStream().map_err(other)?;
        stream.Seek(0).map_err(other)?;

        let decoder = BitmapDecoder::CreateAsync(&stream).map_err(other)?.get().map_err(other)?;
        let bitmap = decoder.GetSoftwareBitmapAsync().map_err(other)?.get().map_err(other)?;
        // Распознаватель принимает только Bgra8 или Gray8.
        let bitmap = SoftwareBitmap::ConvertWithAlpha(&bitmap, BitmapPixelFormat::Bgra8, BitmapAlphaMode::Premultiplied).map_err(other)?;

        let max = OcrEngine::MaxImageDimension().map_err(other)?;
        if bitmap.PixelWidth().unwrap_or(0) as u32 > max || bitmap.PixelHeight().unwrap_or(0) as u32 > max {
            return Err(WinRtError::Other("too_large".into()));
        }
        let engine = OcrEngine::TryCreateFromUserProfileLanguages().map_err(|_| WinRtError::Unavailable)?;
        let result = engine.RecognizeAsync(&bitmap).map_err(other)?.get().map_err(other)?;
        let mut out = String::new();
        for line in result.Lines().map_err(other)? {
            out.push_str(&line.Text().map_err(other)?.to_string());
            out.push('\n');
        }
        Ok(out.trim_end().to_string())
    }

    const HELLO_NAME: &str = "Clavis";

    pub fn hello_available() -> bool {
        KeyCredentialManager::IsSupportedAsync().and_then(|a| a.get()).unwrap_or(false)
    }

    /// Подписывает фиксированную строку ключом Windows Hello. Подпись RSA PKCS#1 детерминирована,
    /// поэтому из неё можно вывести один и тот же ключ, но только после проверки лица, пальца или PIN Windows.
    pub fn hello_sign(challenge: &[u8], create: bool) -> Result<Vec<u8>, WinRtError> {
        let name = HSTRING::from(HELLO_NAME);
        let cred = if create {
            let r = KeyCredentialManager::RequestCreateAsync(&name, KeyCredentialCreationOption::ReplaceExisting).map_err(other)?.get().map_err(other)?;
            match r.Status().map_err(other)? {
                KeyCredentialStatus::Success => r.Credential().map_err(other)?,
                KeyCredentialStatus::UserCanceled => return Err(WinRtError::Cancelled),
                _ => return Err(WinRtError::Unavailable),
            }
        } else {
            let r = KeyCredentialManager::OpenAsync(&name).map_err(other)?.get().map_err(other)?;
            match r.Status().map_err(other)? {
                KeyCredentialStatus::Success => r.Credential().map_err(other)?,
                _ => return Err(WinRtError::Unavailable),
            }
        };
        let buf = CryptographicBuffer::CreateFromByteArray(challenge).map_err(other)?;
        let res = cred.RequestSignAsync(&buf).map_err(other)?.get().map_err(other)?;
        match res.Status().map_err(other)? {
            KeyCredentialStatus::Success => {}
            KeyCredentialStatus::UserCanceled => return Err(WinRtError::Cancelled),
            _ => return Err(WinRtError::Unavailable),
        }
        let sig = res.Result().map_err(other)?;
        let mut out = windows::core::Array::<u8>::new();
        CryptographicBuffer::CopyToByteArray(&sig, &mut out).map_err(other)?;
        Ok(out.to_vec())
    }

    pub fn hello_remove() {
        let _ = KeyCredentialManager::DeleteAsync(&HSTRING::from(HELLO_NAME)).and_then(|a| a.get());
    }

    // Уведомления держим живыми, пока по ним могут нажать: иначе событие нажатия не придёт.
    static TOASTS: std::sync::Mutex<Vec<windows::UI::Notifications::ToastNotification>> = std::sync::Mutex::new(Vec::new());

    fn xml_escape(s: &str) -> String {
        s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
    }

    /// Уведомление Windows, нажатие на которое вызывает `on_click`.
    pub fn toast(app_id: &str, title: &str, body: &str, on_click: Box<dyn Fn() + Send + Sync>) -> Result<(), WinRtError> {
        use windows::Data::Xml::Dom::XmlDocument;
        use windows::Foundation::TypedEventHandler;
        use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
        let xml = XmlDocument::new().map_err(other)?;
        let text = format!(
            "<toast><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual></toast>",
            xml_escape(title),
            xml_escape(body)
        );
        xml.LoadXml(&HSTRING::from(text)).map_err(other)?;
        let t = ToastNotification::CreateToastNotification(&xml).map_err(other)?;
        t.Activated(&TypedEventHandler::new(move |_, _| {
            on_click();
            Ok(())
        }))
        .map_err(other)?;
        ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(app_id)).map_err(other)?.Show(&t).map_err(other)?;
        if let Ok(mut l) = TOASTS.lock() {
            l.push(t);
            let n = l.len();
            if n > 20 {
                l.drain(..n - 20);
            }
        }
        Ok(())
    }
}

#[cfg(not(windows))]
mod imp {
    use super::WinRtError;
    pub fn ocr(_: &[u8]) -> Result<String, WinRtError> {
        Err(WinRtError::Unavailable)
    }
    pub fn hello_available() -> bool {
        false
    }
    pub fn hello_sign(_: &[u8], _: bool) -> Result<Vec<u8>, WinRtError> {
        Err(WinRtError::Unavailable)
    }
    pub fn hello_remove() {}
    pub fn toast(_: &str, _: &str, _: &str, _: Box<dyn Fn() + Send + Sync>) -> Result<(), WinRtError> {
        Err(WinRtError::Unavailable)
    }
}

pub use imp::*;
