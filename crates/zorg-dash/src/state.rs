use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::model::{AutoRefreshConfig, DashboardFrame, PanelId, SearchHistory};

const STATE_SCHEMA: &str = "zorg.dash.state";
const STATE_VERSION: u32 = 1;
const MAX_SEARCH_HISTORY: usize = 50;

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct RestoredDashboardState {
    pub(crate) selected_panel: Option<PanelId>,
    pub(crate) search_query: Option<String>,
    pub(crate) search_history: Vec<String>,
    pub(crate) selected_dashboard_id: Option<String>,
    pub(crate) mouse: Option<bool>,
    pub(crate) auto_refresh: Option<AutoRefreshConfig>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum StateLoadOutcome {
    Missing,
    Restored(RestoredDashboardState),
    Ignored(String),
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub(crate) struct PersistedDashboardState {
    schema: String,
    version: u32,
    selected_panel: Option<String>,
    search_query: Option<String>,
    search_history: Vec<String>,
    selected_dashboard_id: Option<String>,
    preferences: PersistedDashboardPreferences,
}

#[derive(Debug, Clone, Eq, PartialEq, Default, Deserialize, Serialize)]
struct PersistedDashboardPreferences {
    mouse: Option<bool>,
    auto_refresh_ms: Option<u64>,
}

impl PersistedDashboardState {
    pub(crate) fn from_frame(
        frame: &DashboardFrame,
        search_history: &SearchHistory,
        mouse: bool,
    ) -> Self {
        Self {
            schema: STATE_SCHEMA.to_owned(),
            version: STATE_VERSION,
            selected_panel: Some(frame.active_panel_id().key().to_owned()),
            search_query: frame.query.clone(),
            search_history: search_history.entries().to_vec(),
            selected_dashboard_id: frame.requested_dashboard_id.clone(),
            preferences: PersistedDashboardPreferences {
                mouse: Some(mouse),
                auto_refresh_ms: frame
                    .auto_refresh
                    .config
                    .map(|config| saturating_millis(config.interval)),
            },
        }
    }

    fn into_restored(self) -> Result<RestoredDashboardState, String> {
        if self.schema != STATE_SCHEMA {
            return Err(format!(
                "unsupported dashboard state schema `{}`",
                self.schema
            ));
        }
        if self.version != STATE_VERSION {
            return Err(format!(
                "unsupported dashboard state version {}; expected {}",
                self.version, STATE_VERSION
            ));
        }

        let selected_panel = self
            .selected_panel
            .as_deref()
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .map(PanelId::from_key);
        let search_query = non_empty_trimmed(self.search_query);
        let selected_dashboard_id = non_empty_trimmed(self.selected_dashboard_id);
        let search_history = self
            .search_history
            .into_iter()
            .filter_map(|entry| non_empty_trimmed(Some(entry)))
            .rev()
            .take(MAX_SEARCH_HISTORY)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let auto_refresh = self
            .preferences
            .auto_refresh_ms
            .filter(|millis| *millis > 0)
            .map(|millis| AutoRefreshConfig::new(Duration::from_millis(millis)));

        Ok(RestoredDashboardState {
            selected_panel,
            search_query,
            search_history,
            selected_dashboard_id,
            mouse: self.preferences.mouse,
            auto_refresh,
        })
    }
}

pub(crate) fn default_state_path() -> Result<PathBuf, String> {
    if let Some(state_home) = env::var_os("XDG_STATE_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(state_home).join("zorg/dash/state.json"));
    }
    if let Some(home) = env::var_os("HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(home).join(".local/state/zorg/dash/state.json"));
    }
    Err("cannot resolve dashboard state path: set XDG_STATE_HOME or HOME".to_owned())
}

pub(crate) fn load(path: &Path) -> StateLoadOutcome {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return StateLoadOutcome::Missing,
        Err(error) => {
            return StateLoadOutcome::Ignored(format!(
                "failed to read dashboard state {}: {error}",
                path.display()
            ));
        }
    };
    let persisted = match serde_json::from_slice::<PersistedDashboardState>(&bytes) {
        Ok(state) => state,
        Err(error) => {
            return StateLoadOutcome::Ignored(format!(
                "failed to parse dashboard state {}: {error}",
                path.display()
            ));
        }
    };
    match persisted.into_restored() {
        Ok(restored) => StateLoadOutcome::Restored(restored),
        Err(message) => StateLoadOutcome::Ignored(message),
    }
}

pub(crate) fn save(path: &Path, state: &PersistedDashboardState) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create dashboard state directory {}: {error}",
                parent.display()
            )
        })?;
    }
    let bytes = serde_json::to_vec_pretty(state)
        .map_err(|error| format!("failed to serialize dashboard state: {error}"))?;
    fs::write(path, bytes).map_err(|error| {
        format!(
            "failed to write dashboard state {}: {error}",
            path.display()
        )
    })
}

fn non_empty_trimmed(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn saturating_millis(duration: Duration) -> u64 {
    duration.as_millis().try_into().unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_state_is_not_an_error() {
        let temp = tempfile::tempdir().expect("tempdir");
        let outcome = load(&temp.path().join("missing.json"));
        assert_eq!(outcome, StateLoadOutcome::Missing);
    }

    #[test]
    fn corrupt_state_is_ignored() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("state.json");
        fs::write(&path, b"{not-json").expect("write corrupt state");

        let outcome = load(&path);
        assert!(
            matches!(outcome, StateLoadOutcome::Ignored(message) if message.contains("failed to parse"))
        );
    }

    #[test]
    fn version_mismatch_is_ignored() {
        let state = PersistedDashboardState {
            schema: STATE_SCHEMA.to_owned(),
            version: STATE_VERSION + 1,
            selected_panel: Some("search".to_owned()),
            search_query: None,
            search_history: Vec::new(),
            selected_dashboard_id: None,
            preferences: PersistedDashboardPreferences::default(),
        };

        let error = state.into_restored().expect_err("version mismatch");
        assert!(error.contains("unsupported dashboard state version"));
    }

    #[test]
    fn save_and_load_round_trip_restored_state() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("nested/state.json");
        let state = PersistedDashboardState {
            schema: STATE_SCHEMA.to_owned(),
            version: STATE_VERSION,
            selected_panel: Some("open".to_owned()),
            search_query: Some(" #z/inbox ".to_owned()),
            search_history: vec!["".to_owned(), "#z/todo".to_owned()],
            selected_dashboard_id: Some(" @dashboards/daily ".to_owned()),
            preferences: PersistedDashboardPreferences {
                mouse: Some(true),
                auto_refresh_ms: Some(2_000),
            },
        };

        save(&path, &state).expect("save state");
        let StateLoadOutcome::Restored(restored) = load(&path) else {
            panic!("expected restored state");
        };

        assert_eq!(
            restored.selected_panel,
            Some(PanelId::Custom("open".to_owned()))
        );
        assert_eq!(restored.search_query.as_deref(), Some("#z/inbox"));
        assert_eq!(restored.search_history, vec!["#z/todo".to_owned()]);
        assert_eq!(
            restored.selected_dashboard_id.as_deref(),
            Some("@dashboards/daily")
        );
        assert_eq!(restored.mouse, Some(true));
        assert_eq!(
            restored.auto_refresh.map(|config| config.interval),
            Some(Duration::from_millis(2_000))
        );
    }
}
