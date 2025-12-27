//! SQL-ish query surface compiled into AxQL.
//!
//! We use `sqlparser` to avoid hand-rolling SQL parsing. The goal is not full SQL
//! compatibility; it is a *familiar surface* that maps into the same conjunctive
//! query semantics as AxQL.
//!
//! Supported (initial, intentionally small) subset:
//!
//! ```sql
//! SELECT x, y
//! FROM Node AS x, Node AS y
//! WHERE FOLLOW(x, 'rel_0/rel_1', y)
//!   AND ATTR(x, 'name') = 'node_42'
//!   AND HAS(x, 'rel_2', 'rel_3')
//! LIMIT 10;
//! ```
//!
//! - `FROM <Type> AS <var>` becomes `?var : Type`
//! - `FOLLOW(a, 'path', b)` becomes `a -path-> b`
//! - `HAS(x, 'rel_0', ...)` becomes `has(?x, rel_0, ...)`
//! - `ATTR(x, 'k') = 'v'` becomes `attr(?x, "k", "v")`

use crate::axql::{parse_axql_path_expr, AxqlAtom, AxqlPathExpr, AxqlQuery, AxqlTerm};
use anyhow::{anyhow, Result};
use sqlparser::ast::{
    BinaryOperator, Expr, Function, FunctionArg, FunctionArgExpr, Ident, Query, SelectItem,
    SetExpr, Statement, TableAlias, TableFactor,
};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

pub fn parse_sqlish_query(input: &str) -> Result<AxqlQuery> {
    let dialect = GenericDialect {};
    let mut statements =
        Parser::parse_sql(&dialect, input).map_err(|e| anyhow!("failed to parse SQL: {e}"))?;
    if statements.len() != 1 {
        return Err(anyhow!(
            "expected exactly one SQL statement, got {}",
            statements.len()
        ));
    }
    let stmt = statements.remove(0);

    let query = match stmt {
        Statement::Query(q) => q,
        other => return Err(anyhow!("unsupported SQL statement: {other:?}")),
    };

    lower_query(&query)
}

fn lower_query(query: &Box<Query>) -> Result<AxqlQuery> {
    let limit = query
        .limit
        .as_ref()
        .map(expr_as_u64)
        .transpose()?
        .map(|n| n as usize)
        .unwrap_or(20);

    let SetExpr::Select(select) = query.body.as_ref() else {
        return Err(anyhow!("only SELECT queries are supported"));
    };

    let select_vars = lower_select_items(&select.projection)?;
    let mut atoms = Vec::new();

    // FROM
    for table in &select.from {
        let relation = &table.relation;
        let (type_name, alias) = match relation {
            TableFactor::Table { name, alias, .. } => {
                let type_name = name.to_string();
                let alias = alias.as_ref().ok_or_else(|| {
                    anyhow!(
                        "FROM item `{type_name}` must have an alias (e.g. `FROM {type_name} AS x`)"
                    )
                })?;
                (type_name, alias)
            }
            other => return Err(anyhow!("unsupported FROM item: {other:?}")),
        };
        let var = axql_var_from_alias(alias)?;
        atoms.push(AxqlAtom::Type {
            term: AxqlTerm::Var(var),
            type_name,
        });
    }

    // WHERE
    if let Some(selection) = &select.selection {
        let mut where_atoms = Vec::new();
        collect_where_atoms(selection, &mut where_atoms)?;
        atoms.extend(where_atoms);
    }

    Ok(AxqlQuery {
        select_vars,
        disjuncts: vec![atoms],
        limit,
        contexts: Vec::new(),
        max_hops: None,
        min_confidence: None,
    })
}

fn lower_select_items(items: &[SelectItem]) -> Result<Vec<String>> {
    let mut vars = Vec::new();
    for item in items {
        match item {
            SelectItem::Wildcard(_) => {
                // Use AxQL default selection (all non-internal vars).
                return Ok(Vec::new());
            }
            SelectItem::UnnamedExpr(Expr::Identifier(id)) => vars.push(axql_var(id)?),
            SelectItem::ExprWithAlias { expr, alias } => match expr {
                Expr::Identifier(id) => {
                    // `SELECT x AS y` => select `?y` (alias), not the original binding name.
                    vars.push(axql_var(alias)?);
                    let _ = id; // keep for clarity; bindings are still by WHERE atoms.
                }
                other => return Err(anyhow!("unsupported SELECT expression: {other:?}")),
            },
            other => return Err(anyhow!("unsupported SELECT item: {other:?}")),
        }
    }
    Ok(vars)
}

fn axql_var(id: &Ident) -> Result<String> {
    let name = id.value.trim();
    if name.is_empty() {
        return Err(anyhow!("empty identifier in SQL query"));
    }
    Ok(format!("?{name}"))
}

fn axql_var_from_alias(alias: &TableAlias) -> Result<String> {
    axql_var(&alias.name)
}

fn collect_where_atoms(expr: &Expr, out: &mut Vec<AxqlAtom>) -> Result<()> {
    match expr {
        Expr::BinaryOp { left, op, right } => match op {
            BinaryOperator::And => {
                collect_where_atoms(left, out)?;
                collect_where_atoms(right, out)?;
                Ok(())
            }
            BinaryOperator::Eq => lower_eq_predicate(left, right, out),
            other => Err(anyhow!("unsupported WHERE operator `{other}`")),
        },
        Expr::Function(f) => lower_function_predicate(f, out),
        other => Err(anyhow!("unsupported WHERE predicate: {other:?}")),
    }
}

fn lower_eq_predicate(left: &Expr, right: &Expr, out: &mut Vec<AxqlAtom>) -> Result<()> {
    // ATTR(x, 'name') = 'node_42'
    if let Expr::Function(f) = left {
        if function_name(f).eq_ignore_ascii_case("attr") {
            let args = function_args(f)?;
            if args.len() != 2 {
                return Err(anyhow!("ATTR(x, key) expects 2 args"));
            }
            let term = term_from_expr(&args[0])?;
            let key = string_from_expr(&args[1])?;
            let value = string_from_expr(right)?;
            out.push(AxqlAtom::AttrEq { term, key, value });
            return Ok(());
        }
    }
    Err(anyhow!("unsupported WHERE equality predicate"))
}

fn lower_function_predicate(f: &Function, out: &mut Vec<AxqlAtom>) -> Result<()> {
    let name = function_name(f).to_ascii_lowercase();
    let args = function_args(f)?;

    match name.as_str() {
        "follow" | "edge" | "path" => {
            if args.len() != 3 {
                return Err(anyhow!(
                    "FOLLOW(source, path, target) expects 3 args, got {}",
                    args.len()
                ));
            }
            let left = term_from_expr(&args[0])?;
            let path = path_from_expr(&args[1])?;
            let right = term_from_expr(&args[2])?;
            out.push(AxqlAtom::Edge { left, path, right });
            Ok(())
        }
        "has" | "has_out" => {
            if args.len() < 2 {
                return Err(anyhow!("HAS(x, rel, ...) expects at least 2 args"));
            }
            let term = term_from_expr(&args[0])?;
            let mut rels = Vec::new();
            for a in &args[1..] {
                rels.push(string_from_expr(a)?);
            }
            out.push(AxqlAtom::HasOut { term, rels });
            Ok(())
        }
        other => Err(anyhow!("unsupported WHERE function `{other}`")),
    }
}

fn function_name(f: &Function) -> String {
    f.name.to_string()
}

fn function_args(f: &Function) -> Result<Vec<Expr>> {
    let mut out = Vec::new();
    for arg in &f.args {
        let FunctionArg::Unnamed(FunctionArgExpr::Expr(e)) = arg else {
            return Err(anyhow!("unsupported function arg: {arg:?}"));
        };
        out.push(e.clone());
    }
    Ok(out)
}

fn term_from_expr(e: &Expr) -> Result<AxqlTerm> {
    match e {
        Expr::Identifier(id) => Ok(AxqlTerm::Var(axql_var(id)?)),
        Expr::Value(v) => match v {
            sqlparser::ast::Value::Number(s, _) => Ok(AxqlTerm::Const(
                s.parse::<u32>()
                    .map_err(|e| anyhow!("invalid number `{s}`: {e}"))?,
            )),
            sqlparser::ast::Value::SingleQuotedString(s) => Ok(AxqlTerm::Lookup {
                key: "name".to_string(),
                value: s.clone(),
            }),
            other => Err(anyhow!("unsupported literal term: {other:?}")),
        },
        other => Err(anyhow!("unsupported term expression: {other:?}")),
    }
}

fn string_from_expr(e: &Expr) -> Result<String> {
    match e {
        Expr::Identifier(id) => Ok(id.value.clone()),
        Expr::Value(sqlparser::ast::Value::SingleQuotedString(s)) => Ok(s.clone()),
        Expr::Value(sqlparser::ast::Value::DoubleQuotedString(s)) => Ok(s.clone()),
        other => Err(anyhow!("expected string, got {other:?}")),
    }
}

fn path_from_expr(e: &Expr) -> Result<AxqlPathExpr> {
    match e {
        Expr::Identifier(id) => Ok(AxqlPathExpr::rel(id.value.clone())),
        Expr::Value(sqlparser::ast::Value::SingleQuotedString(s)) => parse_axql_path_expr(s),
        Expr::Value(sqlparser::ast::Value::DoubleQuotedString(s)) => parse_axql_path_expr(s),
        other => Err(anyhow!("expected path string, got {other:?}")),
    }
}

fn expr_as_u64(e: &Expr) -> Result<u64> {
    match e {
        Expr::Value(sqlparser::ast::Value::Number(s, _)) => s
            .parse::<u64>()
            .map_err(|e| anyhow!("invalid LIMIT number `{s}`: {e}")),
        other => Err(anyhow!("unsupported LIMIT expression: {other:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_db() -> axiograph_pathdb::PathDB {
        let mut db = axiograph_pathdb::PathDB::new();
        let a = db.add_entity("Node", vec![("name", "a")]);
        let b = db.add_entity("Node", vec![("name", "b")]);
        let c = db.add_entity("Node", vec![("name", "c")]);
        let _ = db.add_relation("rel_0", a, b, 0.9, Vec::new());
        let _ = db.add_relation("rel_1", b, c, 0.9, Vec::new());
        db.build_indexes();
        db
    }

    #[test]
    fn sqlish_follow_path_executes() -> Result<()> {
        let db = tiny_db();
        let q = parse_sqlish_query(
            "SELECT y FROM Node AS y WHERE FOLLOW(0, 'rel_0/rel_1', y) LIMIT 10;",
        )?;
        let res = crate::axql::execute_axql_query(&db, &q)?;
        assert_eq!(res.rows.len(), 1);
        assert_eq!(res.rows[0].get("?y").copied(), Some(2));
        Ok(())
    }

    #[test]
    fn sqlish_has_and_attr_executes() -> Result<()> {
        let db = tiny_db();
        let q = parse_sqlish_query(
            "SELECT x FROM Node AS x WHERE HAS(x, 'rel_0') AND ATTR(x, 'name') = 'a' LIMIT 5;",
        )?;
        let res = crate::axql::execute_axql_query(&db, &q)?;
        assert_eq!(res.rows.len(), 1);
        assert_eq!(res.rows[0].get("?x").copied(), Some(0));
        Ok(())
    }

    #[test]
    fn sqlish_string_term_is_name_lookup() -> Result<()> {
        let db = tiny_db();
        let q =
            parse_sqlish_query("SELECT x FROM Node AS x WHERE FOLLOW(x, 'rel_0', 'b') LIMIT 10;")?;
        let res = crate::axql::execute_axql_query(&db, &q)?;
        assert_eq!(res.rows.len(), 1);
        assert_eq!(res.rows[0].get("?x").copied(), Some(0));
        Ok(())
    }
}
