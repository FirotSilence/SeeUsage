use crate::{
    i18n::{self, Language, key},
    model::{
        AppSnapshot, ProviderSnapshot, ProviderStatus, UsageItem, UsageWindow, unix_timestamp,
    },
};

pub struct DisplayState {
    pub codex_line: String,
    pub minimax_line: String,
    pub updated_line: String,
    pub tooltip: String,
    pub title: String,
    #[cfg(target_os = "windows")]
    pub codex_remaining: Option<u8>,
    #[cfg(target_os = "windows")]
    pub minimax_remaining: Option<u8>,
    #[cfg(target_os = "windows")]
    pub codex_stale: bool,
    #[cfg(target_os = "windows")]
    pub minimax_stale: bool,
    pub minimum_remaining: Option<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderVisibility {
    pub codex: bool,
    pub minimax: bool,
}

impl Default for ProviderVisibility {
    fn default() -> Self {
        Self {
            codex: true,
            minimax: false,
        }
    }
}

pub fn display(
    snapshot: &AppSnapshot,
    language: Language,
    visibility: ProviderVisibility,
) -> DisplayState {
    let language = language.effective();
    let codex_line = codex_line(&snapshot.codex, language);
    let minimax_line = minimax_line(&snapshot.minimax, language);
    let updated_line = update_line(snapshot, language);
    let codex_percent = snapshot
        .codex
        .preferred_item("codex")
        .and_then(item_remaining);
    let minimax_percent = snapshot
        .minimax
        .preferred_item("general")
        .and_then(current_remaining);

    let mut tooltip_parts = vec!["SeeUsage".to_string()];
    let mut title_parts = Vec::new();
    if visibility.codex {
        tooltip_parts.push(codex_line.clone());
        title_parts.push(format!("C{}", short_percent(codex_percent)));
    }
    if visibility.minimax {
        tooltip_parts.push(minimax_line.clone());
        title_parts.push(format!("M{}", short_percent(minimax_percent)));
    }
    let tooltip = truncate_chars(&tooltip_parts.join(" | "), 120);
    let title = if title_parts.is_empty() {
        "SeeUsage".into()
    } else {
        title_parts.join(" ")
    };
    let minimum_remaining = [
        visibility.codex.then_some(codex_percent).flatten(),
        visibility.minimax.then_some(minimax_percent).flatten(),
    ]
    .into_iter()
    .flatten()
    .min();

    DisplayState {
        codex_line,
        minimax_line,
        updated_line,
        tooltip,
        title,
        #[cfg(target_os = "windows")]
        codex_remaining: codex_percent,
        #[cfg(target_os = "windows")]
        minimax_remaining: minimax_percent,
        #[cfg(target_os = "windows")]
        codex_stale: provider_is_stale(&snapshot.codex),
        #[cfg(target_os = "windows")]
        minimax_stale: provider_is_stale(&snapshot.minimax),
        minimum_remaining,
    }
}

fn codex_line(provider: &ProviderSnapshot, language: Language) -> String {
    let Some(item) = provider.preferred_item("codex") else {
        return unavailable_line("Codex", provider, language);
    };
    let Some(window) = most_constrained_window(item) else {
        return with_stale(
            format!("Codex: {}", i18n::text(language, key::USAGE_UNAVAILABLE)),
            provider,
            language,
        );
    };

    let line = if window.unlimited {
        format!("Codex: {}", i18n::text(language, key::UNLIMITED))
    } else if language.is_cjk() {
        let mut line = format!(
            "Codex: {} {}%",
            i18n::text(language, key::LEFT),
            window.remaining_percent
        );
        if let Some(reset) = reset_text(window.resets_at, language) {
            line.push_str(" | ");
            line.push_str(&reset);
        }
        line
    } else {
        let mut line = format!(
            "Codex: {}% {}",
            window.remaining_percent,
            i18n::text(language, key::LEFT)
        );
        if let Some(reset) = reset_text(window.resets_at, language) {
            line.push_str(" | ");
            line.push_str(&reset);
        }
        line
    };
    with_stale(line, provider, language)
}

fn minimax_line(provider: &ProviderSnapshot, language: Language) -> String {
    let Some(item) = provider.preferred_item("general") else {
        return unavailable_line("MiniMax", provider, language);
    };
    if item.windows.is_empty() {
        return with_stale(
            format!("MiniMax: {}", i18n::text(language, key::USAGE_UNAVAILABLE)),
            provider,
            language,
        );
    }

    let current = item
        .windows
        .iter()
        .find(|window| window.label == "current")
        .or_else(|| item.windows.first());
    let weekly = item.windows.iter().find(|window| window.label == "week");
    let mut parts = Vec::new();
    if let Some(window) = current {
        parts.push(window_text(
            window,
            i18n::text(language, key::CURRENT),
            language,
        ));
    }
    if let Some(window) = weekly {
        parts.push(window_text(
            window,
            i18n::text(language, key::WEEK),
            language,
        ));
    }

    with_stale(
        format!("MiniMax: {}", parts.join(" | ")),
        provider,
        language,
    )
}

fn window_text(window: &UsageWindow, label: &str, language: Language) -> String {
    if window.unlimited {
        format!("{label} {}", i18n::text(language, key::UNLIMITED))
    } else if language.is_cjk() {
        format!("{label} {}%", window.remaining_percent)
    } else {
        format!("{}% {label}", window.remaining_percent)
    }
}

fn most_constrained_window(item: &UsageItem) -> Option<&UsageWindow> {
    item.windows
        .iter()
        .filter(|window| !window.unlimited)
        .min_by_key(|window| window.remaining_percent)
        .or_else(|| item.windows.first())
}

fn item_remaining(item: &UsageItem) -> Option<u8> {
    most_constrained_window(item).map(|window| {
        if window.unlimited {
            100
        } else {
            window.remaining_percent
        }
    })
}

fn current_remaining(item: &UsageItem) -> Option<u8> {
    item.windows
        .iter()
        .find(|window| window.label == "current")
        .or_else(|| most_constrained_window(item))
        .map(|window| {
            if window.unlimited {
                100
            } else {
                window.remaining_percent
            }
        })
}

fn with_stale(mut line: String, provider: &ProviderSnapshot, language: Language) -> String {
    let status = match provider.status {
        ProviderStatus::LoginRequired => Some(key::LOGIN_REQUIRED),
        ProviderStatus::ToolMissing => Some(key::CLI_MISSING),
        ProviderStatus::ApprovalRequired => Some(key::CLI_APPROVAL_REQUIRED),
        ProviderStatus::UnsafeTool => Some(key::UNSAFE_TOOL_PATH),
        ProviderStatus::Stale => Some(key::STALE),
        ProviderStatus::Waiting | ProviderStatus::Ready => None,
    };
    if let Some(status) = status {
        line.push_str(" (");
        line.push_str(i18n::text(language, status));
        line.push(')');
    }
    line
}

fn unavailable_line(name: &str, provider: &ProviderSnapshot, language: Language) -> String {
    let status = match provider.status {
        ProviderStatus::LoginRequired => key::LOGIN_REQUIRED,
        ProviderStatus::ToolMissing => key::CLI_MISSING,
        ProviderStatus::ApprovalRequired => key::CLI_APPROVAL_REQUIRED,
        ProviderStatus::UnsafeTool => key::UNSAFE_TOOL_PATH,
        ProviderStatus::Waiting | ProviderStatus::Ready | ProviderStatus::Stale => key::UNAVAILABLE,
    };
    format!("{name}: {}", i18n::text(language, status))
}

#[cfg(target_os = "windows")]
fn provider_is_stale(provider: &ProviderSnapshot) -> bool {
    matches!(
        provider.status,
        ProviderStatus::Stale
            | ProviderStatus::LoginRequired
            | ProviderStatus::ToolMissing
            | ProviderStatus::ApprovalRequired
            | ProviderStatus::UnsafeTool
    )
}

fn update_line(snapshot: &AppSnapshot, language: Language) -> String {
    let latest_success = snapshot
        .codex
        .fetched_at
        .into_iter()
        .chain(snapshot.minimax.fetched_at)
        .max();

    match latest_success {
        Some(timestamp) => format!(
            "{} {}",
            i18n::text(language, key::UPDATED),
            age_text(timestamp, language)
        ),
        None if snapshot.attempted_at.is_some() => {
            i18n::text(language, key::CHECKED_NO_DATA).into()
        }
        None => i18n::text(language, key::WAITING_FIRST_REFRESH).into(),
    }
}

fn reset_text(timestamp: Option<i64>, language: Language) -> Option<String> {
    let seconds = timestamp?.saturating_sub(unix_timestamp());
    if seconds <= 0 {
        Some(i18n::text(language, key::RESET_DUE).into())
    } else {
        Some(format!(
            "{} {}",
            i18n::text(language, key::RESETS_IN),
            compact_duration(seconds, language)
        ))
    }
}

fn age_text(timestamp: i64, language: Language) -> String {
    let seconds = unix_timestamp().saturating_sub(timestamp).max(0);
    if seconds < 10 {
        i18n::text(language, key::JUST_NOW).into()
    } else if matches!(
        language,
        Language::SimplifiedChinese | Language::TraditionalChinese | Language::Japanese
    ) {
        format!(
            "{}{}",
            compact_duration(seconds, language),
            i18n::text(language, key::AGO)
        )
    } else {
        format!(
            "{} {}",
            compact_duration(seconds, language),
            i18n::text(language, key::AGO)
        )
    }
}

fn compact_duration(seconds: i64, language: Language) -> String {
    if seconds >= 86_400 {
        let days = seconds / 86_400;
        let hours = (seconds % 86_400) / 3_600;
        if hours > 0 {
            format!(
                "{} {}",
                duration_part(days, key::UNIT_DAY, key::UNIT_DAYS, language),
                duration_part(hours, key::UNIT_HOUR, key::UNIT_HOURS, language)
            )
        } else {
            duration_part(days, key::UNIT_DAY, key::UNIT_DAYS, language)
        }
    } else if seconds >= 3_600 {
        let hours = seconds / 3_600;
        let minutes = (seconds % 3_600) / 60;
        if minutes > 0 {
            format!(
                "{} {}",
                duration_part(hours, key::UNIT_HOUR, key::UNIT_HOURS, language),
                duration_part(minutes, key::UNIT_MINUTE, key::UNIT_MINUTES, language)
            )
        } else {
            duration_part(hours, key::UNIT_HOUR, key::UNIT_HOURS, language)
        }
    } else if seconds >= 60 {
        duration_part(seconds / 60, key::UNIT_MINUTE, key::UNIT_MINUTES, language)
    } else {
        duration_part(seconds, key::UNIT_SECOND, key::UNIT_SECONDS, language)
    }
}

fn duration_part(
    count: i64,
    singular: &'static str,
    plural: &'static str,
    language: Language,
) -> String {
    let unit = i18n::text(language, if count == 1 { singular } else { plural });
    if language.is_cjk() {
        format!("{count}{unit}")
    } else {
        format!("{count} {unit}")
    }
}

fn short_percent(value: Option<u8>) -> String {
    value.map_or_else(|| "--".into(), |value| value.to_string())
}

fn truncate_chars(value: &str, max: usize) -> String {
    let mut chars = value.chars();
    let shortened: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        format!("{}...", shortened.trim_end())
    } else {
        shortened
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_compact_durations() {
        assert_eq!(compact_duration(45, Language::English), "45 seconds");
        assert_eq!(compact_duration(125, Language::English), "2 minutes");
        assert_eq!(
            compact_duration(7_500, Language::English),
            "2 hours 5 minutes"
        );
        assert_eq!(
            compact_duration(183_600, Language::English),
            "2 days 3 hours"
        );
        assert_eq!(
            compact_duration(7_500, Language::SimplifiedChinese),
            "2小时 5分钟"
        );
    }

    #[test]
    fn chooses_the_tightest_codex_window() {
        let item = UsageItem {
            id: "codex".into(),
            windows: vec![
                UsageWindow {
                    remaining_percent: 60,
                    ..UsageWindow::default()
                },
                UsageWindow {
                    remaining_percent: 25,
                    ..UsageWindow::default()
                },
            ],
            ..UsageItem::default()
        };
        assert_eq!(item_remaining(&item), Some(25));
    }

    #[test]
    fn truncation_respects_character_boundaries() {
        assert_eq!(truncate_chars("abcdef", 3), "abc...");
        assert_eq!(truncate_chars("用量正常", 2), "用量...");
    }

    #[test]
    fn authentication_failures_are_explicit() {
        let provider = ProviderSnapshot {
            status: ProviderStatus::LoginRequired,
            error: Some("Codex authentication required".into()),
            ..ProviderSnapshot::default()
        };
        assert_eq!(
            codex_line(&provider, Language::English),
            "Codex: login required"
        );
        assert_eq!(
            codex_line(&provider, Language::SimplifiedChinese),
            "Codex: 需要登录"
        );
    }

    #[test]
    fn provider_visibility_filters_tray_text_and_warning_color() {
        let provider = |id: &str, label: &str, remaining_percent| ProviderSnapshot {
            items: vec![UsageItem {
                id: id.into(),
                windows: vec![UsageWindow {
                    label: label.into(),
                    remaining_percent,
                    ..UsageWindow::default()
                }],
                ..UsageItem::default()
            }],
            status: ProviderStatus::Ready,
            ..ProviderSnapshot::default()
        };
        let snapshot = AppSnapshot {
            codex: provider("codex", "", 19),
            minimax: provider("general", "current", 81),
            ..AppSnapshot::default()
        };

        let codex_only = display(
            &snapshot,
            Language::English,
            ProviderVisibility {
                codex: true,
                minimax: false,
            },
        );
        assert!(codex_only.tooltip.contains("Codex"));
        assert!(!codex_only.tooltip.contains("MiniMax"));
        assert_eq!(codex_only.title, "C19");
        assert_eq!(codex_only.minimum_remaining, Some(19));

        let minimax_only = display(
            &snapshot,
            Language::English,
            ProviderVisibility {
                codex: false,
                minimax: true,
            },
        );
        assert_eq!(minimax_only.title, "M81");
        assert_eq!(minimax_only.minimum_remaining, Some(81));

        let hidden = display(
            &snapshot,
            Language::English,
            ProviderVisibility {
                codex: false,
                minimax: false,
            },
        );
        assert_eq!(hidden.tooltip, "SeeUsage");
        assert_eq!(hidden.title, "SeeUsage");
        assert_eq!(hidden.minimum_remaining, None);
    }
}
