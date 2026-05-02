use std::collections::BTreeMap;

use zorg_core::{
    BodyBlock, Diagnostic, Reference, ReferenceTarget, ResolvedReference, SourcePath, SourceSpan,
    Zettel, ZettelDocument, ZettelId, ZettelKey,
};

/// A resolved symbol table entry for a canonical zettel ID.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SymbolOccurrence {
    /// Source path for the declared target.
    pub path: Option<SourcePath>,
    /// Source span for the declaring zettel.
    pub span: Option<SourceSpan>,
    /// Parser-local zettel key for the declared target.
    pub key: ZettelKey,
}

/// In-memory zettel symbol table used by validation, store, query, and LSP callers.
#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct SymbolTable {
    symbols: BTreeMap<ZettelId, Vec<SymbolOccurrence>>,
}

impl SymbolTable {
    /// Builds a symbol table from a single parsed document.
    #[must_use]
    pub fn from_document(document: &ZettelDocument) -> Self {
        Self::from_corpus(std::slice::from_ref(document))
    }

    /// Builds a symbol table from a parsed corpus.
    #[must_use]
    pub fn from_corpus(documents: &[ZettelDocument]) -> Self {
        let mut table = Self::default();
        for document in documents {
            collect_symbols(&document.root, None, &mut table);
        }
        table
    }

    /// Returns all occurrences for a canonical target ID.
    #[must_use]
    pub fn occurrences(&self, id: &ZettelId) -> &[SymbolOccurrence] {
        self.symbols.get(id).map(Vec::as_slice).unwrap_or(&[])
    }

    fn insert(&mut self, id: ZettelId, occurrence: SymbolOccurrence) {
        let occurrences = self.symbols.entry(id).or_default();
        if !occurrences
            .iter()
            .any(|candidate| candidate.key == occurrence.key && candidate.path == occurrence.path)
        {
            occurrences.push(occurrence);
        }
    }
}

/// Result of resolving links and local IDs.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ResolutionReport {
    /// Resolution diagnostics produced while walking the document or corpus.
    pub diagnostics: Vec<Diagnostic>,
    /// Symbol table used for this resolution pass.
    pub symbols: SymbolTable,
}

impl ResolutionReport {
    /// Returns true when resolution produced no error diagnostics.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.diagnostics.is_empty()
    }
}

#[derive(Debug, Clone, Default)]
struct ResolutionContext {
    absolute_ancestor: Option<ZettelId>,
    parent_canonical_id: Option<ZettelId>,
}

/// Resolves local IDs and outgoing links in a single parsed document.
pub fn resolve_document(document: &mut ZettelDocument) -> ResolutionReport {
    let symbols = SymbolTable::from_document(document);
    let diagnostics = resolve_document_with_symbols(document, &symbols);
    ResolutionReport {
        diagnostics,
        symbols,
    }
}

/// Resolves local IDs and outgoing links across a parsed corpus.
pub fn resolve_corpus(documents: &mut [ZettelDocument]) -> ResolutionReport {
    let symbols = SymbolTable::from_corpus(documents);
    let mut diagnostics = Vec::new();

    for document in documents {
        diagnostics.extend(resolve_document_with_symbols(document, &symbols));
    }

    ResolutionReport {
        diagnostics,
        symbols,
    }
}

fn collect_symbols(zettel: &Zettel, absolute_ancestor: Option<&ZettelId>, table: &mut SymbolTable) {
    let occurrence = SymbolOccurrence {
        path: zettel.path.clone(),
        span: zettel.span,
        key: zettel.key.clone(),
    };

    if let Some(id) = &zettel.id {
        table.insert(id.clone(), occurrence.clone());
    }

    if let Some(canonical_local) = canonical_local_id(zettel, absolute_ancestor) {
        table.insert(canonical_local, occurrence);
    }

    let child_absolute_ancestor = zettel.id.as_ref().or(absolute_ancestor);
    for child in child_zettels(zettel) {
        collect_symbols(child, child_absolute_ancestor, table);
    }
}

fn resolve_document_with_symbols(
    document: &mut ZettelDocument,
    symbols: &SymbolTable,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    resolve_zettel(
        &mut document.root,
        symbols,
        ResolutionContext::default(),
        &mut diagnostics,
    );

    document.diagnostics.extend(diagnostics.clone());
    document.root.diagnostics = document.diagnostics.clone();
    diagnostics
}

fn resolve_zettel(
    zettel: &mut Zettel,
    symbols: &SymbolTable,
    context: ResolutionContext,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let canonical_local = canonical_local_id(zettel, context.absolute_ancestor.as_ref());
    let canonical_id = zettel.id.clone().or(canonical_local);
    zettel.canonical_id = canonical_id.clone();
    zettel.resolved_links.clear();

    for reference in &zettel.links {
        match resolve_reference(
            reference,
            canonical_id.as_ref(),
            context.parent_canonical_id.as_ref(),
            symbols,
            zettel.path.as_ref(),
        ) {
            Ok(resolved) => zettel.resolved_links.push(resolved),
            Err(diagnostic) => {
                let diagnostic = *diagnostic;
                zettel.diagnostics.push(diagnostic.clone());
                diagnostics.push(diagnostic);
            }
        }
    }

    let child_context = ResolutionContext {
        absolute_ancestor: zettel.id.clone().or(context.absolute_ancestor),
        parent_canonical_id: canonical_id,
    };

    for child in child_zettels_mut(zettel) {
        resolve_zettel(child, symbols, child_context.clone(), diagnostics);
    }
}

fn resolve_reference(
    reference: &Reference,
    current_canonical_id: Option<&ZettelId>,
    parent_canonical_id: Option<&ZettelId>,
    symbols: &SymbolTable,
    path: Option<&SourcePath>,
) -> Result<ResolvedReference, Box<Diagnostic>> {
    let target_id = match &reference.target {
        ReferenceTarget::Absolute(id) => id.clone(),
        ReferenceTarget::Child(relative_id) => {
            let Some(current_id) = current_canonical_id else {
                return Err(Box::new(resolution_diagnostic(
                    "reference.missing_current_id",
                    format!(
                        "child-relative reference `{}` requires the current zettel to have a canonical ID",
                        reference.raw
                    ),
                    reference.span,
                    path,
                )));
            };
            ZettelId::unchecked(format!("{}/{}", current_id.as_str(), relative_id.as_str()))
        }
        ReferenceTarget::Sibling(relative_id) => {
            let Some(parent_id) =
                sibling_base_id(current_canonical_id).or_else(|| parent_canonical_id.cloned())
            else {
                return Err(Box::new(resolution_diagnostic(
                    "reference.missing_parent_id",
                    format!(
                        "sibling-relative reference `{}` requires the parent zettel to have a canonical ID",
                        reference.raw
                    ),
                    reference.span,
                    path,
                )));
            };
            ZettelId::unchecked(format!("{}/{}", parent_id.as_str(), relative_id.as_str()))
        }
        ReferenceTarget::LocalDeclaration(local_id) => {
            let Some(current_id) = current_canonical_id else {
                return Err(Box::new(resolution_diagnostic(
                    "reference.missing_current_id",
                    format!(
                        "local reference `{}` requires the current zettel to have a canonical ID",
                        reference.raw
                    ),
                    reference.span,
                    path,
                )));
            };
            ZettelId::unchecked(format!("{}/{}", current_id.as_str(), local_id.as_str()))
        }
    };

    let occurrences = symbols.occurrences(&target_id);
    match occurrences.len() {
        0 => Err(Box::new(unresolved_diagnostic(reference, &target_id, path))),
        1 => Ok(ResolvedReference {
            reference: reference.clone(),
            target_id,
        }),
        _ => Err(Box::new(resolution_diagnostic(
            "reference.ambiguous",
            format!(
                "reference `{}` resolves ambiguously to duplicate target `@{}`",
                reference.raw,
                target_id.as_str()
            ),
            reference.span,
            path,
        ))),
    }
}

fn sibling_base_id(current_canonical_id: Option<&ZettelId>) -> Option<ZettelId> {
    let current = current_canonical_id?.as_str();
    let (base, _) = current.rsplit_once('/')?;
    Some(ZettelId::unchecked(base))
}

fn unresolved_diagnostic(
    reference: &Reference,
    target_id: &ZettelId,
    path: Option<&SourcePath>,
) -> Diagnostic {
    let code = match &reference.target {
        ReferenceTarget::Absolute(_) => "reference.unresolved_absolute",
        ReferenceTarget::Child(_) => "reference.unresolved_child",
        ReferenceTarget::Sibling(_) => "reference.unresolved_sibling",
        ReferenceTarget::LocalDeclaration(_) => "reference.unresolved_local",
    };

    resolution_diagnostic(
        code,
        format!(
            "reference `{}` does not resolve to target `@{}`",
            reference.raw,
            target_id.as_str()
        ),
        reference.span,
        path,
    )
}

fn canonical_local_id(zettel: &Zettel, absolute_ancestor: Option<&ZettelId>) -> Option<ZettelId> {
    let local_id = zettel.local_id.as_ref()?;
    let ancestor_id = absolute_ancestor?;
    Some(ZettelId::unchecked(format!(
        "{}/{}",
        ancestor_id.as_str(),
        local_id.as_str()
    )))
}

fn child_zettels(zettel: &Zettel) -> impl Iterator<Item = &Zettel> {
    zettel.body.iter().filter_map(|block| match block {
        BodyBlock::ChildZettel(child) => Some(child.as_ref()),
        _ => None,
    })
}

fn child_zettels_mut(zettel: &mut Zettel) -> impl Iterator<Item = &mut Zettel> {
    zettel.body.iter_mut().filter_map(|block| match block {
        BodyBlock::ChildZettel(child) => Some(child.as_mut()),
        _ => None,
    })
}

fn resolution_diagnostic(
    code: impl Into<String>,
    message: impl Into<String>,
    span: Option<SourceSpan>,
    path: Option<&SourcePath>,
) -> Diagnostic {
    let mut diagnostic = Diagnostic::semantic_validation(code, message, span);
    if let Some(path) = path {
        diagnostic.path = Some(path.clone());
    }
    diagnostic
}

#[cfg(test)]
mod tests {
    use super::{resolve_corpus, resolve_document};
    use std::fs;
    use std::path::{Path, PathBuf};
    use zorg_core::{BodyBlock, Zettel, ZettelDocument, ZettelId};

    use crate::{parse_zettel_document_with_path, validate_corpus};

    #[test]
    fn resolves_absolute_child_sibling_and_local_targets() {
        let mut document = parse_fixture("nested.z");
        let report = resolve_document(&mut document);

        assert!(
            report.is_valid(),
            "unexpected resolution diagnostics: {:#?}",
            report.diagnostics
        );

        assert_eq!(
            document
                .root
                .canonical_id
                .as_ref()
                .expect("root ID")
                .as_str(),
            "project"
        );
        assert_resolved(&document.root, "#project/plan", "project/plan");

        let plan = child_with_id(&document.root, "project/plan").expect("plan child");
        assert_resolved(plan, "+task", "project/plan/task");
        assert_resolved(plan, "~review", "project/review");

        let task = child_with_local_id(plan, "task").expect("task child");
        assert_eq!(
            task.canonical_id.as_ref().expect("task canonical").as_str(),
            "project/plan/task"
        );

        let review = child_with_id(plan, "project/review").expect("review child");
        assert_resolved(review, "~plan", "project/plan");
        assert_resolved(review, "#project/plan/task", "project/plan/task");
    }

    #[test]
    fn resolves_directory_local_ids() {
        let mut document = parse_fixture("dir/init.z");
        let report = resolve_document(&mut document);
        assert!(report.is_valid());

        let child = child_with_local_id(&document.root, "child").expect("directory child");
        assert_eq!(
            child.canonical_id.as_ref().expect("canonical ID").as_str(),
            "dir-example/child"
        );
    }

    #[test]
    fn reports_unresolved_and_missing_relative_contexts() {
        let mut anonymous = parse_source(
            "\
missing

- Anonymous child links to +target.
- @target Target.
",
            "anonymous-links.z",
        );
        let report = resolve_document(&mut anonymous);

        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code.as_deref() == Some("reference.missing_current_id")
        }));

        let mut unresolved = parse_source(
            "\
%%% @root #z/ref
Root
%%%

This links to #missing.
",
            "unresolved-link.z",
        );
        let report = resolve_document(&mut unresolved);

        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code.as_deref() == Some("reference.unresolved_absolute")
        }));
    }

    #[test]
    fn reports_ambiguous_duplicate_targets_in_corpus() {
        let mut documents = vec![
            parse_source("%%% @root #z/ref\nRoot\n%%%\n\nSee #dupe.\n", "root.z"),
            parse_source("%%% @dupe #z/ref\nFirst\n%%%\n", "first.z"),
            parse_source("%%% @dupe #z/ref\nSecond\n%%%\n", "second.z"),
        ];
        let report = resolve_corpus(&mut documents);

        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code.as_deref() == Some("reference.ambiguous")
                && diagnostic.message.contains("@dupe")
        }));
    }

    #[test]
    fn valid_fixture_corpus_validates_and_resolves() {
        let mut documents = [
            "minimal.z",
            "nested.z",
            "query_and_template.z",
            "dir/init.z",
        ]
        .into_iter()
        .map(parse_fixture)
        .collect::<Vec<_>>();

        let validation = validate_corpus(&documents);
        assert!(
            validation.is_valid(),
            "unexpected validation diagnostics: {:#?}",
            validation.diagnostics
        );

        let resolution = resolve_corpus(&mut documents);
        assert!(
            resolution.is_valid(),
            "unexpected resolution diagnostics: {:#?}",
            resolution.diagnostics
        );
    }

    fn assert_resolved(zettel: &Zettel, raw: &str, target: &str) {
        assert!(
            zettel.resolved_links.iter().any(|link| {
                link.reference.raw == raw && link.target_id == ZettelId::unchecked(target)
            }),
            "missing resolved link {raw} -> @{target}: {:#?}",
            zettel.resolved_links
        );
    }

    fn parse_fixture(path: impl AsRef<Path>) -> ZettelDocument {
        let path = fixture_path(path);
        let source = fs::read_to_string(&path).expect("fixture");
        parse_zettel_document_with_path(&source, path).expect("parse document")
    }

    fn parse_source(source: &str, path: &str) -> ZettelDocument {
        parse_zettel_document_with_path(source, fixture_path(path)).expect("parse document")
    }

    fn child_with_id<'a>(zettel: &'a Zettel, id: &str) -> Option<&'a Zettel> {
        child_zettels(zettel).find(|child| {
            child
                .id
                .as_ref()
                .is_some_and(|candidate| candidate.as_str() == id)
        })
    }

    fn child_with_local_id<'a>(zettel: &'a Zettel, local_id: &str) -> Option<&'a Zettel> {
        child_zettels(zettel).find(|child| {
            child
                .local_id
                .as_ref()
                .is_some_and(|candidate| candidate.as_str() == local_id)
        })
    }

    fn child_zettels(zettel: &Zettel) -> impl Iterator<Item = &Zettel> {
        zettel.body.iter().filter_map(|block| match block {
            BodyBlock::ChildZettel(child) => Some(child.as_ref()),
            _ => None,
        })
    }

    fn fixture_path(path: impl AsRef<Path>) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("fixtures/corpus")
            .join(path)
    }
}
