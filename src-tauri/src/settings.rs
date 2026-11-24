//! Настройки. Они не секретны и нужны до разблокировки (сочетание клавиш, язык),
//! поэтому хранятся открыто в prefs.json.

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Expander {
    pub enabled: bool,
    pub prefix: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub lang: String,
    pub hotkey: String,
    pub mode: String,
    pub history_limit: usize,
    pub enter_action: String,
    pub expander: Expander,
    pub ignore_password_managers: bool,
    pub excluded_apps: Vec<String>,
    pub autostart: bool,
    pub lock_after_minutes: u32,
    /// Имя, под которым вас видят соавторы общих заметок.
    pub display_name: String,
    /// Версия условий и политики, с которыми человек согласился.
    #[serde(default)]
    pub legal_accepted: String,
    /// Сочетание для быстрой заметки; пустое — выключено.
    pub quick_hotkey: String,
    /// Тема: dark, light или system.
    pub theme: String,
    /// Просьбу о звезде на GitHub уже показывали.
    pub star_asked: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            lang: "ru".into(),
            hotkey: "Ctrl+Shift+V".into(),
            mode: "popup".into(),
            history_limit: 100,
            enter_action: "paste".into(),
            expander: Expander { enabled: true, prefix: ";".into() },
            ignore_password_managers: true,
            excluded_apps: vec!["KeePass.exe".into(), "KeePassXC.exe".into(), "1Password.exe".into(), "Bitwarden.exe".into()],
            autostart: true,
            lock_after_minutes: 30,
            display_name: String::new(),
            legal_accepted: String::new(),
            quick_hotkey: "Ctrl+Alt+N".into(),
            theme: "dark".into(),
            star_asked: false,
        }
    }
}

impl Settings {
    pub fn load(dir: &Path) -> Settings {
        std::fs::read(dir.join("prefs.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Settings>(&b).ok())
            .map(Settings::sanitized)
            .unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) {
        let _ = std::fs::write(dir.join("prefs.json"), serde_json::to_vec_pretty(self).unwrap_or_default());
    }

    /// Приводит значения к допустимым, чтобы испорченный файл не ломал приложение.
    pub fn sanitized(mut self) -> Settings {
        let d = Settings::default();
        if !matches!(self.lang.as_str(), "ru" | "en") {
            self.lang = d.lang;
        }
        if !matches!(self.mode.as_str(), "popup" | "panel") {
            self.mode = d.mode;
        }
        if !matches!(self.enter_action.as_str(), "paste" | "copy") {
            self.enter_action = d.enter_action;
        }
        self.history_limit = self.history_limit.clamp(10, 1000);
        if self.hotkey.trim().is_empty() {
            self.hotkey = d.hotkey;
        }
        if !matches!(self.theme.as_str(), "dark" | "light" | "system") {
            self.theme = d.theme;
        }
        if self.quick_hotkey == self.hotkey {
            self.quick_hotkey.clear();
        }
        self.expander.prefix = self.expander.prefix.chars().filter(|c| !c.is_whitespace()).take(2).collect();
        if self.expander.prefix.is_empty() {
            self.expander.prefix = ";".into();
        }
        self.excluded_apps.retain(|a| !a.trim().is_empty());
        self.excluded_apps.truncate(100);
        self.display_name = self.display_name.chars().filter(|c| !c.is_control()).take(40).collect::<String>().trim().to_string();
        if self.lock_after_minutes > 24 * 60 {
            self.lock_after_minutes = 24 * 60;
        }
        self
    }
}
