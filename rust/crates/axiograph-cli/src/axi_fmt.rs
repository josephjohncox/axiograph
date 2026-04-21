//! `.axi` formatting / fixing helpers.
//!
//! Philosophy
//! ----------
//! `.axi` is intended to be human-authored and well-commented. A formatter must
//! therefore avoid destructive rewrites (e.g. stripping comments or reordering
//! whole modules) unless explicitly requested.
//!
//! For the current release, we implement a *surgical* formatter:
//! - it canonicalizes theory `constraint ...` lines,
//! - it canonicalizes `subtype ...` lines,
//! - it canonicalizes `relation ...` declarations into explicit-role form,
//! - it preserves all other lines verbatim.
//!
//! This is primarily a "make canonical authoring mechanical" path now that:
//! - unknown constraints are fail-closed for certificates, and
//! - accepted-plane promotion rejects unknown constraints.

use std::path::Path;

use anyhow::{anyhow, Result};

fn count_leading_whitespace(s: &str) -> usize {
    s.chars().take_while(|c| c.is_whitespace()).count()
}

fn split_line_comment(line: &str) -> (&str, Option<&str>) {
    // `.axi` uses `--` comments (Idris/Lean style).
    if let Some(idx) = line.find("--") {
        (&line[..idx], Some(&line[idx..]))
    } else {
        (line, None)
    }
}

fn is_top_level_keyword(trimmed: &str) -> bool {
    trimmed.starts_with("schema ")
        || trimmed.starts_with("theory ")
        || trimmed.starts_with("instance ")
        || trimmed.starts_with("module ")
        || trimmed.starts_with("object ")
        || trimmed.starts_with("subtype ")
        || trimmed.starts_with("relation ")
        || trimmed.starts_with("constraint ")
        || trimmed.starts_with("equation ")
        || trimmed.starts_with("rewrite ")
}

fn fix_symmetric_where_shorthand(rest: &str) -> String {
    // Support a small, fixable shorthand:
    //   symmetric Rel where field in {A, B}
    // by rewriting it into the canonical:
    //   symmetric Rel where Rel.field in {A, B}
    //
    // This avoids `ConstraintV1::Unknown` for common, readable sources.
    let rest = rest.trim();
    let Some(after) = rest.strip_prefix("symmetric ").map(str::trim) else {
        return rest.to_string();
    };
    let Some((relation, guard)) = after.split_once(" where ") else {
        return rest.to_string();
    };
    let relation = relation.trim();
    let guard = guard.trim();
    let Some((lhs, rhs)) = guard.split_once(" in ") else {
        return rest.to_string();
    };
    let lhs = lhs.trim();
    let rhs = rhs.trim();
    if lhs.contains('.') || relation.is_empty() {
        return rest.to_string();
    }
    format!("symmetric {relation} where {relation}.{lhs} in {rhs}")
}

fn collect_relation_decl(
    lines: &[&str],
    start: usize,
) -> Result<(String, usize, bool), anyhow::Error> {
    let mut combined = String::new();
    let mut depth: i32 = 0;
    let mut i = start;
    let mut saw_inner_comment = false;

    while i < lines.len() {
        let line = lines[i];
        let (code, comment) = split_line_comment(line);
        if i > start && comment.is_some() {
            saw_inner_comment = true;
        }
        let code = code.trim();
        if code.is_empty() {
            i += 1;
            continue;
        }
        if combined.is_empty() && !code.starts_with("relation ") {
            return Err(anyhow!("expected relation declaration"));
        }
        if !combined.is_empty() {
            combined.push(' ');
        }
        combined.push_str(code);
        for ch in code.chars() {
            if ch == '(' {
                depth += 1;
            } else if ch == ')' {
                depth -= 1;
            }
        }
        i += 1;
        if depth <= 0 {
            break;
        }
    }

    if depth != 0 {
        return Err(anyhow!("unclosed relation declaration"));
    }

    Ok((combined, i, saw_inner_comment))
}

fn format_axi_surface(text: &str) -> Result<String> {
    let mut out: Vec<String> = Vec::new();
    let mut i: usize = 0;
    let lines: Vec<&str> = text.lines().collect();

    while i < lines.len() {
        let line = lines[i];
        let (code, comment) = split_line_comment(line);
        let code_trim_end = code.trim_end();
        let trimmed = code_trim_end.trim_start();

        if trimmed.starts_with("subtype ") {
            let indent_len = count_leading_whitespace(code_trim_end);
            let indent = &code_trim_end[..indent_len];
            let rest = trimmed
                .strip_prefix("subtype ")
                .ok_or_else(|| anyhow!("internal error: missing subtype prefix"))?
                .trim();
            let subtype = axiograph_dsl::schema_v1::parse_subtype_decl_v1(rest)
                .map_err(|e| anyhow!("failed to parse subtype declaration: {e}"))?;
            let formatted = axiograph_dsl::schema_v1::format_subtype_decl_v1(&subtype);
            let mut out_line = String::new();
            out_line.push_str(indent);
            out_line.push_str(&formatted);
            if let Some(c) = comment {
                out_line.push(' ');
                out_line.push_str(c.trim_end());
            }
            out.push(out_line);
            i += 1;
            continue;
        }

        if trimmed.starts_with("relation ") {
            let indent_len = count_leading_whitespace(code_trim_end);
            let indent = &code_trim_end[..indent_len];
            let (combined, next_i, saw_inner_comment) = collect_relation_decl(&lines, i)?;
            if saw_inner_comment {
                out.extend(lines[i..next_i].iter().map(|line| (*line).to_string()));
                i = next_i;
                continue;
            }
            let relation = axiograph_dsl::schema_v1::parse_relation_decl_v1(combined.trim())
                .map_err(|e| anyhow!("failed to parse relation declaration: {e}"))?;
            let formatted = axiograph_dsl::schema_v1::format_relation_decl_v1(&relation);
            let mut out_line = String::new();
            out_line.push_str(indent);
            out_line.push_str(&formatted);
            if let Some(c) = comment {
                out_line.push(' ');
                out_line.push_str(c.trim_end());
            }
            out.push(out_line);
            i = next_i;
            continue;
        }

        if trimmed.starts_with("constraint ") {
            let indent_len = count_leading_whitespace(code_trim_end);
            let indent = &code_trim_end[..indent_len];
            let rest = trimmed
                .strip_prefix("constraint ")
                .ok_or_else(|| anyhow!("internal error: missing constraint prefix"))?
                .trim();

            // Preserve named-block constraints verbatim (multi-line, author-written).
            if rest.ends_with(':') {
                out.push(line.to_string());
                i += 1;
                continue;
            }

            // Collect indented continuation lines (but do NOT consume blank lines).
            let mut extra_parts: Vec<String> = Vec::new();
            let mut j = i + 1;
            while j < lines.len() {
                let next_line = lines[j];
                let (next_code, _next_comment) = split_line_comment(next_line);
                let next_code_trim_end = next_code.trim_end();
                if next_code_trim_end.trim().is_empty() {
                    break;
                }
                let next_indent_len = count_leading_whitespace(next_code_trim_end);
                if next_indent_len <= indent_len {
                    break;
                }
                let next_trimmed = next_code_trim_end.trim();
                if is_top_level_keyword(next_trimmed) {
                    break;
                }
                extra_parts.push(next_trimmed.to_string());
                j += 1;
            }

            let combined_rest = if extra_parts.is_empty() {
                rest.to_string()
            } else {
                format!("{rest} {}", extra_parts.join(" "))
            };
            let combined_rest = fix_symmetric_where_shorthand(&combined_rest);

            let constraint = axiograph_dsl::schema_v1::parse_constraint_v1(&combined_rest)
                .map_err(|e| anyhow!("failed to parse constraint: {e}"))?;
            let formatted = axiograph_dsl::schema_v1::format_constraint_v1(&constraint)
                .map_err(|e| anyhow!("failed to format constraint: {e}"))?;

            let mut out_line = String::new();
            out_line.push_str(indent);
            out_line.push_str(formatted.trim_end());
            if let Some(c) = comment {
                out_line.push(' ');
                out_line.push_str(c.trim_end());
            }
            out.push(out_line);

            // Skip any continuation lines we folded in.
            i = j;
            continue;
        }

        out.push(line.to_string());
        i += 1;
    }

    let mut rendered = out.join("\n");
    if text.ends_with('\n') {
        rendered.push('\n');
    }
    Ok(rendered)
}

pub fn cmd_fmt_axi(input: &Path, out: Option<&Path>, write: bool) -> Result<()> {
    if write && out.is_some() {
        return Err(anyhow!("cannot use --write and --out together"));
    }
    let text = std::fs::read_to_string(input)?;
    let rendered = format_axi_surface(&text)?;
    if write {
        std::fs::write(input, rendered)?;
        println!("formatted {}", input.display());
        return Ok(());
    }
    if let Some(out) = out {
        std::fs::write(out, rendered)?;
        println!("wrote {}", out.display());
        return Ok(());
    }
    print!("{rendered}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::format_axi_surface;

    #[test]
    fn canonicalizes_relation_annotation_shorthand_into_explicit_roles() {
        let input = r#"module Family

schema Fam:
  relation Parent(child: Person, parent: Person) @context Context @temporal Time
"#;
        let rendered = format_axi_surface(input).expect("format axi");
        assert!(rendered
            .contains("relation Parent(child: Person, parent: Person, ctx: Context, time: Time)"));
    }

    #[test]
    fn canonicalizes_subtype_alias_to_canonical_surface() {
        let input = r#"module Demo

schema S:
  subtype Child <: Parent
"#;
        let rendered = format_axi_surface(input).expect("format axi");
        assert!(rendered.contains("subtype Child < Parent"));
        assert!(!rendered.contains("<:"));
    }
}
