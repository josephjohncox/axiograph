//! SQL schema discovery for Axiograph
//!
//! Extracts ontology structure from SQL DDL:
//! - Tables -> objects
//! - Foreign keys -> relations
//! - Unique constraints -> key constraints
//! - Check constraints -> (heuristic mapping)

use anyhow::Result;
use sqlparser::ast::*;
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

/// Discovered SQL schema
#[derive(Debug, Clone, Default)]
pub struct SqlSchema {
    pub tables: Vec<TableDef>,
    pub foreign_keys: Vec<ForeignKey>,
    pub unique_keys: Vec<UniqueKey>,
}

#[derive(Debug, Clone)]
pub struct TableDef {
    pub name: String,
    pub columns: Vec<ColumnDef>,
    pub primary_key: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ColumnDef {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
}

#[derive(Debug, Clone)]
pub struct ForeignKey {
    pub from_table: String,
    pub from_columns: Vec<String>,
    pub to_table: String,
    pub to_columns: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct UniqueKey {
    pub table: String,
    pub columns: Vec<String>,
}

/// Parse SQL DDL and extract schema
pub fn parse_sql_ddl(sql: &str) -> Result<SqlSchema> {
    let dialect = GenericDialect {};
    let statements = Parser::parse_sql(&dialect, sql)?;

    let mut schema = SqlSchema::default();

    for stmt in statements {
        if let Statement::CreateTable {
            name,
            columns: sql_columns,
            constraints: sql_constraints,
            ..
        } = stmt
        {
            let table_name = name.to_string();
            let mut columns = Vec::new();
            let mut primary_key = Vec::new();

            for col in &sql_columns {
                columns.push(ColumnDef {
                    name: col.name.to_string(),
                    data_type: format!("{:?}", col.data_type),
                    nullable: !col
                        .options
                        .iter()
                        .any(|opt| matches!(opt.option, ColumnOption::NotNull)),
                });
            }

            // Extract constraints
            for constraint in &sql_constraints {
                match constraint {
                    TableConstraint::ForeignKey {
                        columns: fk_cols,
                        foreign_table,
                        referred_columns,
                        ..
                    } => {
                        schema.foreign_keys.push(ForeignKey {
                            from_table: table_name.clone(),
                            from_columns: fk_cols.iter().map(|c| c.to_string()).collect(),
                            to_table: foreign_table.to_string(),
                            to_columns: referred_columns.iter().map(|c| c.to_string()).collect(),
                        });
                    }
                    TableConstraint::Unique {
                        columns: uq_cols,
                        is_primary,
                        ..
                    } => {
                        if *is_primary {
                            primary_key = uq_cols.iter().map(|c| c.to_string()).collect();
                        } else {
                            schema.unique_keys.push(UniqueKey {
                                table: table_name.clone(),
                                columns: uq_cols.iter().map(|c| c.to_string()).collect(),
                            });
                        }
                    }
                    _ => {}
                }
            }

            schema.tables.push(TableDef {
                name: table_name,
                columns,
                primary_key,
            });
        }
    }

    Ok(schema)
}

// This adapter intentionally does *not* emit `.axi` directly. Ingestion
// produces untrusted `proposals.json` first; promotion into canonical `.axi`
// is explicit and reviewable.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_table_key_and_foreign_key_structure() -> Result<()> {
        let schema = parse_sql_ddl(
            "CREATE TABLE parent (id INTEGER, PRIMARY KEY (id));\n\
             CREATE TABLE child (\n\
               id INTEGER,\n\
               parent_id INTEGER NOT NULL,\n\
               PRIMARY KEY (id),\n\
               UNIQUE (parent_id),\n\
               FOREIGN KEY (parent_id) REFERENCES parent (id)\n\
             );",
        )?;

        assert_eq!(schema.tables.len(), 2);
        assert_eq!(schema.tables[1].name, "child");
        assert_eq!(schema.tables[1].primary_key, ["id"]);
        assert!(!schema.tables[1].columns[1].nullable);
        assert_eq!(schema.unique_keys.len(), 1);
        assert_eq!(schema.foreign_keys.len(), 1);
        assert_eq!(schema.foreign_keys[0].from_table, "child");
        assert_eq!(schema.foreign_keys[0].to_table, "parent");
        Ok(())
    }
}
