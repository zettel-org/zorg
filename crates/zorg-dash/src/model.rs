use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Duration;

use zorg_core::SourceSpan;
use zorg_fix::DiagnosticFixSelector;
use zorg_refactor::{TodoActionPlan, TodoDateField};

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

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum TodayMode {
    Combined,
    TodosOnly,
    DiagnosticsOnly,
}

impl TodayMode {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Combined => "combined",
            Self::TodosOnly => "todos",
            Self::DiagnosticsOnly => "diagnostics",
        }
    }

    pub(crate) const fn next(self) -> Self {
        match self {
            Self::Combined => Self::TodosOnly,
            Self::TodosOnly => Self::DiagnosticsOnly,
            Self::DiagnosticsOnly => Self::Combined,
        }
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

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum ColorMode {
    Enabled,
    Disabled,
}

impl ColorMode {
    pub(crate) const fn from_disabled(disabled: bool) -> Self {
        if disabled {
            Self::Disabled
        } else {
            Self::Enabled
        }
    }

    pub(crate) const fn is_enabled(self) -> bool {
        matches!(self, Self::Enabled)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum SeverityKind {
    Error,
    Warning,
    Info,
    Unknown,
}

impl SeverityKind {
    pub(crate) fn from_label(label: &str) -> Self {
        match label {
            "error" => Self::Error,
            "warning" => Self::Warning,
            "info" => Self::Info,
            _ => Self::Unknown,
        }
    }

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
            Self::Unknown => "status",
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct StatusEvent {
    pub(crate) order: usize,
    pub(crate) severity: SeverityKind,
    pub(crate) message: String,
    pub(crate) detail: Option<String>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum PendingOperationKind {
    Refresh,
    Reindex,
    Search,
    Capture,
    FixPreview,
    FixApply,
    TodoApply,
}

impl PendingOperationKind {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Refresh => "refresh",
            Self::Reindex => "reindex",
            Self::Search => "search",
            Self::Capture => "capture",
            Self::FixPreview => "fix preview",
            Self::FixApply => "fix apply",
            Self::TodoApply => "todo apply",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) struct PendingActivity {
    pub(crate) operation: PendingOperationKind,
    pub(crate) elapsed: Duration,
    pub(crate) tick: usize,
}

impl PendingActivity {
    pub(crate) const fn new(
        operation: PendingOperationKind,
        elapsed: Duration,
        tick: usize,
    ) -> Self {
        Self {
            operation,
            elapsed,
            tick,
        }
    }

    pub(crate) const fn spinner(self) -> &'static str {
        match self.tick % 4 {
            0 => "|",
            1 => "/",
            2 => "-",
            _ => "\\",
        }
    }
}

impl StatusEvent {
    pub(crate) fn new(
        order: usize,
        severity: SeverityKind,
        message: impl Into<String>,
        detail: Option<String>,
    ) -> Self {
        Self {
            order,
            severity,
            message: message.into(),
            detail,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct DashboardFrame {
    pub(crate) root: PathBuf,
    pub(crate) database_path: PathBuf,
    pub(crate) panel: Panel,
    pub(crate) today_mode: TodayMode,
    pub(crate) query: Option<String>,
    pub(crate) diagnostic_filters: DiagnosticFilters,
    marked_diagnostics: BTreeSet<PanelRowId>,
    pub(crate) snapshot: DashboardSnapshot,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) struct DashboardRenderState {
    pub(crate) selected_index: usize,
    pub(crate) scroll_offset: usize,
    pub(crate) total_row_count: usize,
}

impl DashboardRenderState {
    pub(crate) const fn new(
        selected_index: usize,
        scroll_offset: usize,
        total_row_count: usize,
    ) -> Self {
        Self {
            selected_index,
            scroll_offset,
            total_row_count,
        }
    }

    pub(crate) fn for_frame(frame: &DashboardFrame) -> Self {
        Self::new(0, 0, frame.active_rows().len())
    }

    pub(crate) fn position_text(self) -> String {
        if self.total_row_count == 0 {
            "0/0".to_owned()
        } else {
            format!(
                "{}/{}",
                self.selected_index.min(self.total_row_count - 1) + 1,
                self.total_row_count
            )
        }
    }
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
            today_mode: TodayMode::Combined,
            query,
            diagnostic_filters: DiagnosticFilters::default(),
            marked_diagnostics: BTreeSet::new(),
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

    pub(crate) fn marked_diagnostic_count(&self) -> usize {
        self.marked_diagnostics.len()
    }

    pub(crate) fn is_row_marked(&self, row: &PanelRow) -> bool {
        match row {
            PanelRow::Diagnostic(diagnostic) => self.is_diagnostic_marked(diagnostic),
            _ => false,
        }
    }

    pub(crate) fn is_diagnostic_marked(&self, diagnostic: &DiagnosticRow) -> bool {
        self.marked_diagnostics.contains(&diagnostic.row_id())
    }

    pub(crate) fn toggle_diagnostic_mark(&mut self, diagnostic: &DiagnosticRow) -> bool {
        let row_id = diagnostic.row_id();
        if self.marked_diagnostics.remove(&row_id) {
            false
        } else {
            self.marked_diagnostics.insert(row_id);
            true
        }
    }

    pub(crate) fn marked_diagnostic_summaries(
        &self,
        selected: Option<&DiagnosticRow>,
    ) -> MarkedDiagnosticsSummary {
        let marked_rows = self.marked_diagnostic_rows();
        let selected_file = selected.and_then(DiagnosticRow::display_path);
        let selected_file_marked_count = selected_file.as_ref().map(|path| {
            marked_rows
                .iter()
                .filter(|row| row.display_path().as_ref() == Some(path))
                .count()
        });

        MarkedDiagnosticsSummary {
            marked_count: marked_rows.len(),
            selected_is_marked: selected.is_some_and(|row| self.is_diagnostic_marked(row)),
            selected_file_marked_count,
            rows: marked_rows
                .iter()
                .take(MARKED_DIAGNOSTIC_PREVIEW_LIMIT)
                .map(|row| MarkedDiagnosticSummaryRow {
                    severity: row.severity.clone(),
                    code: row.code.clone().unwrap_or_else(|| row.category.clone()),
                    path: row.display_path().unwrap_or_else(|| "-".to_owned()),
                    position: row.position_text(),
                    message: row.message.clone(),
                })
                .collect(),
        }
    }

    pub(crate) fn active_rows(&self) -> Vec<PanelRow> {
        self.rows_for_panel(self.panel)
    }

    pub(crate) fn rows_for_panel(&self, panel: Panel) -> Vec<PanelRow> {
        match &self.snapshot {
            DashboardSnapshot::Degraded { .. } => Vec::new(),
            DashboardSnapshot::Ready {
                index,
                diagnostics,
                today,
                inbox,
                search,
            } => match panel {
                Panel::Today => today
                    .iter()
                    .filter(|row| match row {
                        PanelRow::Diagnostic(diagnostic) => {
                            self.today_mode != TodayMode::TodosOnly
                                && self.diagnostic_filters.matches(diagnostic)
                        }
                        PanelRow::Zettel(_) => self.today_mode != TodayMode::DiagnosticsOnly,
                        PanelRow::IndexStatus(_) => false,
                    })
                    .cloned()
                    .collect(),
                Panel::Inbox => inbox.iter().cloned().map(PanelRow::Zettel).collect(),
                Panel::Search => search.rows.iter().cloned().map(PanelRow::Zettel).collect(),
                Panel::Diagnostics => diagnostics
                    .iter()
                    .filter(|row| self.diagnostic_filters.matches(row))
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

    pub(crate) fn today_counts(&self) -> Option<TodayCounts> {
        match &self.snapshot {
            DashboardSnapshot::Ready { today, .. } => {
                let todos = today
                    .iter()
                    .filter(|row| matches!(row, PanelRow::Zettel(_)))
                    .count();
                let diagnostics_total = today
                    .iter()
                    .filter(|row| matches!(row, PanelRow::Diagnostic(_)))
                    .count();
                let diagnostics_visible = today
                    .iter()
                    .filter_map(|row| match row {
                        PanelRow::Diagnostic(diagnostic) => Some(diagnostic),
                        _ => None,
                    })
                    .filter(|row| self.diagnostic_filters.matches(row))
                    .count();
                let visible = self.rows_for_panel(Panel::Today).len();
                Some(TodayCounts {
                    visible,
                    todos,
                    diagnostics_visible,
                    diagnostics_total,
                })
            }
            DashboardSnapshot::Degraded { .. } => None,
        }
    }

    pub(crate) fn diagnostic_filter_counts(&self, panel: Panel) -> Option<DiagnosticFilterCounts> {
        match &self.snapshot {
            DashboardSnapshot::Ready {
                diagnostics, today, ..
            } => match panel {
                Panel::Diagnostics => {
                    let total = diagnostics.len();
                    let visible = diagnostics
                        .iter()
                        .filter(|row| self.diagnostic_filters.matches(row))
                        .count();
                    Some(DiagnosticFilterCounts { visible, total })
                }
                Panel::Today => {
                    let total = today
                        .iter()
                        .filter(|row| matches!(row, PanelRow::Diagnostic(_)))
                        .count();
                    let visible = today
                        .iter()
                        .filter_map(|row| match row {
                            PanelRow::Diagnostic(diagnostic) => Some(diagnostic),
                            _ => None,
                        })
                        .filter(|row| self.diagnostic_filters.matches(row))
                        .count();
                    Some(DiagnosticFilterCounts { visible, total })
                }
                _ => None,
            },
            DashboardSnapshot::Degraded { .. } => None,
        }
    }

    pub(crate) fn set_snapshot(&mut self, snapshot: DashboardSnapshot) {
        self.snapshot = snapshot;
        self.prune_marked_diagnostics();
    }

    pub(crate) fn set_query(&mut self, query: Option<String>) {
        self.query = query;
    }

    pub(crate) fn set_search(&mut self, search: SearchPanel) {
        if let DashboardSnapshot::Ready {
            search: current, ..
        } = &mut self.snapshot
        {
            *current = search;
        }
    }

    pub(crate) fn search_panel(&self) -> Option<&SearchPanel> {
        match &self.snapshot {
            DashboardSnapshot::Ready { search, .. } => Some(search),
            DashboardSnapshot::Degraded { .. } => None,
        }
    }

    pub(crate) fn selected_source_location(&self, selected_index: usize) -> Option<SourceLocation> {
        self.active_rows()
            .get(selected_index)
            .and_then(|row| row.source_location(&self.root))
    }

    pub(crate) fn inspector_lines_for_selection(&self, selected_index: usize) -> Vec<String> {
        match &self.snapshot {
            DashboardSnapshot::Degraded { message } => {
                let mut lines = vec![
                    "Read-only index unavailable".to_owned(),
                    String::new(),
                    "The dashboard opens the SQLite index read-only.".to_owned(),
                    format!("Root: {}", self.root.display()),
                    format!("Database: {}", self.database_path.display()),
                    format!("Run: {}", self.reindex_command()),
                    String::new(),
                ];
                lines.extend(message.lines().map(str::to_owned));
                lines
            }
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

    pub(crate) fn reindex_command(&self) -> String {
        format!(
            "zorg db reindex --root {} --db {}",
            shell_word(&self.root),
            shell_word(&self.database_path)
        )
    }

    fn marked_diagnostic_rows(&self) -> Vec<&DiagnosticRow> {
        match &self.snapshot {
            DashboardSnapshot::Ready { diagnostics, .. } => diagnostics
                .iter()
                .filter(|row| self.is_diagnostic_marked(row))
                .collect(),
            DashboardSnapshot::Degraded { .. } => Vec::new(),
        }
    }

    fn prune_marked_diagnostics(&mut self) {
        let active_ids = match &self.snapshot {
            DashboardSnapshot::Ready { diagnostics, .. } => diagnostics
                .iter()
                .map(DiagnosticRow::row_id)
                .collect::<BTreeSet<_>>(),
            DashboardSnapshot::Degraded { .. } => BTreeSet::new(),
        };
        self.marked_diagnostics
            .retain(|row_id| active_ids.contains(row_id));
    }
}

const MARKED_DIAGNOSTIC_PREVIEW_LIMIT: usize = 8;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) struct DiagnosticFilterCounts {
    pub(crate) visible: usize,
    pub(crate) total: usize,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) struct TodayCounts {
    pub(crate) visible: usize,
    pub(crate) todos: usize,
    pub(crate) diagnostics_visible: usize,
    pub(crate) diagnostics_total: usize,
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub(crate) enum DiagnosticSeverityFilter {
    #[default]
    All,
    Error,
    Warning,
    Info,
}

impl DiagnosticSeverityFilter {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
        }
    }

    pub(crate) const fn next(self) -> Self {
        match self {
            Self::All => Self::Error,
            Self::Error => Self::Warning,
            Self::Warning => Self::Info,
            Self::Info => Self::All,
        }
    }

    const fn matches(self, severity: SeverityKind) -> bool {
        match self {
            Self::All => true,
            Self::Error => matches!(severity, SeverityKind::Error),
            Self::Warning => matches!(severity, SeverityKind::Warning),
            Self::Info => matches!(severity, SeverityKind::Info),
        }
    }
}

#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub(crate) struct DiagnosticFilters {
    pub(crate) severity: DiagnosticSeverityFilter,
    pub(crate) code: String,
    pub(crate) path: String,
}

impl DiagnosticFilters {
    pub(crate) fn is_active(&self) -> bool {
        self.severity != DiagnosticSeverityFilter::All
            || !self.code.trim().is_empty()
            || !self.path.trim().is_empty()
    }

    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn active_labels(&self) -> Vec<String> {
        let mut labels = Vec::new();
        if self.severity != DiagnosticSeverityFilter::All {
            labels.push(format!("severity={}", self.severity.label()));
        }
        if !self.code.trim().is_empty() {
            labels.push(format!("code={}", self.code.trim()));
        }
        if !self.path.trim().is_empty() {
            labels.push(format!("path={}", self.path.trim()));
        }
        labels
    }

    pub(crate) fn matches(&self, row: &DiagnosticRow) -> bool {
        self.severity.matches(row.severity_kind())
            && diagnostic_code_matches(&self.code, row)
            && diagnostic_path_matches(&self.path, row)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum DashboardSnapshot {
    Ready {
        index: Box<IndexPanel>,
        diagnostics: Vec<DiagnosticRow>,
        today: Vec<PanelRow>,
        inbox: Vec<ZettelRow>,
        search: SearchPanel,
    },
    Degraded {
        message: String,
    },
}

impl DashboardSnapshot {
    pub(crate) fn metrics(&self) -> DashboardSnapshotMetrics {
        match self {
            Self::Ready {
                index,
                diagnostics,
                today,
                inbox,
                search,
            } => DashboardSnapshotMetrics {
                today_rows: today.len(),
                inbox_rows: inbox.len(),
                search_rows: search.rows.len(),
                diagnostic_rows: diagnostics.len(),
                index_diagnostics: index.diagnostic_count,
            },
            Self::Degraded { .. } => DashboardSnapshotMetrics::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub(crate) struct DashboardSnapshotMetrics {
    pub(crate) today_rows: usize,
    pub(crate) inbox_rows: usize,
    pub(crate) search_rows: usize,
    pub(crate) diagnostic_rows: usize,
    pub(crate) index_diagnostics: usize,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct SearchPanel {
    pub(crate) input: String,
    pub(crate) rows: Vec<ZettelRow>,
    pub(crate) error: Option<String>,
}

impl SearchPanel {
    pub(crate) fn empty(input: impl Into<String>) -> Self {
        Self {
            input: input.into(),
            rows: Vec::new(),
            error: None,
        }
    }

    pub(crate) fn with_rows(input: impl Into<String>, rows: Vec<ZettelRow>) -> Self {
        Self {
            input: input.into(),
            rows,
            error: None,
        }
    }

    pub(crate) fn with_error(input: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            input: input.into(),
            rows: Vec::new(),
            error: Some(error.into()),
        }
    }
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

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum PanelRowId {
    ZettelCanonical(String),
    ZettelStore(i64),
    DiagnosticDb(i64),
    DiagnosticFallback {
        code: String,
        path: String,
        start_line: Option<usize>,
        start_column: Option<usize>,
    },
    IndexStatus(String),
}

impl PanelRow {
    pub(crate) fn row_id(&self) -> PanelRowId {
        match self {
            Self::Zettel(row) => row.row_id(),
            Self::Diagnostic(row) => row.row_id(),
            Self::IndexStatus(row) => PanelRowId::IndexStatus(row.label.clone()),
        }
    }

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
                path: if row.file_path.is_absolute() {
                    row.file_path.clone()
                } else {
                    root.join(&row.file_path)
                },
                line: row.start_line,
                column: row.start_column,
            }),
            Self::Diagnostic(row) => row
                .absolute_path
                .clone()
                .or_else(|| row.relative_path.as_ref().map(|path| root.join(path)))
                .map(|path| SourceLocation {
                    path,
                    line: row.start_line,
                    column: row.start_column,
                }),
            Self::IndexStatus(_) => None,
        }
    }

    pub(crate) fn yank_overlay(&self, root: &std::path::Path) -> YankOverlay {
        let target_summary = self.yank_target_summary();
        let source_link = self
            .source_location(root)
            .map(|location| location.display_link());
        let diagnostic_message = match self {
            Self::Diagnostic(row) => Some(row.message.clone()),
            _ => None,
        };

        YankOverlay::new(
            target_summary,
            vec![
                YankOption::available(YankValueKind::RowId, self.yank_row_id()),
                match source_link {
                    Some(value) => YankOption::available(YankValueKind::SourceLink, value),
                    None => YankOption::unavailable(
                        YankValueKind::SourceLink,
                        "selected row has no source location",
                    ),
                },
                match diagnostic_message {
                    Some(value) => YankOption::available(YankValueKind::DiagnosticMessage, value),
                    None => YankOption::unavailable(
                        YankValueKind::DiagnosticMessage,
                        "selected row is not diagnostic",
                    ),
                },
            ],
        )
    }

    fn yank_row_id(&self) -> String {
        match self {
            Self::Zettel(row) => row
                .canonical_id
                .as_deref()
                .map(|id| format!("@{id}"))
                .unwrap_or_else(|| format!("store:{}", row.store_id)),
            Self::Diagnostic(row) => {
                let code = row.code.as_deref().unwrap_or(row.category.as_str());
                let path = row.display_path().unwrap_or_else(|| "-".to_owned());
                match (row.start_line, row.start_column) {
                    (Some(line), Some(column)) => format!("{code}:{path}:{line}:{column}"),
                    (Some(line), None) => format!("{code}:{path}:{line}"),
                    _ => format!("{code}:{path}"),
                }
            }
            Self::IndexStatus(row) => row.label.clone(),
        }
    }

    fn yank_target_summary(&self) -> String {
        match self {
            Self::Zettel(row) => row
                .canonical_id
                .as_deref()
                .map(|id| format!("@{id} {}", row.title))
                .unwrap_or_else(|| format!("store:{} {}", row.store_id, row.title)),
            Self::Diagnostic(row) => {
                let code = row.code.as_deref().unwrap_or(row.category.as_str());
                let path = row.display_path().unwrap_or_else(|| "-".to_owned());
                format!("{code} {path}")
            }
            Self::IndexStatus(row) => row.label.clone(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct SourceLocation {
    pub(crate) path: PathBuf,
    pub(crate) line: Option<usize>,
    pub(crate) column: Option<usize>,
}

impl SourceLocation {
    pub(crate) fn display_link(&self) -> String {
        match (self.line, self.column) {
            (Some(line), Some(column)) => {
                format!("{}:{line}:{column}", normalize_path(&self.path))
            }
            (Some(line), None) => format!("{}:{line}", normalize_path(&self.path)),
            _ => normalize_path(&self.path),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum DashboardOverlay {
    None,
    Help,
    ConfirmReindex,
    ConfirmFixApply(FixPreviewOverlay),
    ConfirmTodoApply(TodoActionOverlay),
    TodoPrompt(TodoPromptDraft),
    Yank(YankOverlay),
    Capture(CaptureDraft),
    DiagnosticFilter(DiagnosticFilterDraft),
    FixPreview(FixPreviewOverlay),
    EventLog,
    Log { title: String, message: String },
}

impl DashboardOverlay {
    pub(crate) fn is_confirming_reindex(&self) -> bool {
        matches!(self, Self::ConfirmReindex)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum YankValueKind {
    RowId,
    SourceLink,
    DiagnosticMessage,
}

impl YankValueKind {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::RowId => "row id",
            Self::SourceLink => "source link",
            Self::DiagnosticMessage => "diagnostic message",
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct YankOption {
    pub(crate) kind: YankValueKind,
    pub(crate) value: Option<String>,
    pub(crate) unavailable_reason: Option<String>,
}

impl YankOption {
    fn available(kind: YankValueKind, value: impl Into<String>) -> Self {
        Self {
            kind,
            value: Some(value.into()),
            unavailable_reason: None,
        }
    }

    fn unavailable(kind: YankValueKind, reason: impl Into<String>) -> Self {
        Self {
            kind,
            value: None,
            unavailable_reason: Some(reason.into()),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct YankOverlay {
    pub(crate) target_summary: String,
    pub(crate) options: Vec<YankOption>,
    pub(crate) selected_index: usize,
}

impl YankOverlay {
    pub(crate) fn new(target_summary: impl Into<String>, options: Vec<YankOption>) -> Self {
        Self {
            target_summary: target_summary.into(),
            options,
            selected_index: 0,
        }
    }

    pub(crate) fn move_selection(&mut self, direction: isize) {
        if self.options.is_empty() {
            self.selected_index = 0;
            return;
        }
        let len = self.options.len() as isize;
        self.selected_index = (self.selected_index as isize + direction).rem_euclid(len) as usize;
    }

    pub(crate) fn select_index(&mut self, index: usize) {
        if !self.options.is_empty() {
            self.selected_index = index.min(self.options.len() - 1);
        }
    }

    pub(crate) fn selected_option(&self) -> Option<&YankOption> {
        self.options.get(self.selected_index)
    }

    pub(crate) fn selected_command(&self) -> Result<YankCommand, String> {
        let option = self
            .selected_option()
            .ok_or_else(|| "yank unavailable: no values for selected row".to_owned())?;
        match &option.value {
            Some(value) => Ok(YankCommand {
                kind: option.kind,
                target_summary: self.target_summary.clone(),
                value: value.clone(),
            }),
            None => Err(format!(
                "yank {} unavailable: {}",
                option.kind.label(),
                option
                    .unavailable_reason
                    .as_deref()
                    .unwrap_or("value is not available for this row")
            )),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct YankCommand {
    pub(crate) kind: YankValueKind,
    pub(crate) target_summary: String,
    pub(crate) value: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct TodoActionOverlay {
    pub(crate) title: String,
    pub(crate) row: ZettelRow,
    pub(crate) plan: TodoActionPlan,
}

impl TodoActionOverlay {
    pub(crate) fn new(title: impl Into<String>, row: ZettelRow, plan: TodoActionPlan) -> Self {
        Self {
            title: title.into(),
            row,
            plan,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum TodoPromptAction {
    Postpone,
    Schedule,
}

impl TodoPromptAction {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Postpone => "postpone",
            Self::Schedule => "schedule",
        }
    }

    pub(crate) const fn title(self) -> &'static str {
        match self {
            Self::Postpone => "Postpone Todo",
            Self::Schedule => "Schedule Todo",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum TodoPromptField {
    Target,
    Date,
}

impl TodoPromptField {
    const ALL: [Self; 2] = [Self::Target, Self::Date];

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Target => "Field",
            Self::Date => "Date",
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct TodoPromptDraft {
    pub(crate) action: TodoPromptAction,
    pub(crate) row: ZettelRow,
    pub(crate) date_input: String,
    pub(crate) active: TodoPromptField,
    pub(crate) target_field: Option<TodoDateField>,
    pub(crate) field_options: Vec<TodoDateField>,
    pub(crate) error: Option<String>,
}

impl TodoPromptDraft {
    pub(crate) fn postpone(row: ZettelRow, field_options: Vec<TodoDateField>) -> Self {
        let ambiguous = field_options.len() > 1;
        let target_field = if ambiguous {
            None
        } else {
            field_options.first().copied()
        };
        Self {
            action: TodoPromptAction::Postpone,
            row,
            date_input: String::new(),
            active: if ambiguous {
                TodoPromptField::Target
            } else {
                TodoPromptField::Date
            },
            target_field,
            field_options,
            error: None,
        }
    }

    pub(crate) fn schedule(row: ZettelRow) -> Self {
        Self {
            action: TodoPromptAction::Schedule,
            row,
            date_input: String::new(),
            active: TodoPromptField::Date,
            target_field: None,
            field_options: Vec::new(),
            error: None,
        }
    }

    pub(crate) fn visible_fields(&self) -> Vec<TodoPromptField> {
        if self.field_options.len() > 1 {
            TodoPromptField::ALL.to_vec()
        } else {
            vec![TodoPromptField::Date]
        }
    }

    pub(crate) fn next_field(&mut self) {
        self.move_field(1);
    }

    pub(crate) fn previous_field(&mut self) {
        self.move_field(-1);
    }

    pub(crate) fn active_value_mut(&mut self) -> &mut String {
        &mut self.date_input
    }

    pub(crate) fn clear_error(&mut self) {
        self.error = None;
    }

    pub(crate) fn cycle_target(&mut self, direction: isize) {
        if self.field_options.len() < 2 {
            return;
        }
        let selected = self.target_field.unwrap_or(self.field_options[0]);
        let index = self
            .field_options
            .iter()
            .position(|field| *field == selected)
            .unwrap_or(0);
        let len = self.field_options.len() as isize;
        let next = (index as isize + direction).rem_euclid(len) as usize;
        self.target_field = self.field_options.get(next).copied();
    }

    pub(crate) fn select_target(&mut self, field: TodoDateField) {
        if self.field_options.contains(&field) {
            self.target_field = Some(field);
        }
    }

    pub(crate) fn field_value(&self, field: TodoPromptField) -> String {
        match field {
            TodoPromptField::Target => self
                .target_field
                .map(todo_date_field_label)
                .unwrap_or("-")
                .to_owned(),
            TodoPromptField::Date => {
                if self.date_input.is_empty() {
                    "-".to_owned()
                } else {
                    self.date_input.clone()
                }
            }
        }
    }

    fn move_field(&mut self, direction: isize) {
        let fields = self.visible_fields();
        let index = fields
            .iter()
            .position(|field| *field == self.active)
            .unwrap_or(0);
        let len = fields.len() as isize;
        let next = (index as isize + direction).rem_euclid(len) as usize;
        self.active = fields[next];
    }
}

pub(crate) const fn todo_date_field_label(field: TodoDateField) -> &'static str {
    match field {
        TodoDateField::Due => "due",
        TodoDateField::Do => "do",
        TodoDateField::Did => "did",
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct DiagnosticFilterDraft {
    pub(crate) code: String,
    pub(crate) path: String,
    pub(crate) active: DiagnosticFilterField,
}

impl DiagnosticFilterDraft {
    pub(crate) fn from_filters(filters: &DiagnosticFilters) -> Self {
        Self {
            code: filters.code.clone(),
            path: filters.path.clone(),
            active: DiagnosticFilterField::Code,
        }
    }

    pub(crate) fn active_value_mut(&mut self) -> &mut String {
        match self.active {
            DiagnosticFilterField::Code => &mut self.code,
            DiagnosticFilterField::Path => &mut self.path,
        }
    }

    pub(crate) fn field_value(&self, field: DiagnosticFilterField) -> &str {
        match field {
            DiagnosticFilterField::Code => &self.code,
            DiagnosticFilterField::Path => &self.path,
        }
    }

    pub(crate) fn next_field(&mut self) {
        self.active = self.active.next();
    }

    pub(crate) fn previous_field(&mut self) {
        self.active = self.active.previous();
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum DiagnosticFilterField {
    Code,
    Path,
}

impl DiagnosticFilterField {
    pub(crate) const ALL: [Self; 2] = [Self::Code, Self::Path];

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Code => "Code",
            Self::Path => "Path",
        }
    }

    const fn index(self) -> usize {
        match self {
            Self::Code => 0,
            Self::Path => 1,
        }
    }

    fn next(self) -> Self {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    fn previous(self) -> Self {
        Self::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct CaptureDraft {
    pub(crate) template: String,
    pub(crate) title: String,
    pub(crate) body: String,
    pub(crate) destination: String,
    pub(crate) active: CaptureField,
}

impl CaptureDraft {
    pub(crate) fn new(template: impl Into<String>, destination: Option<String>) -> Self {
        Self {
            template: template.into(),
            title: String::new(),
            body: String::new(),
            destination: destination.unwrap_or_default(),
            active: CaptureField::Template,
        }
    }

    pub(crate) fn field_value(&self, field: CaptureField) -> &str {
        match field {
            CaptureField::Template => &self.template,
            CaptureField::Title => &self.title,
            CaptureField::Body => &self.body,
            CaptureField::Destination => &self.destination,
        }
    }

    pub(crate) fn active_value_mut(&mut self) -> &mut String {
        match self.active {
            CaptureField::Template => &mut self.template,
            CaptureField::Title => &mut self.title,
            CaptureField::Body => &mut self.body,
            CaptureField::Destination => &mut self.destination,
        }
    }

    pub(crate) fn next_field(&mut self) {
        self.active = self.active.next();
    }

    pub(crate) fn previous_field(&mut self) {
        self.active = self.active.previous();
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum CaptureField {
    Template,
    Title,
    Body,
    Destination,
}

impl CaptureField {
    pub(crate) const ALL: [Self; 4] = [Self::Template, Self::Title, Self::Body, Self::Destination];

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Template => "Template",
            Self::Title => "Title",
            Self::Body => "Body",
            Self::Destination => "Destination",
        }
    }

    const fn index(self) -> usize {
        match self {
            Self::Template => 0,
            Self::Title => 1,
            Self::Body => 2,
            Self::Destination => 3,
        }
    }

    fn next(self) -> Self {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    fn previous(self) -> Self {
        Self::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
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
pub(crate) struct FixPreviewOverlay {
    pub(crate) diagnostic: DiagnosticPreviewContext,
    pub(crate) previews: Vec<FixPreviewRow>,
    pub(crate) unavailable_reason: Option<String>,
    pub(crate) selector: DiagnosticFixSelector,
    pub(crate) marked_summary: Option<MarkedDiagnosticsSummary>,
}

impl FixPreviewOverlay {
    pub(crate) fn can_apply_selected_fix(&self) -> bool {
        self.previews.iter().any(|preview| preview.is_safe)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct DiagnosticPreviewContext {
    pub(crate) severity: String,
    pub(crate) code: String,
    pub(crate) message: String,
    pub(crate) path: String,
    pub(crate) position: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct MarkedDiagnosticsSummary {
    pub(crate) marked_count: usize,
    pub(crate) selected_is_marked: bool,
    pub(crate) selected_file_marked_count: Option<usize>,
    pub(crate) rows: Vec<MarkedDiagnosticSummaryRow>,
}

impl MarkedDiagnosticsSummary {
    pub(crate) const fn is_empty(&self) -> bool {
        self.marked_count == 0
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct MarkedDiagnosticSummaryRow {
    pub(crate) severity: String,
    pub(crate) code: String,
    pub(crate) path: String,
    pub(crate) position: String,
    pub(crate) message: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct FixPreviewRow {
    pub(crate) rule_code: String,
    pub(crate) severity: String,
    pub(crate) path: PathBuf,
    pub(crate) primary_line: Option<usize>,
    pub(crate) primary_column: Option<usize>,
    pub(crate) replacement_preview: String,
    pub(crate) replacement_truncated: bool,
    pub(crate) is_preferred: bool,
    pub(crate) is_safe: bool,
    pub(crate) explanation: String,
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
    pub(crate) todo_span: Option<SourceSpan>,
    pub(crate) source_span: SourceSpan,
    pub(crate) source_order: i64,
    pub(crate) start_line: Option<usize>,
    pub(crate) start_column: Option<usize>,
    pub(crate) lifecycle_date: Option<String>,
    pub(crate) tags: Vec<String>,
    pub(crate) properties: Vec<(String, String)>,
    pub(crate) preview: Option<String>,
    pub(crate) badges: Vec<QueryBadge>,
}

impl ZettelRow {
    pub(crate) fn row_id(&self) -> PanelRowId {
        self.canonical_id
            .clone()
            .map(PanelRowId::ZettelCanonical)
            .unwrap_or(PanelRowId::ZettelStore(self.store_id))
    }

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
    pub(crate) absolute_path: Option<PathBuf>,
    pub(crate) relative_path: Option<PathBuf>,
    pub(crate) start_byte: Option<usize>,
    pub(crate) end_byte: Option<usize>,
    pub(crate) start_line: Option<usize>,
    pub(crate) start_column: Option<usize>,
    pub(crate) end_line: Option<usize>,
    pub(crate) end_column: Option<usize>,
    pub(crate) zettel_id: Option<i64>,
}

impl DiagnosticRow {
    pub(crate) fn row_id(&self) -> PanelRowId {
        if self.id > 0 {
            return PanelRowId::DiagnosticDb(self.id);
        }

        PanelRowId::DiagnosticFallback {
            code: self.code.clone().unwrap_or_else(|| self.category.clone()),
            path: self
                .relative_path
                .as_ref()
                .map(|path| normalize_path(path))
                .unwrap_or_default(),
            start_line: self.start_line,
            start_column: self.start_column,
        }
    }

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

    pub(crate) fn severity_kind(&self) -> SeverityKind {
        SeverityKind::from_label(&self.severity)
    }

    fn severity_rank(&self) -> u8 {
        match self.severity_kind() {
            SeverityKind::Error => 0,
            SeverityKind::Warning => 1,
            SeverityKind::Info => 2,
            SeverityKind::Unknown => 3,
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

    fn display_path(&self) -> Option<String> {
        self.relative_path
            .as_ref()
            .or(self.absolute_path.as_ref())
            .map(|path| normalize_path(path))
    }

    fn position_text(&self) -> String {
        span_location_text(
            self.start_line,
            self.start_column,
            self.end_line,
            self.end_column,
        )
    }

    fn inspector_lines(&self) -> Vec<String> {
        vec![
            self.message.clone(),
            format!("Severity: {}", self.severity),
            format!("Category: {}", self.category),
            format!("Code: {}", self.code.as_deref().unwrap_or("-")),
            format!(
                "Absolute path: {}",
                self.absolute_path
                    .as_ref()
                    .map(|path| normalize_path(path))
                    .unwrap_or_else(|| "-".to_owned())
            ),
            format!(
                "Path: {}",
                self.relative_path
                    .as_ref()
                    .map(|path| normalize_path(path))
                    .unwrap_or_else(|| "-".to_owned())
            ),
            format!("Position: {}", self.position_text()),
            format!("Bytes: {}", byte_span_text(self.start_byte, self.end_byte)),
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

fn span_location_text(
    start_line: Option<usize>,
    start_column: Option<usize>,
    end_line: Option<usize>,
    end_column: Option<usize>,
) -> String {
    let start = location_text(start_line, start_column);
    let end = location_text(end_line, end_column);
    if start == "-" || end == "-" || start == end {
        start
    } else {
        format!("{start}-{end}")
    }
}

fn byte_span_text(start_byte: Option<usize>, end_byte: Option<usize>) -> String {
    match (start_byte, end_byte) {
        (Some(start), Some(end)) => format!("{start}..{end}"),
        _ => "-".to_owned(),
    }
}

fn diagnostic_code_matches(filter: &str, row: &DiagnosticRow) -> bool {
    let filter = filter.trim();
    if filter.is_empty() {
        return true;
    }

    let filter = filter.to_lowercase();
    row.code
        .as_deref()
        .map(|code| code.to_lowercase().contains(&filter))
        .unwrap_or(false)
        || row.category.to_lowercase().contains(&filter)
}

fn diagnostic_path_matches(filter: &str, row: &DiagnosticRow) -> bool {
    let filter = filter.trim();
    if filter.is_empty() {
        return true;
    }

    let filter = filter.to_lowercase();
    row.relative_path
        .as_ref()
        .map(|path| normalize_path(path).to_lowercase().contains(&filter))
        .unwrap_or(false)
        || row
            .absolute_path
            .as_ref()
            .map(|path| normalize_path(path).to_lowercase().contains(&filter))
            .unwrap_or(false)
}

fn normalize_path(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn shell_word(path: &std::path::Path) -> String {
    let value = path.to_string_lossy();
    if value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || "/._-".contains(character))
    {
        return value.into_owned();
    }

    format!("'{}'", value.replace('\'', "'\\''"))
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
            absolute_path: None,
            relative_path: Some(PathBuf::from("b.z")),
            start_byte: Some(20),
            end_byte: Some(28),
            start_line: Some(3),
            start_column: Some(1),
            end_line: Some(3),
            end_column: Some(9),
            zettel_id: None,
        });
        let zettel = PanelRow::Zettel(ZettelRow {
            store_id: 2,
            canonical_id: Some("task".to_owned()),
            file_path: PathBuf::from("a.z"),
            title: "Task".to_owned(),
            todo_marker: Some("[ ]".to_owned()),
            todo_span: Some(SourceSpan::bytes(0, 3)),
            source_span: SourceSpan::bytes(0, 24),
            source_order: 0,
            start_line: Some(2),
            start_column: Some(1),
            lifecycle_date: Some("2026-05-03".to_owned()),
            tags: vec!["z/todo".to_owned()],
            properties: Vec::new(),
            preview: None,
            badges: Vec::new(),
        });

        let mut rows = [diagnostic, zettel.clone()];
        rows.sort_by_key(PanelRow::sort_key);
        assert_eq!(rows.first(), Some(&zettel));
    }

    #[test]
    fn row_identity_prefers_canonical_zettel_id_with_store_fallback() {
        let mut row = ZettelRow {
            store_id: 7,
            canonical_id: Some("task".to_owned()),
            file_path: PathBuf::from("task.z"),
            title: "Task".to_owned(),
            todo_marker: None,
            todo_span: None,
            source_span: SourceSpan::bytes(0, 0),
            source_order: 0,
            start_line: Some(1),
            start_column: Some(1),
            lifecycle_date: None,
            tags: Vec::new(),
            properties: Vec::new(),
            preview: None,
            badges: Vec::new(),
        };

        assert_eq!(
            PanelRow::Zettel(row.clone()).row_id(),
            PanelRowId::ZettelCanonical("task".to_owned())
        );

        row.canonical_id = None;
        assert_eq!(PanelRow::Zettel(row).row_id(), PanelRowId::ZettelStore(7));
    }

    #[test]
    fn row_identity_covers_diagnostics_and_index_rows() {
        let mut diagnostic = DiagnosticRow {
            id: 11,
            severity: "warning".to_owned(),
            category: "semantic".to_owned(),
            code: Some("reference.missing".to_owned()),
            message: "Missing target".to_owned(),
            absolute_path: None,
            relative_path: Some(PathBuf::from("notes/task.z")),
            start_byte: Some(10),
            end_byte: Some(14),
            start_line: Some(3),
            start_column: Some(5),
            end_line: Some(3),
            end_column: Some(9),
            zettel_id: None,
        };

        assert_eq!(
            PanelRow::Diagnostic(diagnostic.clone()).row_id(),
            PanelRowId::DiagnosticDb(11)
        );

        diagnostic.id = 0;
        assert_eq!(
            PanelRow::Diagnostic(diagnostic).row_id(),
            PanelRowId::DiagnosticFallback {
                code: "reference.missing".to_owned(),
                path: "notes/task.z".to_owned(),
                start_line: Some(3),
                start_column: Some(5),
            }
        );

        assert_eq!(
            PanelRow::IndexStatus(IndexStatusRow::new("Discovered files", 3)).row_id(),
            PanelRowId::IndexStatus("Discovered files".to_owned())
        );
    }

    #[test]
    fn diagnostic_filters_apply_to_diagnostics_and_today_diagnostic_rows_only() {
        let mut frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Diagnostics,
            None,
            DashboardSnapshot::Ready {
                index: Box::new(IndexPanel {
                    schema_version: 2,
                    rows: Vec::new(),
                    discovered_files: 1,
                    indexed_files: 1,
                    changed_files: 0,
                    new_files: 0,
                    deleted_files: 0,
                    diagnostic_count: 2,
                    last_indexed_at_unix_ms: Some(42),
                }),
                diagnostics: vec![
                    diagnostic_row(1, "error", "reference.missing", "notes/a.z"),
                    diagnostic_row(2, "warning", "syntax.sort", "notes/b.z"),
                ],
                today: vec![
                    PanelRow::Zettel(ZettelRow {
                        store_id: 7,
                        canonical_id: Some("task".to_owned()),
                        file_path: PathBuf::from("notes/task.z"),
                        title: "Task".to_owned(),
                        todo_marker: Some("[ ]".to_owned()),
                        todo_span: Some(SourceSpan::bytes(0, 3)),
                        source_span: SourceSpan::bytes(0, 24),
                        source_order: 0,
                        start_line: Some(1),
                        start_column: Some(1),
                        lifecycle_date: None,
                        tags: Vec::new(),
                        properties: Vec::new(),
                        preview: None,
                        badges: Vec::new(),
                    }),
                    PanelRow::Diagnostic(diagnostic_row(
                        1,
                        "error",
                        "reference.missing",
                        "notes/a.z",
                    )),
                    PanelRow::Diagnostic(diagnostic_row(2, "warning", "syntax.sort", "notes/b.z")),
                ],
                inbox: Vec::new(),
                search: SearchPanel::empty(""),
            },
        );

        frame.diagnostic_filters.severity = DiagnosticSeverityFilter::Error;
        frame.diagnostic_filters.code = "reference".to_owned();
        frame.diagnostic_filters.path = "a.z".to_owned();

        assert_eq!(frame.rows_for_panel(Panel::Diagnostics).len(), 1);
        let today_rows = frame.rows_for_panel(Panel::Today);
        assert_eq!(today_rows.len(), 2);
        assert!(matches!(today_rows.first(), Some(PanelRow::Zettel(_))));
        assert_eq!(
            frame.diagnostic_filter_counts(Panel::Diagnostics),
            Some(DiagnosticFilterCounts {
                visible: 1,
                total: 2
            })
        );
    }

    #[test]
    fn today_modes_filter_only_today_rows_and_report_counts() {
        let mut frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Today,
            None,
            DashboardSnapshot::Ready {
                index: Box::new(IndexPanel {
                    schema_version: 2,
                    rows: Vec::new(),
                    discovered_files: 1,
                    indexed_files: 1,
                    changed_files: 0,
                    new_files: 0,
                    deleted_files: 0,
                    diagnostic_count: 2,
                    last_indexed_at_unix_ms: Some(42),
                }),
                diagnostics: vec![
                    diagnostic_row(1, "error", "reference.missing", "notes/a.z"),
                    diagnostic_row(2, "warning", "syntax.sort", "notes/b.z"),
                ],
                today: vec![
                    PanelRow::Zettel(test_zettel_row(7, "task")),
                    PanelRow::Diagnostic(diagnostic_row(
                        1,
                        "error",
                        "reference.missing",
                        "notes/a.z",
                    )),
                    PanelRow::Diagnostic(diagnostic_row(2, "warning", "syntax.sort", "notes/b.z")),
                ],
                inbox: Vec::new(),
                search: SearchPanel::empty(""),
            },
        );
        frame.diagnostic_filters.severity = DiagnosticSeverityFilter::Error;

        assert_eq!(frame.rows_for_panel(Panel::Today).len(), 2);
        assert_eq!(
            frame.today_counts(),
            Some(TodayCounts {
                visible: 2,
                todos: 1,
                diagnostics_visible: 1,
                diagnostics_total: 2,
            })
        );

        frame.today_mode = TodayMode::TodosOnly;
        assert!(
            frame
                .rows_for_panel(Panel::Today)
                .iter()
                .all(|row| matches!(row, PanelRow::Zettel(_)))
        );
        assert_eq!(frame.rows_for_panel(Panel::Diagnostics).len(), 1);

        frame.today_mode = TodayMode::DiagnosticsOnly;
        assert!(
            frame
                .rows_for_panel(Panel::Today)
                .iter()
                .all(|row| matches!(row, PanelRow::Diagnostic(_)))
        );
        assert_eq!(frame.rows_for_panel(Panel::Today).len(), 1);
    }

    #[test]
    fn zettel_source_locations_are_resolved_under_root() {
        let row = PanelRow::Zettel(ZettelRow {
            store_id: 2,
            canonical_id: Some("task".to_owned()),
            file_path: PathBuf::from("notes/task.z"),
            title: "Task".to_owned(),
            todo_marker: Some("[ ]".to_owned()),
            todo_span: Some(SourceSpan::bytes(0, 3)),
            source_span: SourceSpan::bytes(0, 24),
            source_order: 0,
            start_line: Some(2),
            start_column: Some(1),
            lifecycle_date: None,
            tags: Vec::new(),
            properties: Vec::new(),
            preview: None,
            badges: Vec::new(),
        });

        let location = row
            .source_location(std::path::Path::new("/tmp/corpus"))
            .expect("zettel rows have source locations");

        assert_eq!(location.path, PathBuf::from("/tmp/corpus/notes/task.z"));
        assert_eq!(location.line, Some(2));
        assert_eq!(location.column, Some(1));
    }

    #[test]
    fn yank_values_cover_row_ids_source_links_and_diagnostic_messages() {
        let zettel = PanelRow::Zettel(test_zettel_row(7, "task"));
        let zettel_overlay = zettel.yank_overlay(std::path::Path::new("/tmp/corpus"));
        assert_eq!(zettel_overlay.options[0].value.as_deref(), Some("@task"));
        assert_eq!(
            zettel_overlay.options[1].value.as_deref(),
            Some("/tmp/corpus/notes/task.z:1:1")
        );
        assert!(zettel_overlay.options[2].value.is_none());

        let diagnostic = PanelRow::Diagnostic(diagnostic_row(
            0,
            "warning",
            "reference.missing",
            "notes/a.z",
        ));
        let diagnostic_overlay = diagnostic.yank_overlay(std::path::Path::new("/tmp/corpus"));
        assert_eq!(
            diagnostic_overlay.options[0].value.as_deref(),
            Some("reference.missing:notes/a.z:1:1")
        );
        assert_eq!(
            diagnostic_overlay.options[2].value.as_deref(),
            Some("reference.missing message")
        );

        let index = PanelRow::IndexStatus(IndexStatusRow::new("Discovered files", 3));
        let index_overlay = index.yank_overlay(std::path::Path::new("/tmp/corpus"));
        assert_eq!(
            index_overlay.selected_command().expect("row id").value,
            "Discovered files"
        );
        assert!(index_overlay.options[1].value.is_none());
    }

    fn diagnostic_row(id: i64, severity: &str, code: &str, path: &str) -> DiagnosticRow {
        DiagnosticRow {
            id,
            severity: severity.to_owned(),
            category: "semantic".to_owned(),
            code: Some(code.to_owned()),
            message: format!("{code} message"),
            absolute_path: None,
            relative_path: Some(PathBuf::from(path)),
            start_byte: Some(0),
            end_byte: Some(1),
            start_line: Some(1),
            start_column: Some(1),
            end_line: Some(1),
            end_column: Some(2),
            zettel_id: None,
        }
    }

    fn test_zettel_row(store_id: i64, title: &str) -> ZettelRow {
        ZettelRow {
            store_id,
            canonical_id: Some(title.to_owned()),
            file_path: PathBuf::from(format!("notes/{title}.z")),
            title: title.to_owned(),
            todo_marker: Some("[ ]".to_owned()),
            todo_span: Some(SourceSpan::bytes(0, 3)),
            source_span: SourceSpan::bytes(0, 24),
            source_order: store_id,
            start_line: Some(1),
            start_column: Some(1),
            lifecycle_date: None,
            tags: Vec::new(),
            properties: Vec::new(),
            preview: None,
            badges: Vec::new(),
        }
    }
}
