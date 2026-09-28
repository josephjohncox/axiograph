//! Operational source diagnostics. None of these types enter accepted AST or kernel IR.
use std::{
    collections::BTreeMap,
    fmt::Write as _,
    path::{Path, PathBuf},
};

use axiograph_dsl::schema_v1::CanonicalSyntacticAddressV1;
use axiograph_kernel::{
    CanonicalModuleSource, KernelCompileError, KernelDiagnosticCollectionV1,
    KernelDiagnosticLimits, KernelDiagnosticSubjectV1, RevisionDigestV2,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourcePositionV1 {
    /// One-based physical line and Unicode scalar column.
    pub line: usize,
    pub column: usize,
    /// Zero-based LSP line and UTF-16 code-unit column.
    pub lsp_line: u32,
    pub lsp_character: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CarrierKindV1 {
    Object,
    RelationObject,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum SyntacticDiagnosticSubjectV1 {
    ParseLine {
        line: usize,
    },
    ImportName {
        import_index: usize,
        name: String,
    },
    RoleTypeCarrier {
        schema_index: usize,
        relation_index: usize,
        role_index: usize,
        schema: String,
        relation: String,
        role: String,
        carrier_kind: CarrierKindV1,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum DiagnosticSubjectV1 {
    Syntactic {
        subject: SyntacticDiagnosticSubjectV1,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceLocationV1 {
    pub path: String,
    /// Absent when parsing has not established a module identity.
    pub module_name: Option<String>,
    pub revision_digest: String,
    /// A parser-owned syntactic address. Checked refs are exposed only by a
    /// successfully compiled snapshot, never by this failure sidecar.
    pub subject: DiagnosticSubjectV1,
    /// Zero-based half-open UTF-8 byte range in the retained exact source image.
    pub byte_start: usize,
    pub byte_end: usize,
    pub start: SourcePositionV1,
    pub end: SourcePositionV1,
    /// A physical-line excerpt, at most 240 Unicode scalars, without CR/LF.
    pub excerpt: String,
    pub excerpt_byte_start: usize,
    pub excerpt_truncated: bool,
    /// Advisory declared-name suggestion; never an edit or authority.
    pub suggested_name: Option<String>,
}

#[derive(Debug)]
pub(crate) struct CanonicalSourceDiagnostic {
    pub cause: KernelCompileError,
    pub location: Option<SourceLocationV1>,
}

#[derive(Debug)]
pub(crate) struct CanonicalSourceDiagnosticCollection {
    pub diagnostics: Vec<CanonicalSourceDiagnostic>,
    pub observed_errors: usize,
    pub omitted_observed_errors: usize,
    pub work_units: usize,
    pub work_exhausted: bool,
}

impl CanonicalSourceDiagnosticCollection {
    pub(crate) fn truncated(&self) -> bool {
        self.omitted_observed_errors > 0 || self.work_exhausted
    }
}

impl std::fmt::Display for CanonicalSourceDiagnosticCollection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.diagnostics.first() {
            Some(first) => first.fmt(formatter),
            None if self.work_exhausted => formatter.write_str(
                "canonical diagnostic work bound exhausted before a displayable error was retained",
            ),
            None => {
                formatter.write_str("canonical compilation failed without a retained diagnostic")
            }
        }
    }
}

impl std::error::Error for CanonicalSourceDiagnosticCollection {}

impl std::fmt::Display for CanonicalSourceDiagnostic {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.cause.fmt(formatter)
    }
}

impl std::error::Error for CanonicalSourceDiagnostic {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        // Transparent presentation: retain nonredundant causes, not an identical
        // semantic message repeated by an operational wrapper.
        self.cause.source()
    }
}

pub(super) fn compile_diagnostic(
    cause: KernelCompileError,
    sources: &BTreeMap<String, CanonicalModuleSource>,
    paths: &BTreeMap<String, PathBuf>,
) -> anyhow::Error {
    if !matches!(cause, KernelCompileError::RoleCarrier { .. }) {
        return cause.into();
    }
    let mut work = DiagnosticWorkBudget::new(KernelDiagnosticLimits::default().max_work, 0);
    let location = locate_kernel_diagnostic(
        &cause,
        &KernelDiagnosticSubjectV1::Unlocated,
        sources,
        paths,
        &mut work,
    );
    CanonicalSourceDiagnostic { cause, location }.into()
}

struct BoundedMessage {
    text: String,
    max_bytes: usize,
}

impl std::fmt::Write for BoundedMessage {
    fn write_str(&mut self, value: &str) -> std::fmt::Result {
        if self.text.len().saturating_add(value.len()) > self.max_bytes {
            return Err(std::fmt::Error);
        }
        self.text.push_str(value);
        Ok(())
    }
}

fn bounded_error_message(cause: &KernelCompileError, max_bytes: usize) -> Option<String> {
    let mut output = BoundedMessage {
        text: String::with_capacity(max_bytes.min(1024)),
        max_bytes,
    };
    write!(&mut output, "{cause}").ok()?;
    Some(output.text)
}

pub(super) fn parse_diagnostic_collection(
    path: &Path,
    exact_text: &str,
    line: usize,
    message: String,
    work_units: usize,
) -> CanonicalSourceDiagnosticCollection {
    let limits = KernelDiagnosticLimits::default();
    let cause = KernelCompileError::Parse { line, message };
    let Some(_) = bounded_error_message(&cause, limits.max_message_bytes) else {
        return CanonicalSourceDiagnosticCollection {
            diagnostics: Vec::new(),
            observed_errors: 1,
            omitted_observed_errors: 1,
            work_units: work_units.min(limits.max_work),
            work_exhausted: work_units > limits.max_work,
        };
    };
    let range = physical_line_range(exact_text, line);
    let location = range.and_then(|range| {
        source_location(
            path,
            None,
            RevisionDigestV2::from_accepted_text(exact_text).as_str(),
            DiagnosticSubjectV1::Syntactic {
                subject: SyntacticDiagnosticSubjectV1::ParseLine { line },
            },
            exact_text,
            range,
            None,
        )
    });
    CanonicalSourceDiagnosticCollection {
        diagnostics: vec![CanonicalSourceDiagnostic { cause, location }],
        observed_errors: 1,
        omitted_observed_errors: 0,
        work_units: work_units.min(limits.max_work),
        work_exhausted: work_units > limits.max_work,
    }
}

pub(super) fn retain_authoritative_compile_error(
    collection: &mut KernelDiagnosticCollectionV1,
    cause: KernelCompileError,
) {
    retain_authoritative_compile_error_at(collection, cause, false);
}

pub(super) fn retain_authoritative_loader_error(
    collection: &mut KernelDiagnosticCollectionV1,
    cause: KernelCompileError,
) {
    retain_authoritative_compile_error_at(collection, cause, true);
}

fn retain_authoritative_compile_error_at(
    collection: &mut KernelDiagnosticCollectionV1,
    cause: KernelCompileError,
    insert_at_end: bool,
) {
    if collection
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.cause == cause)
    {
        collection.authoritative_error_observed = true;
        return;
    }
    let previously_observed = collection.authoritative_error_observed;
    collection.authoritative_error_observed = true;
    if !previously_observed {
        collection.observed_errors = collection.observed_errors.saturating_add(1);
    }
    let Some(message) = bounded_error_message(&cause, collection.limits.max_message_bytes) else {
        if !previously_observed {
            collection.omitted_observed_errors =
                collection.omitted_observed_errors.saturating_add(1);
        }
        return;
    };
    let mut evicted = 0_usize;
    while collection.diagnostics.len() >= collection.limits.max_items
        || collection.message_bytes.saturating_add(message.len())
            > collection.limits.max_message_bytes
    {
        let Some(removed) = collection.diagnostics.pop() else {
            break;
        };
        let removed_bytes =
            bounded_error_message(&removed.cause, collection.limits.max_message_bytes)
                .map_or(0, |message| message.len());
        collection.message_bytes = collection.message_bytes.saturating_sub(removed_bytes);
        evicted = evicted.saturating_add(1);
    }
    if collection.diagnostics.len() >= collection.limits.max_items
        || collection.message_bytes.saturating_add(message.len())
            > collection.limits.max_message_bytes
    {
        if !previously_observed {
            collection.omitted_observed_errors =
                collection.omitted_observed_errors.saturating_add(1);
        }
        return;
    }
    if previously_observed {
        collection.omitted_observed_errors = collection.omitted_observed_errors.saturating_sub(1);
    }
    collection.omitted_observed_errors = collection.omitted_observed_errors.saturating_add(evicted);
    collection.message_bytes += message.len();
    let diagnostic = axiograph_kernel::KernelCompilationDiagnosticV1 {
        cause,
        subject: KernelDiagnosticSubjectV1::Unlocated,
    };
    if insert_at_end {
        collection.diagnostics.push(diagnostic);
    } else {
        collection.diagnostics.insert(0, diagnostic);
    }
}

pub(super) fn collect_kernel_diagnostics(
    collection: KernelDiagnosticCollectionV1,
    sources: &BTreeMap<String, CanonicalModuleSource>,
    paths: &BTreeMap<String, PathBuf>,
) -> CanonicalSourceDiagnosticCollection {
    let KernelDiagnosticCollectionV1 {
        diagnostics: kernel_diagnostics,
        observed_errors,
        work_units,
        work_exhausted,
        limits,
        ..
    } = collection;
    let mut work = DiagnosticWorkBudget::new(limits.max_work, work_units);
    let mut diagnostics = Vec::new();
    let mut message_bytes = 0_usize;
    for diagnostic in kernel_diagnostics {
        let location = locate_kernel_diagnostic(
            &diagnostic.cause,
            &diagnostic.subject,
            sources,
            paths,
            &mut work,
        );
        let next_message_bytes = message_bytes.saturating_add(diagnostic.cause.to_string().len());
        if diagnostics.len() >= limits.max_items || next_message_bytes > limits.max_message_bytes {
            continue;
        }
        message_bytes = next_message_bytes;
        diagnostics.push(CanonicalSourceDiagnostic {
            location,
            cause: diagnostic.cause,
        });
    }
    let omitted_observed_errors = observed_errors.saturating_sub(diagnostics.len());
    CanonicalSourceDiagnosticCollection {
        diagnostics,
        observed_errors,
        omitted_observed_errors,
        work_units: work.used,
        work_exhausted: work_exhausted || work.exhausted,
    }
}

#[derive(Debug)]
struct DiagnosticWorkBudget {
    used: usize,
    max: usize,
    exhausted: bool,
}

impl DiagnosticWorkBudget {
    fn new(max: usize, used: usize) -> Self {
        Self {
            used: used.min(max),
            max,
            exhausted: used > max,
        }
    }

    fn charge(&mut self, amount: usize) -> bool {
        let Some(next) = self.used.checked_add(amount) else {
            self.used = self.max;
            self.exhausted = true;
            return false;
        };
        if next > self.max {
            self.used = self.max;
            self.exhausted = true;
            return false;
        }
        self.used = next;
        true
    }
}

fn locate_kernel_diagnostic(
    error: &KernelCompileError,
    subject: &KernelDiagnosticSubjectV1,
    sources: &BTreeMap<String, CanonicalModuleSource>,
    paths: &BTreeMap<String, PathBuf>,
    work: &mut DiagnosticWorkBudget,
) -> Option<SourceLocationV1> {
    let (module, address) = match subject {
        KernelDiagnosticSubjectV1::Syntactic { module, address } => (module, address),
        KernelDiagnosticSubjectV1::Unlocated => {
            let KernelCompileError::RoleCarrier {
                module,
                schema_index,
                relation_index,
                role_index,
                ..
            } = error
            else {
                return None;
            };
            return locate_kernel_diagnostic(
                error,
                &KernelDiagnosticSubjectV1::Syntactic {
                    module: module.clone(),
                    address: CanonicalSyntacticAddressV1::RoleTypeCarrier {
                        schema_index: *schema_index,
                        relation_index: *relation_index,
                        role_index: *role_index,
                    },
                },
                sources,
                paths,
                work,
            );
        }
    };
    let source = sources.get(module)?;
    let path = paths.get(module)?;
    match address {
        CanonicalSyntacticAddressV1::RoleTypeCarrier {
            schema_index,
            relation_index,
            role_index,
        } => {
            let KernelCompileError::RoleCarrier { cause, .. } = error else {
                return None;
            };
            let (target, carrier_kind) = match cause.as_ref() {
                KernelCompileError::UnknownObjectTarget { target, .. } => {
                    (target, CarrierKindV1::Object)
                }
                KernelCompileError::UnknownRelationTarget { target, .. } => {
                    (target, CarrierKindV1::RelationObject)
                }
                _ => return None,
            };
            let schema = source.parsed().schemas.get(*schema_index)?;
            let relation = schema.relations.get(*relation_index)?;
            let role = relation.fields.get(*role_index)?;
            let occurrence = source.source_map().occurrence(address)?;
            if source.exact_text().get(occurrence.bytes.clone())? != target {
                return None;
            }
            let candidate_count = match carrier_kind {
                CarrierKindV1::Object => schema.objects.len(),
                CarrierKindV1::RelationObject => schema.relations.len(),
            };
            let candidates = if candidate_count > 4096 || !work.charge(candidate_count) {
                Vec::new()
            } else {
                match carrier_kind {
                    CarrierKindV1::Object => schema
                        .objects
                        .iter()
                        .map(String::as_str)
                        .collect::<Vec<_>>(),
                    CarrierKindV1::RelationObject => schema
                        .relations
                        .iter()
                        .map(|relation| relation.name.as_str())
                        .collect(),
                }
            };
            source_location(
                path,
                Some(module),
                source.revision().as_str(),
                DiagnosticSubjectV1::Syntactic {
                    subject: SyntacticDiagnosticSubjectV1::RoleTypeCarrier {
                        schema_index: *schema_index,
                        relation_index: *relation_index,
                        role_index: *role_index,
                        schema: schema.name.clone(),
                        relation: relation.name.clone(),
                        role: role.field.clone(),
                        carrier_kind,
                    },
                },
                source.exact_text(),
                occurrence.bytes.clone(),
                suggested_name(target, &candidates),
            )
        }
        CanonicalSyntacticAddressV1::ImportName { import_index } => {
            let import = source.parsed().imports.get(*import_index)?;
            let occurrence = source.source_map().occurrence(address)?;
            if source.exact_text().get(occurrence.bytes.clone())? != import {
                return None;
            }
            source_location(
                path,
                Some(module),
                source.revision().as_str(),
                DiagnosticSubjectV1::Syntactic {
                    subject: SyntacticDiagnosticSubjectV1::ImportName {
                        import_index: *import_index,
                        name: import.clone(),
                    },
                },
                source.exact_text(),
                occurrence.bytes.clone(),
                None,
            )
        }
        _ => None,
    }
}

fn physical_line_range(text: &str, requested_line: usize) -> Option<std::ops::Range<usize>> {
    if requested_line == 0 {
        return None;
    }
    let mut line = 1;
    let mut start = 0;
    for (index, byte) in text.bytes().enumerate() {
        if line == requested_line && byte == b'\n' {
            return Some(
                start
                    ..index.saturating_sub(usize::from(
                        index > start && text.as_bytes()[index - 1] == b'\r',
                    )),
            );
        }
        if byte == b'\n' {
            line += 1;
            start = index + 1;
        }
    }
    (line == requested_line).then_some(start..text.len())
}

fn source_location(
    path: &Path,
    module_name: Option<&String>,
    revision_digest: &str,
    subject: DiagnosticSubjectV1,
    text: &str,
    bytes: std::ops::Range<usize>,
    suggested_name: Option<String>,
) -> Option<SourceLocationV1> {
    if bytes.start > bytes.end || text.get(bytes.clone()).is_none() {
        return None;
    }
    let start = position(text, bytes.start)?;
    let end = position(text, bytes.end)?;
    let line_start = text[..bytes.start].rfind('\n').map_or(0, |index| index + 1);
    let line_end = text[bytes.start..]
        .find('\n')
        .map_or(text.len(), |index| bytes.start + index);
    let line = text[line_start..line_end].trim_end_matches('\r');
    let prefix_chars = text[line_start..bytes.start].chars().count();
    let skip = prefix_chars.saturating_sub(80);
    let excerpt_offset = line
        .char_indices()
        .nth(skip)
        .map_or(line.len(), |(index, _)| index);
    let excerpt = line[excerpt_offset..].chars().take(240).collect::<String>();
    let excerpt_truncated = excerpt_offset != 0 || excerpt.chars().count() < line.chars().count();
    Some(SourceLocationV1 {
        path: path.to_str()?.to_string(),
        module_name: module_name.cloned(),
        revision_digest: revision_digest.to_string(),
        subject,
        byte_start: bytes.start,
        byte_end: bytes.end,
        start,
        end,
        excerpt,
        excerpt_byte_start: line_start + excerpt_offset,
        excerpt_truncated,
        suggested_name,
    })
}

pub(crate) fn location_schema() -> serde_json::Value {
    use serde_json::json;
    fn object(properties: serde_json::Value) -> serde_json::Value {
        let required = properties
            .as_object()
            .map(|object| object.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        json!({"type":"object","additionalProperties":false,"required":required,"properties":properties})
    }
    let count = json!({"type":"integer","minimum":0});
    let human = json!({"type":"integer","minimum":1});
    let string = json!({"type":"string"});
    let position =
        object(json!({"line":human,"column":human,"lsp_line":count,"lsp_character":count}));
    let parse_subject = object(json!({"kind":{"const":"parse_line"},"line":human}));
    let import_subject =
        object(json!({"kind":{"const":"import_name"},"import_index":count,"name":string}));
    let role_subject = object(
        json!({"kind":{"const":"role_type_carrier"},"schema_index":count,
        "relation_index":count,"role_index":count,"schema":string,"relation":string,"role":string,
        "carrier_kind":{"enum":["object","relation_object"]}}),
    );
    let syntactic = object(
        json!({"status":{"const":"syntactic"},"subject":{"oneOf":[parse_subject,import_subject,role_subject]}}),
    );
    object(
        json!({"path":string,"module_name":{"type":["string","null"]},"revision_digest":string,
        "subject":syntactic,"byte_start":count,"byte_end":count,"start":position,"end":position,
        "excerpt":{"type":"string","maxLength":240},"excerpt_byte_start":count,"excerpt_truncated":{"type":"boolean"},
        "suggested_name":{"type":["string","null"]}}),
    )
}

fn position(text: &str, offset: usize) -> Option<SourcePositionV1> {
    let prefix = text.get(..offset)?;
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count();
    let column_prefix = &prefix[prefix.rfind('\n').map_or(0, |index| index + 1)..];
    Some(SourcePositionV1 {
        line: line + 1,
        column: column_prefix.chars().count() + 1,
        lsp_line: u32::try_from(line).ok()?,
        lsp_character: u32::try_from(column_prefix.encode_utf16().count()).ok()?,
    })
}

fn suggested_name(target: &str, candidates: &[&str]) -> Option<String> {
    // Bound work independently of the parser's aggregate source bound. Refuse ties.
    if !(4..=128).contains(&target.len()) || candidates.len() > 4096 {
        return None;
    }
    let target = target.chars().collect::<Vec<_>>();
    let mut best = 3;
    let mut suggestion = None;
    for candidate in candidates {
        if !(4..=128).contains(&candidate.len()) {
            continue;
        }
        let chars = candidate.chars().collect::<Vec<_>>();
        if chars.len().abs_diff(target.len()) > 2 {
            continue;
        }
        let mut row = (0..=target.len()).collect::<Vec<_>>();
        for (i, ch) in chars.iter().enumerate() {
            let mut diagonal = row[0];
            row[0] = i + 1;
            for (j, target_ch) in target.iter().enumerate() {
                let previous = row[j + 1];
                row[j + 1] = (row[j] + 1)
                    .min(previous + 1)
                    .min(diagonal + usize::from(ch != target_ch));
                diagonal = previous;
            }
        }
        let distance = row[target.len()];
        if distance < best {
            best = distance;
            suggestion = Some((*candidate).to_string());
        } else if distance == best {
            suggestion = None;
        }
    }
    suggestion
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_position_utf16_crlf_boundaries_and_suggestion_work_bounds() {
        // Astral characters are not canonical identifiers. Test the shared coordinate
        // primitive without broadening the grammar to manufacture an astral role name.
        let text = "# 😀\r\n\u{2003}😀Compny";
        let offset = text.find("Compny").unwrap();
        let position = position(text, offset).unwrap();
        assert_eq!(
            (
                position.line,
                position.column,
                position.lsp_line,
                position.lsp_character
            ),
            (2, 3, 1, 3)
        );
        assert!(super::position(text, offset - 1).is_none());
        assert!(super::position(text, text.len() + 1).is_none());
        assert_eq!(
            suggested_name("Compny", &["Company"]),
            Some("Company".into())
        );
        assert_eq!(suggested_name("Compny", &["Company", "Compnay"]), None);
        assert_eq!(suggested_name("Pers", &["Per"]), None);
        assert_eq!(suggested_name("Compny", &vec!["Company"; 4097]), None);
        assert_eq!(suggested_name(&"x".repeat(129), &["Company"]), None);
    }

    #[test]
    fn authoritative_loader_failure_survives_kernel_work_exhaustion_once() {
        let root = CanonicalModuleSource::parse(b"module Root\nimport Missing\n".to_vec()).unwrap();
        let request = axiograph_kernel::KernelCompilationRequest {
            repository_id: axiograph_kernel::RepositoryIdV2::from_descriptor_bytes(
                b"loader-work-exhaustion",
            ),
            accepted_snapshot_id: axiograph_kernel::SnapshotIdV2::from_canonical_fields(&[root
                .exact_text()
                .as_bytes()]),
            root_module: "Root".into(),
            modules: vec![root],
        };
        let authoritative = KernelCompileError::UnknownImport {
            module: "Root".into(),
            import: "Missing".into(),
        };
        let authoritative_occurrence = ("Root".to_string(), 0_usize, "Missing".to_string());
        let known = std::collections::BTreeSet::from([authoritative_occurrence.clone()]);
        let mut collection =
            axiograph_kernel::CanonicalCompiler::collect_diagnostics_for_known_import_failures_with_authoritative_error(
                &request,
                KernelDiagnosticLimits {
                    max_work: 1,
                    ..KernelDiagnosticLimits::default()
                },
                &known,
                &authoritative_occurrence,
                &authoritative,
            )
            .unwrap();
        assert!(!collection.authoritative_error_observed);
        retain_authoritative_compile_error(&mut collection, authoritative);
        assert!(collection.authoritative_error_observed);
        assert_eq!(collection.observed_errors, 1);
        assert_eq!(collection.omitted_observed_errors, 0);
        assert_eq!(collection.diagnostics.len(), 1);
        assert!(collection.work_exhausted);
    }

    #[test]
    fn source_location_refuses_missing_occurrences_and_mismatched_tokens() {
        let text = "module M\nschema S:\n object Company\n relation R(x: Compny)\n";
        let source = CanonicalModuleSource::parse(text.as_bytes().to_vec()).unwrap();
        let sources = BTreeMap::from([("M".into(), source)]);
        let paths = BTreeMap::from([("M".into(), PathBuf::from("/workspace/M.axi"))]);
        for (role_index, target) in [(1, "Compny"), (0, "Other")] {
            let error = KernelCompileError::RoleCarrier {
                module: "M".into(),
                schema_index: 0,
                relation_index: 0,
                role_index,
                cause: Box::new(KernelCompileError::UnknownObjectTarget {
                    schema: "M.S".into(),
                    site: "not used as a locator".into(),
                    target: target.into(),
                }),
            };
            let mut work = DiagnosticWorkBudget::new(65_536, 0);
            assert!(locate_kernel_diagnostic(
                &error,
                &KernelDiagnosticSubjectV1::Unlocated,
                &sources,
                &paths,
                &mut work,
            )
            .is_none());
        }
    }
}
