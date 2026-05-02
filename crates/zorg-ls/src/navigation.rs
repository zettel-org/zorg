use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use tower_lsp::lsp_types::{
    DocumentSymbol, Location, Position, Range, SymbolInformation, SymbolKind, Url,
};
use zorg_core::{BodyBlock, SourceSpan, Zettel, ZettelDocument};
use zorg_store::Store;

use crate::diagnostics::file_uri;

#[derive(Debug, Clone)]
pub(crate) struct LspIndex {
    declarations: Vec<ZettelSymbol>,
    declarations_by_id: BTreeMap<String, Vec<usize>>,
    references_by_id: BTreeMap<String, Vec<ZettelReference>>,
    references_by_uri: BTreeMap<Url, Vec<ZettelReference>>,
    top_symbols_by_uri: BTreeMap<Url, Vec<usize>>,
}

#[derive(Debug, Clone)]
struct ZettelSymbol {
    canonical_id: Option<String>,
    title: Option<String>,
    fallback_name: String,
    uri: Url,
    range: Range,
    selection_range: Range,
    declaration_range: Option<Range>,
    children: Vec<usize>,
}

#[derive(Debug, Clone)]
struct ZettelReference {
    target_id: String,
    uri: Url,
    range: Range,
}

enum SymbolAtPosition {
    Declaration(usize),
    Reference(String),
}

impl LspIndex {
    pub(crate) fn from_store(store: &Store) -> zorg_core::ZorgResult<Self> {
        let files = store.list_files()?;
        let relative_paths = files
            .iter()
            .map(|file| (file.absolute_path.clone(), file.relative_path.clone()))
            .collect::<BTreeMap<_, _>>();
        let mut documents = Vec::new();

        for file in files {
            let source = fs::read_to_string(&file.absolute_path).map_err(|error| {
                zorg_core::ZorgError::OperationFailed {
                    message: format!(
                        "failed to read indexed source {}: {error}",
                        file.absolute_path.display()
                    ),
                }
            })?;
            documents.push(zorg_parse::parse_zettel_document_with_path(
                &source,
                file.absolute_path,
            )?);
        }

        let _ = zorg_parse::resolve_corpus(&mut documents);
        Ok(Self::from_documents(documents, &relative_paths))
    }

    fn from_documents(
        documents: Vec<ZettelDocument>,
        relative_paths: &BTreeMap<PathBuf, PathBuf>,
    ) -> Self {
        let mut index = Self {
            declarations: Vec::new(),
            declarations_by_id: BTreeMap::new(),
            references_by_id: BTreeMap::new(),
            references_by_uri: BTreeMap::new(),
            top_symbols_by_uri: BTreeMap::new(),
        };

        for document in &documents {
            let Some(path) = document.path.as_ref().map(|path| path.as_path()) else {
                continue;
            };
            let Some(uri) = file_uri(path) else {
                continue;
            };
            let fallback_name = relative_paths
                .get(path)
                .unwrap_or(&path.to_path_buf())
                .display()
                .to_string();

            if let Some(root_index) =
                index.collect_zettel(&document.root, None, &document.source, &uri, fallback_name)
            {
                index
                    .top_symbols_by_uri
                    .entry(uri)
                    .or_default()
                    .push(root_index);
            }
        }

        index
    }

    pub(crate) fn goto_definition(&self, uri: &Url, position: Position) -> Option<Location> {
        match self.symbol_at_position(uri, position)? {
            SymbolAtPosition::Declaration(index) => {
                self.declaration_location(&self.declarations[index])
            }
            SymbolAtPosition::Reference(target_id) => {
                self.canonical_declaration_location(&target_id)
            }
        }
    }

    pub(crate) fn references(
        &self,
        uri: &Url,
        position: Position,
        include_declaration: bool,
    ) -> Option<Vec<Location>> {
        let target_id = match self.symbol_at_position(uri, position)? {
            SymbolAtPosition::Declaration(index) => {
                self.declarations[index].canonical_id.clone()?
            }
            SymbolAtPosition::Reference(target_id) => target_id,
        };

        let mut locations = Vec::new();
        if include_declaration {
            for declaration in self.unique_or_all_declarations(&target_id) {
                if let Some(location) = self.declaration_location(declaration) {
                    locations.push(location);
                }
            }
        }
        if let Some(references) = self.references_by_id.get(&target_id) {
            locations.extend(references.iter().map(reference_location));
        }

        (!locations.is_empty()).then_some(locations)
    }

    pub(crate) fn document_symbols(&self, uri: &Url) -> Vec<DocumentSymbol> {
        self.top_symbols_by_uri
            .get(uri)
            .into_iter()
            .flat_map(|indices| indices.iter())
            .map(|index| self.document_symbol(*index))
            .collect()
    }

    pub(crate) fn workspace_symbols(&self, query: &str) -> Vec<SymbolInformation> {
        let query = query.to_ascii_lowercase();
        self.declarations
            .iter()
            .filter(|declaration| declaration.canonical_id.is_some())
            .filter(|declaration| declaration.declaration_range.is_some())
            .filter(|declaration| {
                query.is_empty()
                    || declaration
                        .canonical_id
                        .as_deref()
                        .is_some_and(|id| id.to_ascii_lowercase().contains(&query))
                    || declaration
                        .title
                        .as_deref()
                        .is_some_and(|title| title.to_ascii_lowercase().contains(&query))
            })
            .map(|declaration| {
                #[allow(deprecated)]
                SymbolInformation {
                    name: declaration.name(),
                    kind: SymbolKind::OBJECT,
                    tags: None,
                    deprecated: None,
                    location: self
                        .declaration_location(declaration)
                        .expect("workspace symbols are filtered to source-backed declarations"),
                    container_name: None,
                }
            })
            .collect()
    }

    fn collect_zettel(
        &mut self,
        zettel: &Zettel,
        _parent_index: Option<usize>,
        source: &str,
        uri: &Url,
        fallback_name: String,
    ) -> Option<usize> {
        let range = source_span_to_range(zettel_full_span(zettel)?)?;
        let declaration_range = declaration_span(zettel, source).and_then(source_span_to_range);
        let selection_range = declaration_range.unwrap_or(range);
        let canonical_id = zettel
            .canonical_id
            .as_ref()
            .map(|id| id.as_str().to_owned());
        let symbol = ZettelSymbol {
            canonical_id: canonical_id.clone(),
            title: zettel.plain_title(),
            fallback_name,
            uri: uri.clone(),
            range,
            selection_range,
            declaration_range,
            children: Vec::new(),
        };
        let index = self.declarations.len();
        self.declarations.push(symbol);

        if let Some(canonical_id) = canonical_id {
            self.declarations_by_id
                .entry(canonical_id)
                .or_default()
                .push(index);
        }

        self.collect_references(zettel, uri);

        for child in child_zettels(zettel) {
            if let Some(child_index) = self.collect_zettel(
                child,
                Some(index),
                source,
                uri,
                "anonymous zettel".to_owned(),
            ) {
                self.declarations[index].children.push(child_index);
            }
        }

        Some(index)
    }

    fn collect_references(&mut self, zettel: &Zettel, uri: &Url) {
        for resolved in &zettel.resolved_links {
            let Some(range) = resolved.reference.span.and_then(source_span_to_range) else {
                continue;
            };
            let reference = ZettelReference {
                target_id: resolved.target_id.as_str().to_owned(),
                uri: uri.clone(),
                range,
            };
            self.references_by_id
                .entry(reference.target_id.clone())
                .or_default()
                .push(reference.clone());
            self.references_by_uri
                .entry(uri.clone())
                .or_default()
                .push(reference);
        }
    }

    fn symbol_at_position(&self, uri: &Url, position: Position) -> Option<SymbolAtPosition> {
        if let Some(declaration_index) = self.declarations.iter().position(|declaration| {
            &declaration.uri == uri
                && declaration
                    .declaration_range
                    .is_some_and(|range| range_contains(range, position))
        }) {
            return Some(SymbolAtPosition::Declaration(declaration_index));
        }

        self.references_by_uri.get(uri).and_then(|references| {
            references
                .iter()
                .find(|reference| range_contains(reference.range, position))
                .map(|reference| SymbolAtPosition::Reference(reference.target_id.clone()))
        })
    }

    fn canonical_declaration_location(&self, canonical_id: &str) -> Option<Location> {
        let declarations = self.declarations_by_id.get(canonical_id)?;
        if declarations.len() != 1 {
            return None;
        }
        self.declaration_location(&self.declarations[declarations[0]])
    }

    fn unique_or_all_declarations(&self, canonical_id: &str) -> Vec<&ZettelSymbol> {
        self.declarations_by_id
            .get(canonical_id)
            .into_iter()
            .flat_map(|indices| indices.iter())
            .map(|index| &self.declarations[*index])
            .collect()
    }

    fn declaration_location(&self, declaration: &ZettelSymbol) -> Option<Location> {
        Some(Location {
            uri: declaration.uri.clone(),
            range: declaration.declaration_range?,
        })
    }

    fn document_symbol(&self, index: usize) -> DocumentSymbol {
        let declaration = &self.declarations[index];
        #[allow(deprecated)]
        DocumentSymbol {
            name: declaration.name(),
            detail: declaration.title.clone(),
            kind: SymbolKind::OBJECT,
            tags: None,
            deprecated: None,
            range: declaration.range,
            selection_range: declaration.selection_range,
            children: Some(
                declaration
                    .children
                    .iter()
                    .map(|child| self.document_symbol(*child))
                    .collect(),
            )
            .filter(|children: &Vec<DocumentSymbol>| !children.is_empty()),
        }
    }
}

impl ZettelSymbol {
    fn name(&self) -> String {
        self.canonical_id
            .clone()
            .or_else(|| self.title.clone())
            .unwrap_or_else(|| self.fallback_name.clone())
    }
}

fn reference_location(reference: &ZettelReference) -> Location {
    Location {
        uri: reference.uri.clone(),
        range: reference.range,
    }
}

fn source_span_to_range(span: SourceSpan) -> Option<Range> {
    Some(Range {
        start: position(span.start_line?, span.start_column?),
        end: position(span.end_line?, span.end_column?),
    })
}

fn position(line: usize, column: usize) -> Position {
    Position::new(
        u32::try_from(line.saturating_sub(1)).unwrap_or(u32::MAX),
        u32::try_from(column.saturating_sub(1)).unwrap_or(u32::MAX),
    )
}

fn range_contains(range: Range, position: Position) -> bool {
    if range.start == range.end {
        return position == range.start;
    }
    compare_position(range.start, position) != std::cmp::Ordering::Greater
        && compare_position(position, range.end) == std::cmp::Ordering::Less
}

fn compare_position(left: Position, right: Position) -> std::cmp::Ordering {
    (left.line, left.character).cmp(&(right.line, right.character))
}

fn declaration_span(zettel: &Zettel, source: &str) -> Option<SourceSpan> {
    let opening = zettel.span?;
    let token = if let Some(id) = &zettel.id {
        format!("@{}", id.as_str())
    } else if let Some(local_id) = &zettel.local_id {
        format!("^{}", local_id.as_str())
    } else {
        return None;
    };

    let source_slice = source.get(opening.start_byte..opening.end_byte)?;
    let relative_start = source_slice.find(&token)?;
    let start = opening.start_byte + relative_start;
    Some(SourceSpan::from_offsets(source, start, start + token.len()))
}

fn zettel_full_span(zettel: &Zettel) -> Option<SourceSpan> {
    let mut span = zettel.span?;
    for block in &zettel.body {
        if let Some(block_span) = body_block_span(block) {
            span = merge_spans(span, block_span);
        }
    }
    Some(span)
}

fn body_block_span(block: &BodyBlock) -> Option<SourceSpan> {
    match block {
        BodyBlock::Paragraph(paragraph) => paragraph.span,
        BodyBlock::FencedCode(fenced) => fenced.span,
        BodyBlock::ChildZettel(child) => zettel_full_span(child),
    }
}

fn merge_spans(left: SourceSpan, right: SourceSpan) -> SourceSpan {
    let start = if left.start_byte <= right.start_byte {
        left
    } else {
        right
    };
    let end = if left.end_byte >= right.end_byte {
        left
    } else {
        right
    };

    SourceSpan {
        start_byte: left.start_byte.min(right.start_byte),
        end_byte: left.end_byte.max(right.end_byte),
        start_line: start.start_line,
        start_column: start.start_column,
        end_line: end.end_line,
        end_column: end.end_column,
    }
}

fn child_zettels(zettel: &Zettel) -> impl Iterator<Item = &Zettel> {
    zettel.body.iter().filter_map(|block| match block {
        BodyBlock::ChildZettel(child) => Some(child.as_ref()),
        _ => None,
    })
}
