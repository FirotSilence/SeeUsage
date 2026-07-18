use crate::{
    command::{ProviderEnvironment, ToolCommand, safe_command_failure, safe_provider_failure},
    model::{UsageItem, UsageWindow},
};
use serde_json::Value;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(20);

pub fn query(interactive_approval: bool) -> Result<Vec<UsageItem>, String> {
    let tool = ToolCommand::resolve(
        "SEEUSAGE_MMX_BIN",
        "mmx",
        ProviderEnvironment::MiniMax,
        interactive_approval,
    )?;
    let output = tool.run(
        [
            "quota",
            "show",
            "--output",
            "json",
            "--quiet",
            "--non-interactive",
        ],
        TIMEOUT,
    )?;
    if !output.success {
        return Err(safe_command_failure("MiniMax", output.code, &output.stderr));
    }
    let value: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("MiniMax returned invalid JSON: {error}"))?;
    parse_quota(&value)
}

pub(crate) fn parse_quota(value: &Value) -> Result<Vec<UsageItem>, String> {
    if let Some(code) = value
        .pointer("/base_resp/status_code")
        .and_then(Value::as_i64)
        && code != 0
    {
        let message = value
            .pointer("/base_resp/status_msg")
            .and_then(Value::as_str)
            .unwrap_or("unknown MiniMax error");
        return Err(safe_provider_failure(
            "MiniMax",
            "MiniMax quota request",
            message,
        ));
    }

    let remains = value
        .get("model_remains")
        .and_then(Value::as_array)
        .ok_or_else(|| "MiniMax returned no quota list".to_string())?;
    let mut items = Vec::new();
    for model in remains {
        let Some(id) = model.get("model_name").and_then(Value::as_str) else {
            continue;
        };
        let current = remaining_percent(
            model,
            "current_interval_remaining_percent",
            "current_interval_total_count",
            "current_interval_usage_count",
        );
        let weekly = remaining_percent(
            model,
            "current_weekly_remaining_percent",
            "current_weekly_total_count",
            "current_weekly_usage_count",
        );
        let mut windows = Vec::new();
        if let Some(remaining_percent) = current {
            windows.push(UsageWindow {
                label: "current".into(),
                remaining_percent,
                resets_at: timestamp_seconds(model.get("end_time").and_then(Value::as_i64)),
                duration_minutes: duration_minutes(model, "start_time", "end_time"),
                unlimited: false,
            });
        }
        if let Some(remaining_percent) = weekly {
            windows.push(UsageWindow {
                label: "week".into(),
                remaining_percent,
                resets_at: timestamp_seconds(model.get("weekly_end_time").and_then(Value::as_i64)),
                duration_minutes: duration_minutes(model, "weekly_start_time", "weekly_end_time"),
                unlimited: false,
            });
        }
        if !windows.is_empty() {
            items.push(UsageItem {
                id: id.to_string(),
                name: display_name(id),
                windows,
            });
        }
    }

    items.sort_by_key(|item| if item.id == "general" { 0 } else { 1 });
    if items.is_empty() {
        Err("MiniMax returned no usable quota windows".into())
    } else {
        Ok(items)
    }
}

fn remaining_percent(
    value: &Value,
    percent_key: &str,
    total_key: &str,
    used_key: &str,
) -> Option<u8> {
    if let Some(percent) = value.get(percent_key).and_then(Value::as_u64) {
        return Some(percent.min(100) as u8);
    }
    let total = value.get(total_key).and_then(Value::as_u64)?;
    let used = value.get(used_key).and_then(Value::as_u64)?;
    if total == 0 {
        return None;
    }
    Some((((total.saturating_sub(used)) * 100) / total).min(100) as u8)
}

fn timestamp_seconds(timestamp: Option<i64>) -> Option<i64> {
    timestamp.map(|value| {
        if value > 10_000_000_000 {
            value / 1_000
        } else {
            value
        }
    })
}

fn duration_minutes(value: &Value, start_key: &str, end_key: &str) -> Option<u64> {
    let start = value.get(start_key).and_then(Value::as_i64)?;
    let end = value.get(end_key).and_then(Value::as_i64)?;
    let milliseconds = end.checked_sub(start)?;
    (milliseconds > 0).then_some(milliseconds as u64 / 60_000)
}

fn display_name(id: &str) -> String {
    let mut chars = id.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => "MiniMax".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_current_and_weekly_quota() {
        let value: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/minimax_quota.json")).unwrap();
        let items = parse_quota(&value).unwrap();

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id, "general");
        assert_eq!(items[0].windows[0].remaining_percent, 85);
        assert_eq!(items[0].windows[1].remaining_percent, 100);
        assert_eq!(items[0].windows[0].resets_at, Some(1_893_456_000));
    }

    #[test]
    fn computes_percentage_when_the_api_omits_it() {
        let value = serde_json::json!({
            "current_interval_total_count": 20,
            "current_interval_usage_count": 5
        });
        assert_eq!(
            remaining_percent(
                &value,
                "current_interval_remaining_percent",
                "current_interval_total_count",
                "current_interval_usage_count"
            ),
            Some(75)
        );
    }
}
