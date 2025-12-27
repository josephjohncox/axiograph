//! Natural-language-ish query templates for the REPL.
//!
//! This is intentionally **not** an LLM-driven parser: it is a small set of
//! deterministic templates that compile into AxQL.
//!
//! Why do this?
//! - It makes the REPL friendlier (“human-readable first”) without changing the
//!   **core semantics** (AxQL) or the certified-query pipeline.
//! - It provides a stable bridge to future NL layers (LLM-assisted parsing can
//!   simply emit AxQL).

use anyhow::{anyhow, Result};

use crate::axql::{parse_axql_path_expr, AxqlAtom, AxqlPathExpr, AxqlQuery, AxqlRegex, AxqlTerm};

pub fn parse_ask_query(tokens: &[String]) -> Result<AxqlQuery> {
    if tokens.is_empty() {
        return Err(anyhow!("usage: ask <query>"));
    }

    // Prefer "follow" templates if they match, because they're unambiguous and
    // map directly to a single AxQL path atom.
    if let Some(q) = try_parse_follow_query(tokens)? {
        return Ok(q);
    }

    if let Some(q) = try_parse_find_query(tokens)? {
        return Ok(q);
    }

    Err(anyhow!(
        "unsupported `ask` query; try e.g. `ask find Node named b` or `ask from 0 follow rel_0/rel_1`"
    ))
}

pub fn render_axql_query(query: &AxqlQuery) -> String {
    let mut out = String::new();
    out.push_str("select ");
    if query.select_vars.is_empty() {
        out.push_str("*");
    } else {
        out.push_str(&query.select_vars.join(" "));
    }
    out.push_str(" where ");
    let disjuncts = query
        .disjuncts
        .iter()
        .map(|atoms| atoms.iter().map(render_atom).collect::<Vec<_>>().join(", "))
        .collect::<Vec<_>>();
    out.push_str(&disjuncts.join(" or "));
    if !query.contexts.is_empty() {
        out.push_str(" in ");
        if query.contexts.len() == 1 {
            match &query.contexts[0] {
                crate::axql::AxqlContextSpec::EntityId(id) => out.push_str(&id.to_string()),
                crate::axql::AxqlContextSpec::Name(name) => out.push_str(name),
            }
        } else {
            out.push('{');
            out.push_str(
                &query
                    .contexts
                    .iter()
                    .map(|c| match c {
                        crate::axql::AxqlContextSpec::EntityId(id) => id.to_string(),
                        crate::axql::AxqlContextSpec::Name(name) => name.clone(),
                    })
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            out.push('}');
        }
    }
    if let Some(max_hops) = query.max_hops {
        out.push_str(&format!(" max_hops {max_hops}"));
    }
    if let Some(min_confidence) = query.min_confidence {
        out.push_str(&format!(" min_conf {min_confidence}"));
    }
    out.push_str(&format!(" limit {}", query.limit));
    out
}

fn try_parse_follow_query(tokens: &[String]) -> Result<Option<AxqlQuery>> {
    // Supported templates (case-insensitive):
    // - from <start_id> [follow] <path...> [max_hops N]
    // - follow <start_id> <path...> [max_hops N]
    // - follow from <start_id> <path...> [max_hops N]

    let lower: Vec<String> = tokens.iter().map(|s| s.to_ascii_lowercase()).collect();

    let (start_id, after_start) = if lower.get(0).is_some_and(|t| t == "from") {
        let Some(start) = tokens.get(1).and_then(|t| t.parse::<u32>().ok()) else {
            return Err(anyhow!("ask follow: expected `from <start_id>`"));
        };
        (start, 2usize)
    } else if lower.get(0).is_some_and(|t| t == "follow") {
        if lower.get(1).is_some_and(|t| t == "from") {
            let Some(start) = tokens.get(2).and_then(|t| t.parse::<u32>().ok()) else {
                return Err(anyhow!("ask follow: expected `follow from <start_id>`"));
            };
            (start, 3usize)
        } else {
            let Some(start) = tokens.get(1).and_then(|t| t.parse::<u32>().ok()) else {
                return Ok(None);
            };
            (start, 2usize)
        }
    } else {
        return Ok(None);
    };

    let mut i = after_start;
    if lower.get(i).is_some_and(|t| t == "follow") {
        i += 1;
    }

    if i >= tokens.len() {
        return Err(anyhow!(
            "ask follow: expected a path after the start id (e.g. `rel_0/rel_1`)"
        ));
    }

    let (path_tokens, max_hops) = split_off_max_hops(&tokens[i..])?;
    let expr_text = normalize_path_tokens(&path_tokens);
    let path: AxqlPathExpr = parse_axql_path_expr(&expr_text)?;

    Ok(Some(AxqlQuery {
        select_vars: vec!["?y".to_string()],
        disjuncts: vec![vec![AxqlAtom::Edge {
            left: AxqlTerm::Const(start_id),
            path,
            right: AxqlTerm::Var("?y".to_string()),
        }]],
        limit: 20,
        contexts: Vec::new(),
        max_hops,
        min_confidence: None,
    }))
}

fn try_parse_find_query(tokens: &[String]) -> Result<Option<AxqlQuery>> {
    // Supported templates (case-insensitive):
    // - find <Type> [named <name>] [has <rel>] [where|with <key> (=|is) <value>] [limit N]
    // - find things [named <name>] ...

    let lower: Vec<String> = tokens.iter().map(|s| s.to_ascii_lowercase()).collect();
    if !matches!(lower.get(0).map(|s| s.as_str()), Some("find" | "list")) {
        return Ok(None);
    }

    let (tokens, lower, limit) = split_off_limit(tokens, &lower)?;

    let mut i = 1usize;
    while i < lower.len() && matches!(lower[i].as_str(), "all" | "any" | "the") {
        i += 1;
    }
    if i >= lower.len() {
        return Err(anyhow!(
            "ask find: expected a type (e.g. `Node`) or `things`"
        ));
    }

    let mut type_name: Option<String> = None;
    if !matches!(
        lower[i].as_str(),
        "thing" | "things" | "entity" | "entities"
    ) {
        type_name = Some(canonicalize_type_name(&tokens[i]));
        i += 1;
    } else {
        i += 1;
    }

    let mut rels: Vec<String> = Vec::new();
    let mut attrs: Vec<(String, String)> = Vec::new();

    // Convenience sugar: "named X" ↦ name="X".
    while i < lower.len() {
        match lower[i].as_str() {
            "named" | "called" => {
                let (value, next_i) = take_value_phrase(&tokens, &lower, i + 1)?;
                attrs.push(("name".to_string(), value));
                i = next_i;
            }
            "has" => {
                let Some(rel) = tokens.get(i + 1) else {
                    return Err(anyhow!("ask find: expected a relation name after `has`"));
                };
                rels.push(rel.to_string());
                i += 2;
            }
            "with" | "where" | "and" => {
                i += 1;
            }
            _ => {
                // Allow inline `key=value` tokens.
                if let Some((k, v)) = split_kv_inline(&tokens[i]) {
                    attrs.push((k, v));
                    i += 1;
                    continue;
                }

                // Allow `key (=|is) value` patterns.
                if i + 2 < tokens.len()
                    && (lower[i + 1] == "=" || lower[i + 1] == "is")
                    && !is_stop_keyword(&lower[i + 2])
                {
                    let key = tokens[i].clone();
                    let (value, next_i) = take_value_phrase(&tokens, &lower, i + 2)?;
                    attrs.push((key, value));
                    i = next_i;
                    continue;
                }

                i += 1;
            }
        }
    }

    let subject = AxqlTerm::Var("?x".to_string());
    let mut atoms: Vec<AxqlAtom> = Vec::new();
    if let Some(type_name) = type_name {
        atoms.push(AxqlAtom::Type {
            term: subject.clone(),
            type_name,
        });
    }
    for rel in rels {
        atoms.push(AxqlAtom::HasOut {
            term: subject.clone(),
            rels: vec![rel],
        });
    }
    for (key, value) in attrs {
        atoms.push(AxqlAtom::AttrEq {
            term: subject.clone(),
            key,
            value,
        });
    }

    Ok(Some(AxqlQuery {
        select_vars: vec!["?x".to_string()],
        disjuncts: vec![atoms],
        limit,
        contexts: Vec::new(),
        max_hops: None,
        min_confidence: None,
    }))
}

fn split_off_limit(
    tokens: &[String],
    lower: &[String],
) -> Result<(Vec<String>, Vec<String>, usize)> {
    // Recognize `limit N` at the end.
    if lower.len() >= 2 && lower[lower.len() - 2] == "limit" {
        let n = lower[lower.len() - 1]
            .parse::<usize>()
            .map_err(|_| anyhow!("ask: invalid limit `{}`", tokens[tokens.len() - 1]))?;
        let trimmed = tokens[..tokens.len() - 2].to_vec();
        let trimmed_lower = lower[..lower.len() - 2].to_vec();
        return Ok((trimmed, trimmed_lower, n));
    }
    Ok((tokens.to_vec(), lower.to_vec(), 20))
}

fn split_off_max_hops(tokens: &[String]) -> Result<(Vec<String>, Option<u32>)> {
    // Recognize:
    // - ... max_hops N
    // - ... max hops N
    // - ... within N hops
    let lower: Vec<String> = tokens.iter().map(|s| s.to_ascii_lowercase()).collect();
    if lower.len() >= 2 && lower[lower.len() - 2] == "max_hops" {
        let n = lower[lower.len() - 1].parse::<u32>().map_err(|_| {
            anyhow!(
                "ask follow: invalid max_hops `{}`",
                tokens[tokens.len() - 1]
            )
        })?;
        return Ok((tokens[..tokens.len() - 2].to_vec(), Some(n)));
    }
    if lower.len() >= 3 && lower[lower.len() - 3] == "max" && lower[lower.len() - 2] == "hops" {
        let n = lower[lower.len() - 1].parse::<u32>().map_err(|_| {
            anyhow!(
                "ask follow: invalid max hops `{}`",
                tokens[tokens.len() - 1]
            )
        })?;
        return Ok((tokens[..tokens.len() - 3].to_vec(), Some(n)));
    }
    if lower.len() >= 3 && lower[lower.len() - 3] == "within" && lower[lower.len() - 1] == "hops" {
        let n = lower[lower.len() - 2].parse::<u32>().map_err(|_| {
            anyhow!(
                "ask follow: invalid hops bound `{}`",
                tokens[tokens.len() - 2]
            )
        })?;
        return Ok((tokens[..tokens.len() - 3].to_vec(), Some(n)));
    }
    Ok((tokens.to_vec(), None))
}

fn normalize_path_tokens(tokens: &[String]) -> String {
    // Drop natural-language separators and keep the rest.
    //
    // We first try to parse the space-joined text as an RPQ (it supports
    // whitespace around `/` and other operators). If that fails, we interpret
    // the remaining tokens as a simple relation chain.
    let mut kept: Vec<String> = Vec::new();
    for t in tokens {
        let lower = t.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "then" | "next" | "and" | "to" | "," | "->" | "→"
        ) {
            continue;
        }
        kept.push(t.to_string());
    }

    if kept.is_empty() {
        return String::new();
    }

    let joined = kept.join(" ");
    if parse_axql_path_expr(&joined).is_ok() {
        return joined;
    }

    // Fallback: treat as a plain chain.
    kept.join("/")
}

fn canonicalize_type_name(token: &str) -> String {
    // Gentle heuristics so users can say `nodes` and get `Node`.
    let lower = token.to_ascii_lowercase();
    match lower.as_str() {
        "node" | "nodes" => "Node".to_string(),
        _ => {
            // If the user wrote a lowercase plural like `widgets`, drop the
            // trailing `s`. Don't do this for mixed-case type names.
            if token.chars().all(|c| c.is_ascii_lowercase())
                && lower.ends_with('s')
                && lower.len() > 2
            {
                lower.trim_end_matches('s').to_string()
            } else {
                token.to_string()
            }
        }
    }
}

fn is_stop_keyword(token_lower: &str) -> bool {
    matches!(
        token_lower,
        "named"
            | "called"
            | "has"
            | "with"
            | "where"
            | "and"
            | "limit"
            | "max_hops"
            | "max"
            | "hops"
            | "within"
    )
}

fn take_value_phrase(tokens: &[String], lower: &[String], start: usize) -> Result<(String, usize)> {
    if start >= tokens.len() {
        return Err(anyhow!("ask: expected a value"));
    }

    let mut parts: Vec<String> = Vec::new();
    let mut i = start;
    while i < tokens.len() && !is_stop_keyword(lower[i].as_str()) {
        parts.push(tokens[i].to_string());
        i += 1;
    }
    if parts.is_empty() {
        return Err(anyhow!("ask: expected a value"));
    }
    Ok((parts.join(" "), i))
}

fn split_kv_inline(token: &str) -> Option<(String, String)> {
    // Accept `key=value` when typed as a single token.
    let (k, v) = token.split_once('=')?;
    if k.is_empty() || v.is_empty() {
        return None;
    }
    Some((k.to_string(), v.to_string()))
}

fn render_atom(atom: &AxqlAtom) -> String {
    match atom {
        AxqlAtom::Type { term, type_name } => format!("{} : {}", render_term(term), type_name),
        AxqlAtom::Edge { left, path, right } => format!(
            "{} -{}-> {}",
            render_term(left),
            render_path_expr(path),
            render_term(right)
        ),
        AxqlAtom::AttrEq { term, key, value } => {
            format!("{}.{} = {}", render_term(term), key, render_string(value))
        }
        AxqlAtom::AttrContains { term, key, needle } => format!(
            "contains({}, {}, {})",
            render_term(term),
            render_string(key),
            render_string(needle)
        ),
        AxqlAtom::AttrFts { term, key, query } => format!(
            "fts({}, {}, {})",
            render_term(term),
            render_string(key),
            render_string(query)
        ),
        AxqlAtom::AttrFuzzy {
            term,
            key,
            needle,
            max_dist,
        } => format!(
            "fuzzy({}, {}, {}, {max_dist})",
            render_term(term),
            render_string(key),
            render_string(needle)
        ),
        AxqlAtom::Fact {
            fact,
            relation,
            fields,
        } => {
            let head = match fact {
                Some(f) => format!("{} = {relation}", render_term(f)),
                None => relation.clone(),
            };
            let args = fields
                .iter()
                .map(|(k, v)| format!("{k}={}", render_term(v)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{head}({args})")
        }
        AxqlAtom::HasOut { term, rels } => {
            if rels.len() == 1 {
                format!("{} has {}", render_term(term), rels[0])
            } else {
                format!("has({}, {})", render_term(term), rels.join(", "))
            }
        }
        AxqlAtom::Attrs { term, pairs } => {
            let pairs = pairs
                .iter()
                .map(|(k, v)| format!("{k}={}", render_string(v)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("attrs({}, {pairs})", render_term(term))
        }
        AxqlAtom::Shape {
            term,
            type_name,
            rels,
            attrs,
        } => {
            let mut items: Vec<String> = Vec::new();
            if let Some(t) = type_name {
                items.push(format!("is {t}"));
            }
            items.extend(rels.iter().cloned());
            items.extend(
                attrs
                    .iter()
                    .map(|(k, v)| format!("{k}={}", render_string(v))),
            );
            format!("{} {{ {} }}", render_term(term), items.join(", "))
        }
    }
}

fn render_term(term: &AxqlTerm) -> String {
    match term {
        AxqlTerm::Var(v) => v.clone(),
        AxqlTerm::Const(n) => n.to_string(),
        AxqlTerm::Wildcard => "_".to_string(),
        AxqlTerm::Lookup { key, value } => {
            if key == "name" {
                format!("name({})", render_string(value))
            } else {
                format!("entity({}, {})", render_string(key), render_string(value))
            }
        }
    }
}

fn render_path_expr(expr: &AxqlPathExpr) -> String {
    render_regex(&expr.regex)
}

fn render_regex(regex: &AxqlRegex) -> String {
    match regex {
        AxqlRegex::Epsilon => "ε".to_string(),
        AxqlRegex::Rel(r) => r.clone(),
        AxqlRegex::Seq(parts) => parts.iter().map(render_regex).collect::<Vec<_>>().join("/"),
        AxqlRegex::Alt(parts) => {
            let inner = parts.iter().map(render_regex).collect::<Vec<_>>().join("|");
            format!("({inner})")
        }
        AxqlRegex::Star(inner) => format!("{}*", render_regex(inner)),
        AxqlRegex::Plus(inner) => format!("{}+", render_regex(inner)),
        AxqlRegex::Opt(inner) => format!("{}?", render_regex(inner)),
    }
}

fn render_string(s: &str) -> String {
    let escaped = s
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
        .replace('\r', "\\r");
    format!("\"{escaped}\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(s: &str) -> Vec<String> {
        s.split_whitespace().map(|t| t.to_string()).collect()
    }

    #[test]
    fn ask_follow_from_parses() -> Result<()> {
        let q = parse_ask_query(&toks("from 0 follow rel_0 rel_1 max hops 5"))?;
        assert_eq!(q.select_vars, vec!["?y"]);
        assert_eq!(q.max_hops, Some(5));
        assert_eq!(q.disjuncts.len(), 1);
        assert_eq!(q.disjuncts[0].len(), 1);
        Ok(())
    }

    #[test]
    fn ask_find_named_parses() -> Result<()> {
        let q = parse_ask_query(&toks("find Node named b limit 3"))?;
        assert_eq!(q.limit, 3);
        assert_eq!(q.select_vars, vec!["?x"]);
        assert_eq!(q.disjuncts.len(), 1);
        let atoms = &q.disjuncts[0];
        assert!(q
            .disjuncts
            .iter()
            .flatten()
            .any(|a| matches!(a, AxqlAtom::Type { type_name, .. } if type_name == "Node")));
        assert!(atoms.iter().any(
            |a| matches!(a, AxqlAtom::AttrEq { key, value, .. } if key == "name" && value == "b")
        ));
        Ok(())
    }
}
