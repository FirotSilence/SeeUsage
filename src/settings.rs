use crate::{cache, format::ProviderVisibility, i18n::Language, personal::PersonalItems};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard, OnceLock},
};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct HudPosition {
    pub x: i32,
    pub y: i32,
}

#[derive(Deserialize, Serialize)]
#[serde(default)]
struct Settings {
    hud_position: Option<HudPosition>,
    hud_pinned: bool,
    approved_unsafe_tools: BTreeMap<String, String>,
    personal: PersonalItems,
    show_distant_reminders: bool,
    show_codex: bool,
    show_minimax: bool,
    language: Language,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hud_position: None,
            hud_pinned: true,
            approved_unsafe_tools: BTreeMap::new(),
            personal: PersonalItems::default(),
            show_distant_reminders: false,
            show_codex: true,
            show_minimax: false,
            language: Language::default(),
        }
    }
}

#[cfg(target_os = "windows")]
pub fn load_hud_position() -> Option<HudPosition> {
    let _guard = lock_settings();
    load_unlocked().hud_position
}

#[cfg(target_os = "windows")]
pub fn save_hud_position(position: HudPosition) -> Result<(), String> {
    update(|settings| {
        settings.hud_position = Some(position);
    })
}

#[cfg(target_os = "windows")]
pub fn clear_hud_position() -> Result<(), String> {
    update(|settings| {
        settings.hud_position = None;
    })
}

#[cfg(target_os = "windows")]
pub fn hud_pinned() -> bool {
    let _guard = lock_settings();
    load_unlocked().hud_pinned
}

#[cfg(target_os = "windows")]
pub fn set_hud_pinned(pinned: bool) -> Result<(), String> {
    update(|settings| settings.hud_pinned = pinned)
}

pub fn unsafe_tool_approved(provider: &str, path: &Path) -> bool {
    let _guard = lock_settings();
    load_unlocked()
        .approved_unsafe_tools
        .get(provider)
        .is_some_and(|approved| approved == &normalized_path(path))
}

pub fn approve_unsafe_tool(provider: &str, path: &Path) -> Result<(), String> {
    update(|settings| {
        settings
            .approved_unsafe_tools
            .insert(provider.to_string(), normalized_path(path));
    })
}

pub fn clear_unsafe_tool_approvals() -> Result<(), String> {
    update(|settings| settings.approved_unsafe_tools.clear())
}

pub fn has_unsafe_tool_approvals() -> bool {
    let _guard = lock_settings();
    !load_unlocked().approved_unsafe_tools.is_empty()
}

pub fn load_personal_items() -> PersonalItems {
    let _guard = lock_settings();
    load_unlocked().personal
}

pub fn save_personal_items(items: &PersonalItems) -> Result<(), String> {
    update(|settings| settings.personal = items.clone())
}

pub fn show_distant_reminders() -> bool {
    let _guard = lock_settings();
    load_unlocked().show_distant_reminders
}

pub fn set_show_distant_reminders(show: bool) -> Result<(), String> {
    update(|settings| settings.show_distant_reminders = show)
}

pub fn provider_visibility() -> ProviderVisibility {
    let _guard = lock_settings();
    let settings = load_unlocked();
    ProviderVisibility {
        codex: settings.show_codex,
        minimax: settings.show_minimax,
    }
}

pub fn set_provider_visibility(visibility: ProviderVisibility) -> Result<(), String> {
    update(|settings| {
        settings.show_codex = visibility.codex;
        settings.show_minimax = visibility.minimax;
    })
}

pub fn language() -> Language {
    let _guard = lock_settings();
    load_unlocked().language
}

pub fn set_language(language: Language) -> Result<(), String> {
    update(|settings| settings.language = language)
}

fn update(update: impl FnOnce(&mut Settings)) -> Result<(), String> {
    let _guard = lock_settings();
    let mut settings = load_unlocked();
    update(&mut settings);
    save_unlocked(&settings)
}

fn load_unlocked() -> Settings {
    fs::read(settings_path())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn save_unlocked(settings: &Settings) -> Result<(), String> {
    let path = settings_path();
    cache::ensure_data_dir()?;
    let bytes = serde_json::to_vec(settings)
        .map_err(|error| format!("could not serialize settings: {error}"))?;
    fs::write(&path, bytes)
        .map_err(|error| format!("could not write {}: {error}", path.display()))?;
    restrict_permissions(&path)
}

fn normalized_path(path: &Path) -> String {
    let path = path.to_string_lossy().into_owned();
    if cfg!(target_os = "windows") {
        path.to_lowercase()
    } else {
        path
    }
}

fn lock_settings() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn settings_path() -> PathBuf {
    cache::data_dir().join("settings.json")
}

fn restrict_permissions(path: &std::path::Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("could not secure {}: {error}", path.display()))?;
    }

    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_round_trips_through_json() {
        let position = HudPosition { x: -120, y: 45 };
        let bytes = serde_json::to_vec(&Settings {
            hud_position: Some(position),
            ..Settings::default()
        })
        .unwrap();
        let decoded: Settings = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(decoded.hud_position, Some(position));
    }

    #[test]
    fn legacy_settings_keep_the_hud_pinned_by_default() {
        let decoded: Settings = serde_json::from_str("{}").unwrap();
        assert!(decoded.hud_pinned);

        let bytes = serde_json::to_vec(&Settings {
            hud_pinned: false,
            ..Settings::default()
        })
        .unwrap();
        let decoded: Settings = serde_json::from_slice(&bytes).unwrap();
        assert!(!decoded.hud_pinned);
    }

    #[test]
    fn distant_reminders_are_hidden_for_existing_settings() {
        let decoded: Settings = serde_json::from_str("{}").unwrap();
        assert!(!decoded.show_distant_reminders);
    }

    #[test]
    fn distant_reminder_preference_round_trips_through_json() {
        let bytes = serde_json::to_vec(&Settings {
            show_distant_reminders: true,
            ..Settings::default()
        })
        .unwrap();
        let decoded: Settings = serde_json::from_slice(&bytes).unwrap();
        assert!(decoded.show_distant_reminders);
    }

    #[test]
    fn provider_visibility_defaults_and_round_trips_independently() {
        let existing: Settings = serde_json::from_str("{}").unwrap();
        assert!(existing.show_codex);
        assert!(!existing.show_minimax);

        let bytes = serde_json::to_vec(&Settings {
            show_codex: false,
            show_minimax: true,
            ..Settings::default()
        })
        .unwrap();
        let decoded: Settings = serde_json::from_slice(&bytes).unwrap();
        assert!(!decoded.show_codex);
        assert!(decoded.show_minimax);
    }

    #[test]
    fn language_defaults_to_system_and_round_trips() {
        let existing: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(existing.language, Language::System);

        let bytes = serde_json::to_vec(&Settings {
            language: Language::Japanese,
            ..Settings::default()
        })
        .unwrap();
        let decoded: Settings = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(decoded.language, Language::Japanese);
    }
}
