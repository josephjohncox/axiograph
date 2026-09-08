//! Operational source diagnostics. None of these types enter accepted AST or kernel IR.
use std::{collections::BTreeMap, path::PathBuf};

use axiograph_kernel::{CanonicalModuleSource, KernelCompileError};
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct RoleCarrierSubjectV1 {
    pub schema_index: usize,
    pub relation_index: usize,
    pub role_index: usize,
    pub schema: String,
    pub relation: String,
    pub role: String,
    pub carrier_kind: CarrierKindV1,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CarrierKindV1 {
    Object,
    RelationObject,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceLocationV1 {
    pub path: String,
    pub module_name: String,
    pub revision_digest: String,
    /// Syntactic occurrence address, NOT a compiled/accepted KernelRef.
    pub syntactic_role_carrier: RoleCarrierSubjectV1,
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
    let location = locate(&cause, sources, paths);
    CanonicalSourceDiagnostic { cause, location }.into()
}

fn locate(
    error: &KernelCompileError,
    sources: &BTreeMap<String, CanonicalModuleSource>,
    paths: &BTreeMap<String, PathBuf>,
) -> Option<SourceLocationV1> {
    let KernelCompileError::RoleCarrier {
        module,
        schema_index,
        relation_index,
        role_index,
        cause,
    } = error
    else {
        return None;
    };
    let (target, carrier_kind) = match cause.as_ref() {
        KernelCompileError::UnknownObjectTarget { target, .. } => (target, CarrierKindV1::Object),
        KernelCompileError::UnknownRelationTarget { target, .. } => {
            (target, CarrierKindV1::RelationObject)
        }
        _ => return None,
    };
    let source = sources.get(module)?;
    let schema = source.parsed().schemas.get(*schema_index)?;
    let relation = schema.relations.get(*relation_index)?;
    let role = relation.fields.get(*role_index)?;
    let mut matches = source.source_map().role_carriers.iter().filter(|span| {
        span.schema_index == *schema_index
            && span.relation_index == *relation_index
            && span.role_index == *role_index
    });
    let span = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    let text = source.exact_text();
    if text.get(span.bytes.clone())? != target {
        return None;
    }
    let start = position(text, span.bytes.start)?;
    let end = position(text, span.bytes.end)?;
    let line_start = text[..span.bytes.start]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let line_end = text[span.bytes.start..]
        .find('\n')
        .map_or(text.len(), |index| span.bytes.start + index);
    let line = text[line_start..line_end].trim_end_matches('\r');
    let prefix_chars = text[line_start..span.bytes.start].chars().count();
    let skip = prefix_chars.saturating_sub(80);
    let excerpt_offset = line
        .char_indices()
        .nth(skip)
        .map_or(line.len(), |(index, _)| index);
    let excerpt = line[excerpt_offset..].chars().take(240).collect::<String>();
    let excerpt_truncated = excerpt_offset != 0 || excerpt.len() < line.len();
    let candidates = match carrier_kind {
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
    };
    Some(SourceLocationV1 {
        path: paths.get(module)?.to_str()?.to_string(),
        module_name: module.clone(),
        revision_digest: source.revision().as_str().to_string(),
        syntactic_role_carrier: RoleCarrierSubjectV1 {
            schema_index: *schema_index,
            relation_index: *relation_index,
            role_index: *role_index,
            schema: schema.name.clone(),
            relation: relation.name.clone(),
            role: role.field.clone(),
            carrier_kind,
        },
        byte_start: span.bytes.start,
        byte_end: span.bytes.end,
        start,
        end,
        excerpt,
        excerpt_byte_start: line_start + excerpt_offset,
        excerpt_truncated,
        suggested_name: suggested_name(target, &candidates),
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
    let subject = object(
        json!({"schema_index":count,"relation_index":count,"role_index":count,
        "schema":string,"relation":string,"role":string,"carrier_kind":{"enum":["object","relation_object"]}}),
    );
    object(
        json!({"path":string,"module_name":string,"revision_digest":string,"syntactic_role_carrier":subject,
        "byte_start":count,"byte_end":count,"start":position,"end":position,"excerpt":{"type":"string","maxLength":240},
        "excerpt_byte_start":count,"excerpt_truncated":{"type":"boolean"},"suggested_name":{"type":["string","null"]}}),
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
        if candidate.len() > 128 {
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
        assert_eq!(suggested_name("Compny", &vec!["Company"; 4097]), None);
        assert_eq!(suggested_name(&"x".repeat(129), &["Company"]), None);
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
            assert!(locate(&error, &sources, &paths).is_none());
        }
    }
}
