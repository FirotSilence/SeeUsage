use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, env, sync::OnceLock};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    #[default]
    System,
    English,
    SimplifiedChinese,
    TraditionalChinese,
    Japanese,
    Korean,
    Spanish,
    French,
    German,
}

impl Language {
    pub const ALL: [Self; 9] = [
        Self::System,
        Self::English,
        Self::SimplifiedChinese,
        Self::TraditionalChinese,
        Self::Japanese,
        Self::Korean,
        Self::Spanish,
        Self::French,
        Self::German,
    ];

    #[cfg(test)]
    const TRANSLATED: [Self; 8] = [
        Self::English,
        Self::SimplifiedChinese,
        Self::TraditionalChinese,
        Self::Japanese,
        Self::Korean,
        Self::Spanish,
        Self::French,
        Self::German,
    ];

    pub fn effective(self) -> Self {
        if self == Self::System {
            detected_language()
        } else {
            self
        }
    }

    pub const fn menu_id(self) -> &'static str {
        match self {
            Self::System => "language.system",
            Self::English => "language.en",
            Self::SimplifiedChinese => "language.zh_cn",
            Self::TraditionalChinese => "language.zh_tw",
            Self::Japanese => "language.ja",
            Self::Korean => "language.ko",
            Self::Spanish => "language.es",
            Self::French => "language.fr",
            Self::German => "language.de",
        }
    }

    pub fn from_menu_id(id: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|language| language.menu_id() == id)
    }

    pub fn menu_label(self, interface_language: Self) -> &'static str {
        match self {
            Self::System => text(interface_language, key::SYSTEM_DEFAULT),
            Self::English => "English",
            Self::SimplifiedChinese => "简体中文",
            Self::TraditionalChinese => "繁體中文",
            Self::Japanese => "日本語",
            Self::Korean => "한국어",
            Self::Spanish => "Español",
            Self::French => "Français",
            Self::German => "Deutsch",
        }
    }

    pub const fn is_cjk(self) -> bool {
        matches!(
            self,
            Self::SimplifiedChinese | Self::TraditionalChinese | Self::Japanese | Self::Korean
        )
    }
}

pub mod key {
    pub const LANGUAGE: &str = "language";
    pub const SYSTEM_DEFAULT: &str = "system_default";
    pub const DISPLAY: &str = "display";
    pub const SHOW_CODEX: &str = "show_codex";
    pub const SHOW_MINIMAX: &str = "show_minimax";
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    pub const SHOW_METER: &str = "show_meter";
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    pub const RESET_METER: &str = "reset_meter";
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    pub const PIN_METER: &str = "pin_meter";
    pub const REFRESH_NOW: &str = "refresh_now";
    pub const REFRESHING: &str = "refreshing";
    pub const LAUNCH_LOGIN: &str = "launch_login";
    pub const LAUNCH_UNAVAILABLE: &str = "launch_unavailable";
    pub const FORGET_APPROVALS: &str = "forget_approvals";
    pub const CLEAR_APPROVALS_FAILED: &str = "clear_approvals_failed";
    pub const QUIT: &str = "quit";
    pub const NOTES: &str = "notes";
    pub const ADD_NOTE: &str = "add_note";
    pub const NO_NOTES: &str = "no_notes";
    pub const DELETE_NOTE: &str = "delete_note";
    pub const REMINDERS: &str = "reminders";
    pub const ADD_REMINDER: &str = "add_reminder";
    pub const AFTER_DELAY: &str = "after_delay";
    pub const REPEAT_EVERY: &str = "repeat_every";
    pub const AT_SCHEDULE: &str = "at_schedule";
    pub const MINUTES: &str = "minutes";
    pub const HOURS: &str = "hours";
    pub const DAYS: &str = "days";
    pub const WEEKS: &str = "weeks";
    pub const ONE_TIME: &str = "one_time";
    pub const DAILY: &str = "daily";
    pub const WEEKDAYS: &str = "weekdays";
    pub const WEEKENDS: &str = "weekends";
    pub const WEEKLY: &str = "weekly";
    pub const SELECTED_WEEKDAYS: &str = "selected_weekdays";
    pub const MONTHLY: &str = "monthly";
    pub const MONTHLY_LAST: &str = "monthly_last";
    pub const SOLAR_DATE: &str = "solar_date";
    pub const LUNAR_DATE: &str = "lunar_date";
    pub const SHOW_DISTANT: &str = "show_distant";
    #[cfg(target_os = "windows")]
    pub const UPDATE_INTERVAL_ANCHOR_ON_CLICK: &str = "update_interval_anchor_on_click";
    pub const NO_REMINDERS: &str = "no_reminders";
    pub const MARK_COMPLETE: &str = "mark_complete";
    pub const DELETE_REMINDER: &str = "delete_reminder";
    pub const ADD_NOTE_TITLE: &str = "add_note_title";
    pub const SHORT_NOTE_PROMPT: &str = "short_note_prompt";
    pub const FOCUS: &str = "focus";
    pub const REMINDER: &str = "reminder";
    pub const REMINDER_TEXT_PROMPT: &str = "reminder_text_prompt";
    pub const CANCEL: &str = "cancel";
    pub const ADD: &str = "add";
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub const OK: &str = "ok";
    pub const INPUT_VALUE: &str = "input_value";
    pub const DATE_TIME_FORMAT: &str = "date_time_format";
    pub const TIME_FORMAT: &str = "time_format";
    pub const WEEKDAY_TIME_FORMAT: &str = "weekday_time_format";
    pub const WEEKDAYS_TIME_FORMAT: &str = "weekdays_time_format";
    pub const DAY_TIME_FORMAT: &str = "day_time_format";
    pub const SOLAR_TIME_FORMAT: &str = "solar_time_format";
    pub const LUNAR_TIME_FORMAT: &str = "lunar_time_format";
    pub const TEXT_EMPTY: &str = "text_empty";
    pub const MAX_NOTES: &str = "max_notes";
    pub const MAX_REMINDERS: &str = "max_reminders";
    pub const TEXT_TOO_LONG: &str = "text_too_long";
    pub const USAGE_UNAVAILABLE: &str = "usage_unavailable";
    pub const UNLIMITED: &str = "unlimited";
    pub const LEFT: &str = "left";
    pub const CURRENT: &str = "current";
    pub const WEEK: &str = "week";
    pub const LOGIN_REQUIRED: &str = "login_required";
    pub const CLI_MISSING: &str = "cli_missing";
    pub const CLI_APPROVAL_REQUIRED: &str = "cli_approval_required";
    pub const UNSAFE_TOOL_PATH: &str = "unsafe_tool_path";
    pub const STALE: &str = "stale";
    pub const UNAVAILABLE: &str = "unavailable";
    pub const UPDATED: &str = "updated";
    pub const CHECKED_NO_DATA: &str = "checked_no_data";
    pub const WAITING_FIRST_REFRESH: &str = "waiting_first_refresh";
    pub const RESET_DUE: &str = "reset_due";
    pub const RESETS_IN: &str = "resets_in";
    pub const JUST_NOW: &str = "just_now";
    pub const AGO: &str = "ago";
    pub const DUE: &str = "due";
    pub const SELECTED_DAYS: &str = "selected_days";
    pub const EVERY: &str = "every";
    pub const MONTH_DAY: &str = "month_day";
    pub const LAST_DAY_MONTHLY: &str = "last_day_monthly";
    pub const SOLAR_YEARLY: &str = "solar_yearly";
    pub const LUNAR: &str = "lunar";
    pub const LEAP: &str = "leap";
    pub const UNIT_WEEK: &str = "unit_week";
    pub const UNIT_WEEKS: &str = "unit_weeks";
    pub const UNIT_DAY: &str = "unit_day";
    pub const UNIT_DAYS: &str = "unit_days";
    pub const UNIT_HOUR: &str = "unit_hour";
    pub const UNIT_HOURS: &str = "unit_hours";
    pub const UNIT_MINUTE: &str = "unit_minute";
    pub const UNIT_MINUTES: &str = "unit_minutes";
    pub const UNIT_SECOND: &str = "unit_second";
    pub const UNIT_SECONDS: &str = "unit_seconds";
    pub const SECURITY_TITLE: &str = "security_title";
    pub const SECURITY_FOUND: &str = "security_found";
    pub const SECURITY_WRITABLE: &str = "security_writable";
    pub const SECURITY_REPLACED: &str = "security_replaced";
    pub const SECURITY_QUESTION: &str = "security_question";
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub const ALLOW: &str = "allow";
    pub const WHOLE_NUMBER: &str = "whole_number";
    pub const NUMBER_RANGE: &str = "number_range";
    pub const LOCAL_TIME_MISSING: &str = "local_time_missing";
    pub const FUTURE_TIME_REQUIRED: &str = "future_time_required";
    pub const NEXT_REMINDER_FAILED: &str = "next_reminder_failed";
    pub const INTERVAL_TOO_LARGE: &str = "interval_too_large";
    pub const REMINDER_OUT_OF_RANGE: &str = "reminder_out_of_range";
    pub const TIME_INVALID: &str = "time_invalid";
    pub const UNKNOWN_WEEKDAY: &str = "unknown_weekday";
    pub const WEEKDAY_REQUIRED: &str = "weekday_required";
    pub const MONTH_DAY_RANGE: &str = "month_day_range";
    pub const MONTH_RANGE: &str = "month_range";
    pub const DAY_INVALID: &str = "day_invalid";
    pub const DATE_INVALID: &str = "date_invalid";
    pub const LUNAR_MONTH_RANGE: &str = "lunar_month_range";
    pub const LUNAR_DAY_RANGE: &str = "lunar_day_range";
    pub const LUNAR_NOT_FOUND: &str = "lunar_not_found";
}

pub fn text(language: Language, key: &'static str) -> &'static str {
    let language = language.effective();
    translations(language)
        .get(key)
        .or_else(|| translations(Language::English).get(key))
        .map(String::as_str)
        .unwrap_or(key)
}

pub fn template(language: Language, key: &'static str, values: &[(&str, String)]) -> String {
    let mut result = text(language, key).to_string();
    for (name, value) in values {
        result = result.replace(&format!("{{{name}}}"), value);
    }
    result
}

fn translations(language: Language) -> &'static BTreeMap<String, String> {
    macro_rules! catalog {
        ($cell:ident, $path:literal) => {{
            static $cell: OnceLock<BTreeMap<String, String>> = OnceLock::new();
            $cell.get_or_init(|| {
                serde_json::from_str(include_str!($path))
                    .expect(concat!("invalid embedded translation catalog: ", $path))
            })
        }};
    }

    match language {
        Language::System | Language::English => catalog!(EN, "../locales/en.json"),
        Language::SimplifiedChinese => catalog!(ZH_CN, "../locales/zh-CN.json"),
        Language::TraditionalChinese => catalog!(ZH_TW, "../locales/zh-TW.json"),
        Language::Japanese => catalog!(JA, "../locales/ja.json"),
        Language::Korean => catalog!(KO, "../locales/ko.json"),
        Language::Spanish => catalog!(ES, "../locales/es.json"),
        Language::French => catalog!(FR, "../locales/fr.json"),
        Language::German => catalog!(DE, "../locales/de.json"),
    }
}

fn detected_language() -> Language {
    static DETECTED: OnceLock<Language> = OnceLock::new();
    *DETECTED.get_or_init(|| {
        platform_locale()
            .as_deref()
            .and_then(language_from_locale)
            .or_else(|| {
                ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"]
                    .into_iter()
                    .filter_map(|name| env::var(name).ok())
                    .find_map(|locale| language_from_locale(&locale))
            })
            .unwrap_or(Language::English)
    })
}

fn language_from_locale(locale: &str) -> Option<Language> {
    let locale = locale
        .split(['.', ':'])
        .next()
        .unwrap_or(locale)
        .replace('_', "-")
        .to_ascii_lowercase();
    if locale.starts_with("zh-hant")
        || locale.starts_with("zh-tw")
        || locale.starts_with("zh-hk")
        || locale.starts_with("zh-mo")
    {
        Some(Language::TraditionalChinese)
    } else if locale.starts_with("zh") {
        Some(Language::SimplifiedChinese)
    } else if locale.starts_with("ja") {
        Some(Language::Japanese)
    } else if locale.starts_with("ko") {
        Some(Language::Korean)
    } else if locale.starts_with("es") {
        Some(Language::Spanish)
    } else if locale.starts_with("fr") {
        Some(Language::French)
    } else if locale.starts_with("de") {
        Some(Language::German)
    } else if locale.starts_with("en") {
        Some(Language::English)
    } else {
        None
    }
}

#[cfg(target_os = "windows")]
fn platform_locale() -> Option<String> {
    use windows_sys::Win32::{
        Globalization::GetUserDefaultLocaleName, System::SystemServices::LOCALE_NAME_MAX_LENGTH,
    };

    let mut buffer = vec![0u16; LOCALE_NAME_MAX_LENGTH as usize];
    let length = unsafe { GetUserDefaultLocaleName(buffer.as_mut_ptr(), buffer.len() as i32) };
    (length > 1).then(|| String::from_utf16_lossy(&buffer[..length as usize - 1]))
}

#[cfg(target_os = "macos")]
fn platform_locale() -> Option<String> {
    use std::process::{Command, Stdio};

    let output = Command::new("/usr/bin/defaults")
        .args(["read", "-g", "AppleLocale"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|locale| !locale.is_empty())
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn platform_locale() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_detection_understands_supported_variants() {
        assert_eq!(
            language_from_locale("zh-Hant-HK"),
            Some(Language::TraditionalChinese)
        );
        assert_eq!(
            language_from_locale("zh_CN.UTF-8"),
            Some(Language::SimplifiedChinese)
        );
        assert_eq!(language_from_locale("ja_JP"), Some(Language::Japanese));
        assert_eq!(language_from_locale("pt_BR"), None);
    }

    #[test]
    fn every_catalog_has_the_english_keys() {
        let english = translations(Language::English);
        for language in Language::TRANSLATED {
            let catalog = translations(language);
            let missing = english
                .keys()
                .filter(|key| !catalog.contains_key(*key))
                .collect::<Vec<_>>();
            assert!(missing.is_empty(), "{language:?} is missing {missing:?}");
            assert_eq!(catalog.len(), english.len(), "{language:?} has extra keys");
            for (key, english_value) in english {
                let translated = &catalog[key];
                assert!(!translated.trim().is_empty(), "{language:?}:{key} is empty");
                assert_eq!(
                    placeholders(translated),
                    placeholders(english_value),
                    "{language:?}:{key} has different placeholders"
                );
            }
        }
    }

    fn placeholders(value: &str) -> Vec<&str> {
        let mut result = value
            .match_indices('{')
            .filter_map(|(start, _)| {
                let tail = &value[start + 1..];
                let end = tail.find('}')?;
                Some(&tail[..end])
            })
            .collect::<Vec<_>>();
        result.sort_unstable();
        result
    }

    #[test]
    fn language_menu_ids_round_trip() {
        for language in Language::ALL {
            assert_eq!(Language::from_menu_id(language.menu_id()), Some(language));
        }
    }
}
