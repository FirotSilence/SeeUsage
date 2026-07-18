mod codex;
mod minimax;

use crate::{
    command::contains_sensitive_marker,
    model::{AppSnapshot, ProviderSnapshot, ProviderStatus, UsageItem, unix_timestamp},
};

pub fn refresh(previous: AppSnapshot, interactive_approval: bool) -> AppSnapshot {
    let attempted_at = unix_timestamp();
    let codex = merge_provider(
        previous.codex,
        codex::query(interactive_approval),
        attempted_at,
    );
    let minimax = merge_provider(
        previous.minimax,
        minimax::query(interactive_approval),
        attempted_at,
    );
    AppSnapshot {
        schema_version: 1,
        attempted_at: Some(attempted_at),
        codex,
        minimax,
    }
}

fn merge_provider(
    mut previous: ProviderSnapshot,
    result: Result<Vec<UsageItem>, String>,
    attempted_at: i64,
) -> ProviderSnapshot {
    match result {
        Ok(items) => ProviderSnapshot {
            fetched_at: Some(attempted_at),
            items,
            status: ProviderStatus::Ready,
            error: None,
        },
        Err(error) => {
            let error = sanitize_error(&error);
            previous.status = error_status(&error, !previous.items.is_empty());
            previous.error = Some(error);
            previous
        }
    }
}

fn error_status(error: &str, has_cached_data: bool) -> ProviderStatus {
    if error.contains("authentication required") {
        ProviderStatus::LoginRequired
    } else if error.contains("requires approval") {
        ProviderStatus::ApprovalRequired
    } else if error.contains("refusing broadly writable path")
        || error.contains("PowerShell tool wrappers are not supported")
    {
        ProviderStatus::UnsafeTool
    } else if error.contains("could not find") {
        ProviderStatus::ToolMissing
    } else if has_cached_data {
        ProviderStatus::Stale
    } else {
        ProviderStatus::Waiting
    }
}

fn sanitize_error(error: &str) -> String {
    let sanitized: String = error
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(240)
        .collect();
    if contains_sensitive_marker(&sanitized) {
        "Provider request failed; sensitive details withheld".into()
    } else {
        sanitized
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::UsageItem;

    #[test]
    fn failed_refresh_preserves_last_good_data() {
        let previous = ProviderSnapshot {
            fetched_at: Some(10),
            items: vec![UsageItem {
                id: "cached".into(),
                ..UsageItem::default()
            }],
            status: ProviderStatus::Ready,
            error: None,
        };
        let merged = merge_provider(previous, Err("offline".into()), 20);
        assert_eq!(merged.fetched_at, Some(10));
        assert_eq!(merged.items[0].id, "cached");
        assert_eq!(merged.error.as_deref(), Some("offline"));
        assert_eq!(merged.status, ProviderStatus::Stale);
    }

    #[test]
    fn stored_errors_withhold_sensitive_details() {
        assert_eq!(
            sanitize_error("authorization: Bearer secret-value"),
            "Provider request failed; sensitive details withheld"
        );
    }
}
