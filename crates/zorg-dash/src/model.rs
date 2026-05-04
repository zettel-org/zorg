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
                Panel::Today => today.clone(),
                Panel::Inbox => inbox.iter().cloned().map(PanelRow::Zettel).collect(),
                Panel::Search => search.rows.iter().cloned().map(PanelRow::Zettel).collect(),
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

#[derive(Debug, Clone, Eq, PartialEq)]
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
                path: root.join(&row.file_path),
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
    Capture(CaptureDraft),
    Log { title: String, message: String },
}

impl DashboardOverlay {
    pub(crate) fn is_confirming_reindex(&self) -> bool {
        matches!(self, Self::ConfirmReindex)
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
    pub(crate) relative_path: Option<PathBuf>,
    pub(crate) start_line: Option<usize>,
    pub(crate) start_column: Option<usize>,
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
            relative_path: Some(PathBuf::from("notes/task.z")),
            start_line: Some(3),
            start_column: Some(5),
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
    fn zettel_source_locations_are_resolved_under_root() {
        let row = PanelRow::Zettel(ZettelRow {
            store_id: 2,
            canonical_id: Some("task".to_owned()),
            file_path: PathBuf::from("notes/task.z"),
            title: "Task".to_owned(),
            todo_marker: Some("[ ]".to_owned()),
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
}
