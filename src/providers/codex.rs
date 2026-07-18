use crate::{
    command::{
        ManagedChild, ProviderEnvironment, ToolCommand, read_capped, safe_command_failure,
        safe_provider_failure,
    },
    model::{UsageItem, UsageWindow},
};
use serde_json::{Value, json};
use std::{
    io::{self, BufRead, BufReader, Read, Write},
    process::Stdio,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

const TIMEOUT: Duration = Duration::from_secs(15);
const MAX_PROTOCOL_LINE_BYTES: usize = 1024 * 1024;

pub fn query(interactive_approval: bool) -> Result<Vec<UsageItem>, String> {
    let result = request_rate_limits(interactive_approval)?;
    parse_rate_limits(&result)
}

fn request_rate_limits(interactive_approval: bool) -> Result<Value, String> {
    let tool = ToolCommand::resolve(
        "SEEUSAGE_CODEX_BIN",
        "codex",
        ProviderEnvironment::Codex,
        interactive_approval,
    )?;
    let mut command = tool.command(["app-server", "--listen", "stdio://"])?;
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = ManagedChild::spawn(&mut command)
        .map_err(|error| format!("could not start Codex: {error}"))?;
    let mut stdin = child
        .child
        .stdin
        .take()
        .ok_or_else(|| "could not open Codex input".to_string())?;
    let stdout = child
        .child
        .stdout
        .take()
        .ok_or_else(|| "could not read Codex output".to_string())?;
    let mut stderr = child
        .child
        .stderr
        .take()
        .ok_or_else(|| "could not read Codex errors".to_string())?;

    let (line_tx, line_rx) = mpsc::sync_channel(32);
    let stdout_reader = thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            match read_protocol_line(&mut reader) {
                Ok(Some(line)) => {
                    if line_tx.send(Ok(line)).is_err() {
                        break;
                    }
                }
                Ok(None) => break,
                Err(error) => {
                    let _ = line_tx.send(Err(error));
                    break;
                }
            }
        }
    });
    let stderr_reader = thread::spawn(move || read_capped(&mut stderr));

    let started = Instant::now();
    let response = (|| {
        send(
            &mut stdin,
            &json!({
                "id": 1,
                "method": "initialize",
                "params": {
                    "clientInfo": {
                        "name": "seeusage",
                        "title": "SeeUsage",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }
            }),
        )?;

        let initialize = receive_response(&line_rx, 1, started)?;
        rpc_result(initialize, "Codex initialization")?;

        send(
            &mut stdin,
            &json!({ "method": "initialized", "params": {} }),
        )?;
        send(
            &mut stdin,
            &json!({ "id": 2, "method": "account/rateLimits/read" }),
        )?;
        receive_response(&line_rx, 2, started)
    })();
    drop(stdin);
    child.terminate();
    drop(line_rx);
    let _ = stdout_reader.join();
    let stderr = stderr_reader.join().unwrap_or_default();

    match response {
        Ok(response) => rpc_result(response, "Codex usage request"),
        Err(_) if !stderr.is_empty() => Err(safe_command_failure("Codex", None, &stderr)),
        Err(error) => Err(error),
    }
}

fn read_protocol_line(reader: &mut impl BufRead) -> io::Result<Option<String>> {
    let mut bytes = Vec::new();
    let count = Read::by_ref(reader)
        .take((MAX_PROTOCOL_LINE_BYTES + 1) as u64)
        .read_until(b'\n', &mut bytes)?;
    if count == 0 {
        return Ok(None);
    }
    if bytes.len() > MAX_PROTOCOL_LINE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Codex response line exceeded the safety limit",
        ));
    }
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
        if bytes.last() == Some(&b'\r') {
            bytes.pop();
        }
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Codex returned invalid UTF-8"))
}

fn send(stdin: &mut impl Write, message: &Value) -> Result<(), String> {
    writeln!(stdin, "{message}").map_err(|error| format!("could not write to Codex: {error}"))?;
    stdin
        .flush()
        .map_err(|error| format!("could not flush Codex input: {error}"))
}

fn receive_response(
    receiver: &mpsc::Receiver<std::io::Result<String>>,
    expected_id: u64,
    started: Instant,
) -> Result<Value, String> {
    loop {
        let remaining = TIMEOUT
            .checked_sub(started.elapsed())
            .ok_or_else(|| format!("Codex timed out after {}s", TIMEOUT.as_secs()))?;
        let line = receiver
            .recv_timeout(remaining)
            .map_err(|_| format!("Codex timed out after {}s", TIMEOUT.as_secs()))?
            .map_err(|error| format!("could not read Codex output: {error}"))?;
        let message: Value = serde_json::from_str(&line)
            .map_err(|error| format!("Codex returned invalid JSON: {error}"))?;
        if message.get("id").and_then(Value::as_u64) == Some(expected_id) {
            return Ok(message);
        }
    }
}

fn rpc_result(mut response: Value, operation: &str) -> Result<Value, String> {
    if let Some(error) = response.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unknown Codex error");
        return Err(safe_provider_failure("Codex", operation, message));
    }
    response
        .get_mut("result")
        .map(Value::take)
        .ok_or_else(|| format!("{operation} returned no result"))
}

pub(crate) fn parse_rate_limits(result: &Value) -> Result<Vec<UsageItem>, String> {
    let mut items = Vec::new();
    if let Some(limits) = result.get("rateLimitsByLimitId").and_then(Value::as_object) {
        let mut entries: Vec<_> = limits.iter().collect();
        entries.sort_by_key(|(id, _)| if id.as_str() == "codex" { 0 } else { 1 });
        for (id, snapshot) in entries {
            if let Some(item) = parse_limit(id, snapshot) {
                items.push(item);
            }
        }
    }

    if items.is_empty()
        && let Some(snapshot) = result.get("rateLimits")
        && let Some(item) = parse_limit("codex", snapshot)
    {
        items.push(item);
    }

    if items.is_empty() {
        Err("Codex returned no rate-limit windows".into())
    } else {
        Ok(items)
    }
}

fn parse_limit(fallback_id: &str, snapshot: &Value) -> Option<UsageItem> {
    let id = snapshot
        .get("limitId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback_id)
        .to_string();
    let name = snapshot
        .get("limitName")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| default_codex_name(&id));
    let mut windows = Vec::new();
    for field in ["primary", "secondary"] {
        if let Some(window) = snapshot.get(field).and_then(Value::as_object) {
            let used = window
                .get("usedPercent")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                .min(100) as u8;
            let duration_minutes = window.get("windowDurationMins").and_then(Value::as_u64);
            windows.push(UsageWindow {
                label: duration_label(duration_minutes, field),
                remaining_percent: 100u8.saturating_sub(used),
                resets_at: window.get("resetsAt").and_then(Value::as_i64),
                duration_minutes,
                unlimited: false,
            });
        }
    }

    if let Some(limit) = snapshot.get("individualLimit").and_then(Value::as_object) {
        windows.push(UsageWindow {
            label: "spend".into(),
            remaining_percent: limit
                .get("remainingPercent")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                .min(100) as u8,
            resets_at: limit.get("resetsAt").and_then(Value::as_i64),
            duration_minutes: None,
            unlimited: false,
        });
    }

    let unlimited = snapshot
        .pointer("/credits/unlimited")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if windows.is_empty() && unlimited {
        windows.push(UsageWindow {
            label: "unlimited".into(),
            remaining_percent: 100,
            unlimited: true,
            ..UsageWindow::default()
        });
    }
    if windows.is_empty()
        && snapshot
            .get("rateLimitReachedType")
            .is_some_and(|value| !value.is_null())
    {
        windows.push(UsageWindow {
            label: "limit".into(),
            remaining_percent: 0,
            ..UsageWindow::default()
        });
    }

    (!windows.is_empty()).then_some(UsageItem { id, name, windows })
}

fn duration_label(duration_minutes: Option<u64>, fallback: &str) -> String {
    match duration_minutes {
        Some(minutes) if minutes % 10_080 == 0 => format!("{}w", minutes / 10_080),
        Some(minutes) if minutes % 1_440 == 0 => format!("{}d", minutes / 1_440),
        Some(minutes) if minutes % 60 == 0 => format!("{}h", minutes / 60),
        Some(minutes) => format!("{minutes}m"),
        None => fallback.into(),
    }
}

fn default_codex_name(id: &str) -> String {
    if id == "codex" {
        "Codex".into()
    } else {
        id.replace('_', " ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multi_bucket_rate_limits() {
        let value: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/codex_rate_limits.json"))
                .unwrap();
        let items = parse_rate_limits(&value).unwrap();

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id, "codex");
        assert_eq!(items[0].windows[0].remaining_percent, 49);
        assert_eq!(items[0].windows[0].label, "1w");
        assert_eq!(items[1].name, "Additional Codex quota");
    }

    #[test]
    fn falls_back_to_legacy_bucket() {
        let value = json!({
            "rateLimits": {
                "primary": { "usedPercent": 12, "windowDurationMins": 300 }
            }
        });
        let items = parse_rate_limits(&value).unwrap();
        assert_eq!(items[0].windows[0].remaining_percent, 88);
        assert_eq!(items[0].windows[0].label, "5h");
    }
}
