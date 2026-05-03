use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum Panel {
    Today,
    Inbox,
    Search,
    Diagnostics,
    Index,
}

impl Panel {
    pub(crate) const ALL: [Self; 5] = [
        Self::Today,
        Self::Inbox,
        Self::Search,
        Self::Diagnostics,
        Self::Index,
    ];

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Today => "Today",
            Self::Inbox => "Inbox",
            Self::Search => "Search",
            Self::Diagnostics => "Diagnostics",
            Self::Index => "Index",
        }
    }

    pub(crate) const fn value(self) -> &'static str {
        match self {
            Self::Today => "today",
            Self::Inbox => "inbox",
            Self::Search => "search",
            Self::Diagnostics => "diagnostics",
            Self::Index => "index",
        }
    }

    pub(crate) const fn index(self) -> usize {
        match self {
            Self::Today => 0,
            Self::Inbox => 1,
            Self::Search => 2,
            Self::Diagnostics => 3,
            Self::Index => 4,
        }
    }

    pub(crate) fn next(self) -> Self {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    pub(crate) fn previous(self) -> Self {
        Self::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct TodayQuerySpec {
    pub(crate) label: &'static str,
    pub(crate) query: &'static str,
}

pub(crate) const TODAY_QUERY_SPECS: &[TodayQuerySpec] = &[
    TodayQuerySpec {
        label: "due",
        query: "#z/todo due:<=today -did:*",
    },
    TodayQuerySpec {
        label: "do",
        query: "#z/todo do:<=today -did:*",
    },
    TodayQuerySpec {
        label: "todo",
        query: "#z/todo todo:[ ] -did:*",
    },
];

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct DashboardFrame {
    pub(crate) root: PathBuf,
    pub(crate) database_path: PathBuf,
    pub(crate) panel: Panel,
    pub(crate) query: Option<String>,
    pub(crate) snapshot: DashboardSnapshot,
}

impl DashboardFrame {
    pub(crate) fn new(
        root: PathBuf,
        database_path: PathBuf,
        panel: Panel,
        query: Option<String>,
        snapshot: DashboardSnapshot,
    ) -> Self {
        Self {
            root,
            database_path,
            panel,
            query,
            snapshot,
        }
    }

    #[cfg(test)]
    pub(crate) fn is_ready(&self) -> bool {
        matches!(self.snapshot, DashboardSnapshot::Ready { .. })
    }

    pub(crate) fn health_label(&self) -> &'static str {
        match &self.snapshot {
            DashboardSnapshot::Degraded { .. } => "degraded",
            DashboardSnapshot::Ready { index, .. } => index.health_label(),
        }
    }

    pub(crate) fn diagnostics_label(&self) -> String {
        match &self.snapshot {
            DashboardSnapshot::Ready { index, .. } => index.diagnostic_count.to_string(),
            DashboardSnapshot::Degraded { .. } => "unknown".to_owned(),
        }
    }

    pub(crate) fn active_rows(&self) -> Vec<PanelRow> {
        match &self.snapshot {
            DashboardSnapshot::Degraded { .. } => Vec::new(),
            DashboardSnapshot::Ready {
                index,
                diagnostics,
                today,
                inbox,
                search,
            } => match self.panel {
                Panel::Today => today.clone(),
                Panel::Inbox => inbox.iter().cloned().map(PanelRow::Zettel).collect(),
                Panel::Search => search.iter().cloned().map(PanelRow::Zettel).collect(),
                Panel::Diagnostics => diagnostics
                    .iter()
                    .cloned()
                    .map(PanelRow::Diagnostic)
                    .collect(),
                Panel::Index => index
                    .rows
                    .iter()
                    .cloned()
                    .map(PanelRow::IndexStatus)
                    .collect(),
            },
        }
    }

    pub(crate) fn set_snapshot(&mut self, snapshot: DashboardSnapshot) {
        self.snapshot = snapshot;
    }

    pub(crate) fn selected_source_location(&self, selected_index: usize) -> Option<SourceLocation> {
        self.active_rows()
            .get(selected_index)
            .and_then(|row| row.source_location(&self.root))
    }

    pub(crate) fn inspector_lines_for_selection(&self, selected_index: usize) -> Vec<String> {
        match &self.snapshot {
            DashboardSnapshot::Degraded { message } => vec![
                "Read-only index unavailable".to_owned(),
                String::new(),
                message.clone(),
            ],
            DashboardSnapshot::Ready { index, .. } if self.panel == Panel::Index => {
                index.inspector_lines()
            }
            DashboardSnapshot::Ready { .. } => self
                .active_rows()
                .get(selected_index)
                .map(PanelRow::inspector_lines)
                .unwrap_or_else(|| {
                    vec![
                        format!("{} panel", self.panel.label()),
                        String::new(),
                        "No rows to inspect.".to_owned(),
                    ]
                }),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum DashboardSnapshot {
    Ready {
        index: IndexPanel,
        diagnostics: Vec<DiagnosticRow>,
        today: Vec<PanelRow>,
        inbox: Vec<ZettelRow>,
        search: Vec<ZettelRow>,
    },
    Degraded {
        message: String,
    },
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct IndexPanel {
    pub(crate) schema_version: i64,
    pub(crate) rows: Vec<IndexStatusRow>,
    pub(crate) discovered_files: usize,
    pub(crate) indexed_files: usize,
    pub(crate) changed_files: usize,
    pub(crate) new_files: usize,
    pub(crate) deleted_files: usize,
    pub(crate) diagnostic_count: usize,
    pub(crate) last_indexed_at_unix_ms: Option<i64>,
}

impl IndexPanel {
    pub(crate) fn health_label(&self) -> &'static str {
        if self.last_indexed_at_unix_ms.is_none() {
            "missing"
        } else if self.changed_files > 0 || self.new_files > 0 || self.deleted_files > 0 {
            "stale"
        } else {
            "current"
        }
    }

    pub(crate) fn inspector_lines(&self) -> Vec<String> {
        let mut lines = vec![
            "Index metadata".to_owned(),
            format!("Schema version: {}", self.schema_version),
            format!("Health: {}", self.health_label()),
            format!(
                "Last indexed: {}",
                self.last_indexed_at_unix_ms
                    .map(|timestamp| timestamp.to_string())
                    .unwrap_or_else(|| "never".to_owned())
            ),
        ];
        lines.extend(
            self.rows
                .iter()
                .map(|row| format!("{}: {}", row.label, row.value)),
        );
        lines
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct IndexStatusRow {
    pub(crate) label: String,
    pub(crate) value: usize,
}

impl IndexStatusRow {
    pub(crate) fn new(label: impl Into<String>, value: usize) -> Self {
        Self {
            label: label.into(),
            value,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum PanelRow {
    Zettel(ZettelRow),
    Diagnostic(DiagnosticRow),
    IndexStatus(IndexStatusRow),
}

impl PanelRow {
    pub(crate) fn sort_key(&self) -> (u8, String, usize) {
        match self {
            Self::Zettel(row) => (
                0,
                format!(
                    "{}:{}:{}",
                    row.lifecycle_date.as_deref().unwrap_or("9999-12-31"),
                    normalize_path(&row.file_path),
                    row.start_line.unwrap_or(usize::MAX)
                ),
                row.store_id as usize,
            ),
            Self::Diagnostic(row) => (
                1,
                format!(
                    "{}:{}:{}",
                    row.severity_rank(),
                    row.relative_path
                        .as_ref()
                        .map(|path| normalize_path(path))
                        .unwrap_or_default(),
                    row.message
                ),
                row.id as usize,
            ),
            Self::IndexStatus(row) => (2, row.label.clone(), row.value),
        }
    }

    pub(crate) fn list_line(&self) -> String {
        match self {
            Self::Zettel(row) => row.list_line(),
            Self::Diagnostic(row) => row.list_line(),
            Self::IndexStatus(row) => format!("{:<18} {}", row.label, row.value),
        }
    }

    fn inspector_lines(&self) -> Vec<String> {
        match self {
            Self::Zettel(row) => row.inspector_lines(),
            Self::Diagnostic(row) => row.inspector_lines(),
            Self::IndexStatus(row) => vec![row.label.clone(), format!("Value: {}", row.value)],
        }
    }

    fn source_location(&self, root: &std::path::Path) -> Option<SourceLocation> {
        match self {
            Self::Zettel(row) => Some(SourceLocation {
                path: row.file_path.clone(),
                line: row.start_line,
                column: row.start_column,
            }),
            Self::Diagnostic(row) => row.relative_path.as_ref().map(|path| SourceLocation {
                path: root.join(path),
                line: row.start_line,
                column: row.start_column,
            }),
            Self::IndexStatus(_) => None,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct SourceLocation {
    pub(crate) path: PathBuf,
    pub(crate) line: Option<usize>,
    pub(crate) column: Option<usize>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum DashboardOverlay {
    None,
    Help,
    ConfirmReindex,
    Log { title: String, message: String },
}

impl DashboardOverlay {
    pub(crate) fn is_confirming_reindex(&self) -> bool {
        matches!(self, Self::ConfirmReindex)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct QueryBadge {
    pub(crate) label: String,
    pub(crate) query: String,
}

impl QueryBadge {
    pub(crate) fn new(label: &str, query: &str) -> Self {
        Self {
            label: label.to_owned(),
            query: query.to_owned(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct TodayQuery {
    pub(crate) label: String,
    pub(crate) query: String,
    pub(crate) count: usize,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct ZettelRow {
    pub(crate) store_id: i64,
    pub(crate) canonical_id: Option<String>,
    pub(crate) file_path: PathBuf,
    pub(crate) title: String,
    pub(crate) todo_marker: Option<String>,
    pub(crate) start_line: Option<usize>,
    pub(crate) start_column: Option<usize>,
    pub(crate) lifecycle_date: Option<String>,
    pub(crate) tags: Vec<String>,
    pub(crate) properties: Vec<(String, String)>,
    pub(crate) preview: Option<String>,
    pub(crate) badges: Vec<QueryBadge>,
}

impl ZettelRow {
    fn list_line(&self) -> String {
        let marker = self.todo_marker.as_deref().unwrap_or("   ");
        let id = self
            .canonical_id
            .as_deref()
            .map(|id| format!("@{id}"))
            .unwrap_or_else(|| "-".to_owned());
        let badges = if self.badges.is_empty() {
            String::new()
        } else {
            format!(
                " [{}]",
                self.badges
                    .iter()
                    .map(|badge| badge.label.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            )
        };
        format!(
            "{marker:<3} {id:<24} {:<20} {}{}",
            normalize_path(&self.file_path),
            self.title,
            badges
        )
    }

    fn inspector_lines(&self) -> Vec<String> {
        let mut lines = vec![
            self.title.clone(),
            format!(
                "ID: {}",
                self.canonical_id
                    .as_deref()
                    .map(|id| format!("@{id}"))
                    .unwrap_or_else(|| "-".to_owned())
            ),
            format!("Path: {}", normalize_path(&self.file_path)),
            format!(
                "Position: {}",
                location_text(self.start_line, self.start_column)
            ),
        ];
        if let Some(marker) = &self.todo_marker {
            lines.push(format!("Todo: {marker}"));
        }
        if let Some(date) = &self.lifecycle_date {
            lines.push(format!("Lifecycle: {date}"));
        }
        if !self.tags.is_empty() {
            lines.push(format!(
                "Tags: {}",
                self.tags
                    .iter()
                    .map(|tag| format!("#{tag}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if !self.properties.is_empty() {
            lines.push(format!(
                "Properties: {}",
                self.properties
                    .iter()
                    .map(|(key, value)| format!("{key}::{value}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if !self.badges.is_empty() {
            lines.push(format!(
                "Today queries: {}",
                self.badges
                    .iter()
                    .map(|badge| badge.query.as_str())
                    .collect::<Vec<_>>()
                    .join(" | ")
            ));
        }
        if let Some(preview) = &self.preview {
            lines.push(format!("Preview: {preview}"));
        }
        lines
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct DiagnosticRow {
    pub(crate) id: i64,
    pub(crate) severity: String,
    pub(crate) category: String,
    pub(crate) code: Option<String>,
    pub(crate) message: String,
    pub(crate) relative_path: Option<PathBuf>,
    pub(crate) start_line: Option<usize>,
    pub(crate) start_column: Option<usize>,
    pub(crate) zettel_id: Option<i64>,
}

impl DiagnosticRow {
    pub(crate) fn sort_key(&self) -> (u8, String, i64) {
        (
            self.severity_rank(),
            self.relative_path
                .as_ref()
                .map(|path| normalize_path(path))
                .unwrap_or_default(),
            self.id,
        )
    }

    fn severity_rank(&self) -> u8 {
        match self.severity.as_str() {
            "error" => 0,
            "warning" => 1,
            "info" => 2,
            _ => 3,
        }
    }

    fn list_line(&self) -> String {
        let path = self
            .relative_path
            .as_ref()
            .map(|path| normalize_path(path))
            .unwrap_or_else(|| "-".to_owned());
        let code = self.code.as_deref().unwrap_or(self.category.as_str());
        format!(
            "{:<7} {:<28} {:<30} {}",
            self.severity, path, code, self.message
        )
    }

    fn inspector_lines(&self) -> Vec<String> {
        vec![
            self.message.clone(),
            format!("Severity: {}", self.severity),
            format!("Category: {}", self.category),
            format!("Code: {}", self.code.as_deref().unwrap_or("-")),
            format!(
                "Path: {}",
                self.relative_path
                    .as_ref()
                    .map(|path| normalize_path(path))
                    .unwrap_or_else(|| "-".to_owned())
            ),
            format!(
                "Position: {}",
                location_text(self.start_line, self.start_column)
            ),
            format!(
                "Zettel row: {}",
                self.zettel_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "-".to_owned())
            ),
        ]
    }
}

fn location_text(line: Option<usize>, column: Option<usize>) -> String {
    match (line, column) {
        (Some(line), Some(column)) => format!("{line}:{column}"),
        (Some(line), None) => line.to_string(),
        _ => "-".to_owned(),
    }
}

fn normalize_path(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn today_query_strings_are_documented() {
        let queries = TODAY_QUERY_SPECS
            .iter()
            .map(|spec| spec.query)
            .collect::<Vec<_>>();
        assert_eq!(
            queries,
            vec![
                "#z/todo due:<=today -did:*",
                "#z/todo do:<=today -did:*",
                "#z/todo todo:[ ] -did:*"
            ]
        );
    }

    #[test]
    fn index_health_classifies_missing_stale_and_current() {
        let mut panel = IndexPanel {
            schema_version: 2,
            rows: Vec::new(),
            discovered_files: 1,
            indexed_files: 1,
            changed_files: 0,
            new_files: 0,
            deleted_files: 0,
            diagnostic_count: 0,
            last_indexed_at_unix_ms: None,
        };
        assert_eq!(panel.health_label(), "missing");

        panel.last_indexed_at_unix_ms = Some(42);
        assert_eq!(panel.health_label(), "current");

        panel.changed_files = 1;
        assert_eq!(panel.health_label(), "stale");
    }

    #[test]
    fn row_order_prioritizes_zettel_before_diagnostics() {
        let diagnostic = PanelRow::Diagnostic(DiagnosticRow {
            id: 1,
            severity: "error".to_owned(),
            category: "semantic".to_owned(),
            code: Some("reference.unresolved_absolute".to_owned()),
            message: "Missing target".to_owned(),
            relative_path: Some(PathBuf::from("b.z")),
            start_line: Some(3),
            start_column: Some(1),
            zettel_id: None,
        });
        let zettel = PanelRow::Zettel(ZettelRow {
            store_id: 2,
            canonical_id: Some("task".to_owned()),
            file_path: PathBuf::from("a.z"),
            title: "Task".to_owned(),
            todo_marker: Some("[ ]".to_owned()),
            start_line: Some(2),
            start_column: Some(1),
            lifecycle_date: Some("2026-05-03".to_owned()),
            tags: vec!["z/todo".to_owned()],
            properties: Vec::new(),
            preview: None,
            badges: Vec::new(),
        });

        let mut rows = vec![diagnostic, zettel.clone()];
        rows.sort_by_key(PanelRow::sort_key);
        assert_eq!(rows.first(), Some(&zettel));
    }
}
