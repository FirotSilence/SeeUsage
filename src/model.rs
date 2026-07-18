use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct UsageWindow {
    pub label: String,
    pub remaining_percent: u8,
    pub resets_at: Option<i64>,
    pub duration_minutes: Option<u64>,
    pub unlimited: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct UsageItem {
    pub id: String,
    pub name: String,
    pub windows: Vec<UsageWindow>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ProviderSnapshot {
    pub fetched_at: Option<i64>,
    pub items: Vec<UsageItem>,
    pub status: ProviderStatus,
    #[serde(skip)]
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderStatus {
    #[default]
    Waiting,
    Ready,
    Stale,
    LoginRequired,
    ToolMissing,
    ApprovalRequired,
    UnsafeTool,
}

impl ProviderSnapshot {
    pub fn preferred_item(&self, id: &str) -> Option<&UsageItem> {
        self.items
            .iter()
            .find(|item| item.id.eq_ignore_ascii_case(id))
            .or_else(|| self.items.first())
    }

    pub fn minimum_remaining(&self) -> Option<u8> {
        self.items
            .iter()
            .flat_map(|item| item.windows.iter())
            .filter(|window| !window.unlimited)
            .map(|window| window.remaining_percent)
            .min()
            .or_else(|| {
                self.items
                    .iter()
                    .flat_map(|item| item.windows.iter())
                    .any(|window| window.unlimited)
                    .then_some(100)
            })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct AppSnapshot {
    pub schema_version: u8,
    pub attempted_at: Option<i64>,
    pub codex: ProviderSnapshot,
    pub minimax: ProviderSnapshot,
}

impl Default for AppSnapshot {
    fn default() -> Self {
        Self {
            schema_version: 1,
            attempted_at: None,
            codex: ProviderSnapshot::default(),
            minimax: ProviderSnapshot::default(),
        }
    }
}

impl AppSnapshot {
    pub fn minimum_remaining(&self) -> Option<u8> {
        self.codex
            .minimum_remaining()
            .into_iter()
            .chain(self.minimax.minimum_remaining())
            .min()
    }
}

pub fn unix_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimum_ignores_unlimited_when_a_real_limit_exists() {
        let snapshot = ProviderSnapshot {
            items: vec![UsageItem {
                windows: vec![
                    UsageWindow {
                        remaining_percent: 100,
                        unlimited: true,
                        ..UsageWindow::default()
                    },
                    UsageWindow {
                        remaining_percent: 17,
                        ..UsageWindow::default()
                    },
                ],
                ..UsageItem::default()
            }],
            ..ProviderSnapshot::default()
        };

        assert_eq!(snapshot.minimum_remaining(), Some(17));
    }

    #[test]
    fn transient_errors_are_never_serialized() {
        let snapshot = ProviderSnapshot {
            error: Some("Bearer secret-value".into()),
            ..ProviderSnapshot::default()
        };
        let json = serde_json::to_string(&snapshot).unwrap();
        assert!(!json.contains("secret-value"));
        assert!(!json.contains("error"));
    }
}
