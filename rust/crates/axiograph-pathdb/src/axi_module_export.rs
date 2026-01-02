//! Export canonical `axi_schema_v1` modules from PathDB.
//!
//! This is distinct from `axi_export`:
//! - `axi_export` round-trips PathDB *snapshots* via `PathDBExportV1`
//! - this module round-trips *canonical schema modules* (`schema/theory/instance`)
//!   when PathDB contains the meta-plane produced by `axi_module_import`

use anyhow::{anyhow, Result};

use crate::axi_meta::*;
use crate::PathDB;

pub fn export_axi_schema_v1_module_from_pathdb(db: &PathDB, module_name: &str) -> Result<String> {
    let module_entity = find_entity_by_type_and_attr(
        db,
        META_TYPE_MODULE,
        META_ATTR_ID,
        &meta_id_module(module_name),
    )
    .ok_or_else(|| anyhow!("no meta module `{module_name}` found in PathDB (import a canonical `.axi` module first)"))?;

    let schema_entities = follow_ids(db, module_entity, META_REL_HAS_SCHEMA);
    if schema_entities.is_empty() {
        return Err(anyhow!(
            "meta module `{module_name}` has no schemas (unexpected)"
        ));
    }

    // Index schema metadata first (names + ordering).
    let mut schemas: Vec<(String, u32)> = Vec::new();
    for id in schema_entities {
        let name = entity_attr(db, id, META_ATTR_NAME)
            .ok_or_else(|| anyhow!("meta schema entity {id} missing `name`"))?;
        schemas.push((name, id));
    }
    schemas.sort_by(|a, b| a.0.cmp(&b.0));

    let mut out = String::new();
    out.push_str(&format!("module {module_name}\n\n"));

    // ---------------------------------------------------------------------
    // Schemas
    // ---------------------------------------------------------------------
    for (schema_name, schema_entity) in &schemas {
        out.push_str(&format!("schema {schema_name}:\n"));

        // Objects.
        let mut object_names: Vec<String> =
            follow_names(db, *schema_entity, META_REL_SCHEMA_HAS_OBJECT);
        object_names.sort();
        for obj in &object_names {
            out.push_str(&format!("  object {obj}\n"));
        }

        // Subtypes.
        let subtype_ids = follow_ids(db, *schema_entity, META_REL_SCHEMA_HAS_SUBTYPE);
        let mut subtype_lines: Vec<String> = Vec::new();
        for sid in subtype_ids {
            let sub = entity_attr(db, sid, ATTR_SUBTYPE_SUB).unwrap_or_default();
            let sup = entity_attr(db, sid, ATTR_SUBTYPE_SUP).unwrap_or_default();
            if sub.is_empty() || sup.is_empty() {
                continue;
            }
            let incl = entity_attr(db, sid, ATTR_SUBTYPE_INCLUSION).unwrap_or_default();
            if incl.trim().is_empty() {
                subtype_lines.push(format!("  subtype {sub} < {sup}\n"));
            } else {
                subtype_lines.push(format!("  subtype {sub} < {sup} as {incl}\n"));
            }
        }
        subtype_lines.sort();
        for line in subtype_lines {
            out.push_str(&line);
        }

        // Relations.
        let rel_ids = follow_ids(db, *schema_entity, META_REL_SCHEMA_HAS_RELATION);
        let mut relations: Vec<(String, u32)> = Vec::new();
        for rid in rel_ids {
            let rname = entity_attr(db, rid, META_ATTR_NAME)
                .ok_or_else(|| anyhow!("meta relation decl {rid} missing `name`"))?;
            relations.push((rname, rid));
        }
        relations.sort_by(|a, b| a.0.cmp(&b.0));

        for (rname, rid) in &relations {
            let fields = relation_fields(db, *rid)?;
            out.push_str(&format_relation_decl(rname, &fields));
        }

        out.push('\n');
    }

    // ---------------------------------------------------------------------
    // Theories (attached to schemas)
    // ---------------------------------------------------------------------
    for (schema_name, schema_entity) in &schemas {
        let theory_ids = follow_ids(db, *schema_entity, META_REL_SCHEMA_HAS_THEORY);
        if theory_ids.is_empty() {
            continue;
        }

        let mut theories: Vec<(String, u32)> = Vec::new();
        for tid in theory_ids {
            let tname = entity_attr(db, tid, META_ATTR_NAME)
                .ok_or_else(|| anyhow!("meta theory {tid} missing `name`"))?;
            theories.push((tname, tid));
        }
        theories.sort_by(|a, b| a.0.cmp(&b.0));

        for (theory_name, theory_id) in theories {
            out.push_str(&format!("theory {theory_name} on {schema_name}:\n"));

            // Constraints (stable by `axi_constraint_index`).
            let constraint_ids = follow_ids(db, theory_id, META_REL_THEORY_HAS_CONSTRAINT);
            let mut constraints: Vec<(usize, u32)> = Vec::new();
            for cid in constraint_ids {
                let idx = entity_attr(db, cid, ATTR_CONSTRAINT_INDEX)
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(usize::MAX);
                constraints.push((idx, cid));
            }
            constraints.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

            for (_idx, cid) in constraints {
                out.push_str("  ");
                out.push_str(&format_constraint(db, cid)?);
                out.push('\n');
            }

            // Equations (stable by `axi_equation_index`).
            let equation_ids = follow_ids(db, theory_id, META_REL_THEORY_HAS_EQUATION);
            let mut equations: Vec<(usize, u32)> = Vec::new();
            for eid in equation_ids {
                let idx = entity_attr(db, eid, ATTR_EQUATION_INDEX)
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(usize::MAX);
                equations.push((idx, eid));
            }
            equations.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

            for (_idx, eid) in equations {
                let ename =
                    entity_attr(db, eid, META_ATTR_NAME).unwrap_or_else(|| "equation".to_string());
                let lhs = entity_attr(db, eid, ATTR_EQUATION_LHS).unwrap_or_default();
                let rhs = entity_attr(db, eid, ATTR_EQUATION_RHS).unwrap_or_default();
                out.push_str(&format!("  equation {ename}:\n"));
                out.push_str(&format!("    {lhs} =\n"));
                out.push_str(&format!("    {rhs}\n"));
            }

            // Rewrite rules (stable by `axi_rewrite_rule_index`).
            let rule_ids = follow_ids(db, theory_id, META_REL_THEORY_HAS_REWRITE_RULE);
            let mut rules: Vec<(usize, u32)> = Vec::new();
            for rid in rule_ids {
                let idx = entity_attr(db, rid, ATTR_REWRITE_RULE_INDEX)
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(usize::MAX);
                rules.push((idx, rid));
            }
            rules.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

            for (_idx, rid) in rules {
                let rname =
                    entity_attr(db, rid, META_ATTR_NAME).unwrap_or_else(|| "rewrite".to_string());
                let orientation = entity_attr(db, rid, ATTR_REWRITE_RULE_ORIENTATION)
                    .unwrap_or_else(|| "forward".to_string());
                let vars = entity_attr(db, rid, ATTR_REWRITE_RULE_VARS).unwrap_or_default();
                let lhs = entity_attr(db, rid, ATTR_REWRITE_RULE_LHS).unwrap_or_default();
                let rhs = entity_attr(db, rid, ATTR_REWRITE_RULE_RHS).unwrap_or_default();

                out.push_str(&format!("  rewrite {rname}:\n"));
                if orientation != "forward" {
                    out.push_str(&format!("    orientation: {orientation}\n"));
                }
                if !vars.trim().is_empty() {
                    out.push_str(&format!("    vars: {vars}\n"));
                }
                out.push_str(&format!("    lhs: {lhs}\n"));
                out.push_str(&format!("    rhs: {rhs}\n"));
            }

            out.push('\n');
        }
    }

    // ---------------------------------------------------------------------
    // Instances
    // ---------------------------------------------------------------------
    let instance_ids = follow_ids(db, module_entity, META_REL_HAS_INSTANCE);
    let mut instances: Vec<(String, String, u32)> = Vec::new(); // (instance_name, schema_name, id)
    for iid in instance_ids {
        let iname = entity_attr(db, iid, META_ATTR_NAME)
            .ok_or_else(|| anyhow!("meta instance {iid} missing `name`"))?;
        let schema = entity_attr(db, iid, ATTR_INSTANCE_SCHEMA)
            .ok_or_else(|| anyhow!("meta instance {iid} missing `{ATTR_INSTANCE_SCHEMA}`"))?;
        instances.push((iname, schema, iid));
    }
    instances.sort_by(|a, b| a.0.cmp(&b.0));

    for (instance_name, schema_name, _iid) in instances {
        // Find schema meta entity by name.
        let schema_entity = schemas
            .iter()
            .find(|(n, _)| n == &schema_name)
            .map(|(_, id)| *id)
            .ok_or_else(|| {
                anyhow!("instance `{instance_name}` references missing schema `{schema_name}`")
            })?;

        out.push_str(&format!("instance {instance_name} of {schema_name}:\n"));

        // Object assignments (iterate over declared object types).
        let mut object_names: Vec<String> =
            follow_names(db, schema_entity, META_REL_SCHEMA_HAS_OBJECT);
        object_names.sort();
        for obj in &object_names {
            let ids = instance_entities_of_type(db, module_name, &schema_name, &instance_name, obj);
            let mut names: Vec<String> = ids
                .into_iter()
                .filter_map(|id| entity_attr(db, id, META_ATTR_NAME))
                .collect();
            names.sort();
            out.push_str(&format!("  {obj} = {}\n", format_ident_set(&names)));
        }

        out.push('\n');

        // Relation assignments (iterate over declared relations).
        let rel_ids = follow_ids(db, schema_entity, META_REL_SCHEMA_HAS_RELATION);
        let mut relations: Vec<(String, u32)> = Vec::new();
        for rid in rel_ids {
            let rname = entity_attr(db, rid, META_ATTR_NAME)
                .ok_or_else(|| anyhow!("meta relation decl {rid} missing `name`"))?;
            relations.push((rname, rid));
        }
        relations.sort_by(|a, b| a.0.cmp(&b.0));

        for (relation_name, relation_decl_id) in relations {
            let fields = relation_fields(db, relation_decl_id)?;
            let tuple_ids = instance_tuples_for_relation(
                db,
                module_name,
                &schema_name,
                &instance_name,
                &relation_name,
            );

            // Stable by fact id when available.
            let mut tuples: Vec<(String, u32)> = Vec::new();
            for tid in tuple_ids {
                let fid =
                    entity_attr(db, tid, ATTR_AXI_FACT_ID).unwrap_or_else(|| format!("id:{tid}"));
                tuples.push((fid, tid));
            }
            tuples.sort_by(|a, b| a.0.cmp(&b.0));

            let mut rendered: Vec<String> = Vec::new();
            for (_fid, tid) in tuples {
                rendered.push(render_tuple_instance(db, tid, &fields)?);
            }

            out.push_str(&format!(
                "  {relation_name} = {}\n",
                format_tuple_set(&rendered)
            ));
        }

        out.push('\n');
    }

    Ok(out)
}

// =============================================================================
// Helpers
// =============================================================================

fn entity_attr(db: &PathDB, entity_id: u32, key: &str) -> Option<String> {
    db.get_entity(entity_id)?.attrs.get(key).cloned()
}

fn follow_ids(db: &PathDB, source: u32, rel: &str) -> Vec<u32> {
    db.follow_one(source, rel).iter().collect()
}

fn follow_names(db: &PathDB, source: u32, rel: &str) -> Vec<String> {
    follow_ids(db, source, rel)
        .into_iter()
        .filter_map(|id| entity_attr(db, id, META_ATTR_NAME))
        .collect()
}

fn find_entity_by_type_and_attr(
    db: &PathDB,
    type_name: &str,
    key: &str,
    value: &str,
) -> Option<u32> {
    let type_id = db.interner.id_of(type_name)?;
    let key_id = db.interner.id_of(key)?;
    let value_id = db.interner.id_of(value)?;

    let mut candidates = db.entities.entities_with_attr_value(key_id, value_id);
    let type_bitmap = db.entities.by_type(type_id)?;
    candidates &= type_bitmap.clone();
    candidates.iter().next()
}

fn relation_fields(db: &PathDB, relation_decl_id: u32) -> Result<Vec<(String, String)>> {
    let field_ids = follow_ids(db, relation_decl_id, META_REL_RELATION_HAS_FIELD);
    let mut fields: Vec<(usize, String, String)> = Vec::new(); // (idx, name, ty)
    for fid in field_ids {
        let field = entity_attr(db, fid, ATTR_FIELD_NAME).unwrap_or_default();
        let ty = entity_attr(db, fid, ATTR_FIELD_TYPE).unwrap_or_default();
        let idx = entity_attr(db, fid, ATTR_FIELD_INDEX)
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(usize::MAX);
        fields.push((idx, field, ty));
    }
    fields.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    Ok(fields.into_iter().map(|(_, f, t)| (f, t)).collect())
}

fn format_relation_decl(relation_name: &str, fields: &[(String, String)]) -> String {
    let mut flat = String::new();
    flat.push_str("  relation ");
    flat.push_str(relation_name);
    flat.push('(');
    for (i, (field, ty)) in fields.iter().enumerate() {
        if i > 0 {
            flat.push_str(", ");
        }
        flat.push_str(field);
        flat.push_str(": ");
        flat.push_str(ty);
    }
    flat.push_str(")\n");

    if flat.len() <= 100 && fields.len() <= 3 {
        return flat;
    }

    let mut out = String::new();
    out.push_str(&format!("  relation {relation_name}(\n"));
    for (i, (field, ty)) in fields.iter().enumerate() {
        let comma = if i + 1 == fields.len() { "" } else { "," };
        out.push_str(&format!("    {field}: {ty}{comma}\n"));
    }
    out.push_str("  )\n");
    out
}

fn format_constraint(db: &PathDB, constraint_id: u32) -> Result<String> {
    let kind = entity_attr(db, constraint_id, ATTR_CONSTRAINT_KIND)
        .ok_or_else(|| anyhow!("constraint {constraint_id} missing `{ATTR_CONSTRAINT_KIND}`"))?;

    Ok(match kind.as_str() {
        "functional" => {
            let rel = entity_attr(db, constraint_id, ATTR_CONSTRAINT_RELATION).unwrap_or_default();
            let src = entity_attr(db, constraint_id, ATTR_CONSTRAINT_SRC_FIELD).unwrap_or_default();
            let dst = entity_attr(db, constraint_id, ATTR_CONSTRAINT_DST_FIELD).unwrap_or_default();
            format!("constraint functional {rel}.{src} -> {rel}.{dst}")
        }
        "typing" => {
            let rel = entity_attr(db, constraint_id, ATTR_CONSTRAINT_RELATION).unwrap_or_default();
            let rule = entity_attr(db, constraint_id, ATTR_CONSTRAINT_TEXT).unwrap_or_default();
            format!("constraint typing {rel}: {rule}")
        }
        "symmetric" => {
            let rel = entity_attr(db, constraint_id, ATTR_CONSTRAINT_RELATION).unwrap_or_default();
            let mut out = format!("constraint symmetric {rel}");
            if let (Some(left), Some(right)) = (
                entity_attr(db, constraint_id, ATTR_CONSTRAINT_SRC_FIELD),
                entity_attr(db, constraint_id, ATTR_CONSTRAINT_DST_FIELD),
            ) {
                if !left.trim().is_empty() && !right.trim().is_empty() {
                    out.push_str(&format!(" on ({left}, {right})"));
                }
            }
            if let Some(params) = entity_attr(db, constraint_id, ATTR_CONSTRAINT_PARAM_FIELDS) {
                let params = params
                    .split(',')
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(", ");
                if !params.is_empty() {
                    out.push_str(&format!(" param ({params})"));
                }
            }
            out
        }
        "symmetric_where_in" => {
            let rel = entity_attr(db, constraint_id, ATTR_CONSTRAINT_RELATION).unwrap_or_default();
            let field =
                entity_attr(db, constraint_id, ATTR_CONSTRAINT_WHERE_FIELD).unwrap_or_default();
            let values =
                entity_attr(db, constraint_id, ATTR_CONSTRAINT_WHERE_IN_VALUES).unwrap_or_default();
            let values = values
                .split(',')
                .filter(|s| !s.trim().is_empty())
                .collect::<Vec<_>>()
                .join(", ");
            let mut out = format!("constraint symmetric {rel} where {rel}.{field} in {{{values}}}");
            if let (Some(left), Some(right)) = (
                entity_attr(db, constraint_id, ATTR_CONSTRAINT_SRC_FIELD),
                entity_attr(db, constraint_id, ATTR_CONSTRAINT_DST_FIELD),
            ) {
                if !left.trim().is_empty() && !right.trim().is_empty() {
                    out.push_str(&format!(" on ({left}, {right})"));
                }
            }
            if let Some(params) = entity_attr(db, constraint_id, ATTR_CONSTRAINT_PARAM_FIELDS) {
                let params = params
                    .split(',')
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(", ");
                if !params.is_empty() {
                    out.push_str(&format!(" param ({params})"));
                }
            }
            out
        }
        "transitive" => {
            let rel = entity_attr(db, constraint_id, ATTR_CONSTRAINT_RELATION).unwrap_or_default();
            let mut out = format!("constraint transitive {rel}");
            if let (Some(left), Some(right)) = (
                entity_attr(db, constraint_id, ATTR_CONSTRAINT_SRC_FIELD),
                entity_attr(db, constraint_id, ATTR_CONSTRAINT_DST_FIELD),
            ) {
                if !left.trim().is_empty() && !right.trim().is_empty() {
                    out.push_str(&format!(" on ({left}, {right})"));
                }
            }
            if let Some(params) = entity_attr(db, constraint_id, ATTR_CONSTRAINT_PARAM_FIELDS) {
                let params = params
                    .split(',')
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(", ");
                if !params.is_empty() {
                    out.push_str(&format!(" param ({params})"));
                }
            }
            out
        }
        "key" => {
            let rel = entity_attr(db, constraint_id, ATTR_CONSTRAINT_RELATION).unwrap_or_default();
            let fields = entity_attr(db, constraint_id, ATTR_CONSTRAINT_FIELDS).unwrap_or_default();
            let fields = fields
                .split(',')
                .filter(|s| !s.trim().is_empty())
                .collect::<Vec<_>>()
                .join(", ");
            format!("constraint key {rel}({fields})")
        }
        "named_block" => {
            let name = entity_attr(db, constraint_id, ATTR_CONSTRAINT_NAME)
                .unwrap_or_else(|| "Constraint".to_string());
            let body = entity_attr(db, constraint_id, ATTR_CONSTRAINT_TEXT).unwrap_or_default();
            let body = body.trim();
            if body.is_empty() {
                format!("constraint {name}:")
            } else {
                let mut out = format!("constraint {name}:");
                for line in body.lines() {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }
                    out.push('\n');
                    out.push_str("    ");
                    out.push_str(line);
                }
                out
            }
        }
        "unknown" => {
            let text = entity_attr(db, constraint_id, ATTR_CONSTRAINT_TEXT).unwrap_or_default();
            format!("constraint {text}")
        }
        other => format!("constraint unknown {other}"),
    })
}

fn instance_entities_of_type(
    db: &PathDB,
    module_name: &str,
    schema_name: &str,
    instance_name: &str,
    type_name: &str,
) -> Vec<u32> {
    let Some(type_bitmap) = db.find_by_type(type_name) else {
        return Vec::new();
    };

    let mut candidates = type_bitmap.clone();
    for (k, v) in [
        (ATTR_AXI_MODULE, module_name),
        (ATTR_AXI_SCHEMA, schema_name),
        (ATTR_AXI_INSTANCE, instance_name),
    ] {
        let Some(kid) = db.interner.id_of(k) else {
            return Vec::new();
        };
        let Some(vid) = db.interner.id_of(v) else {
            return Vec::new();
        };
        candidates &= db.entities.entities_with_attr_value(kid, vid);
    }

    candidates.iter().collect()
}

fn instance_tuples_for_relation(
    db: &PathDB,
    module_name: &str,
    schema_name: &str,
    instance_name: &str,
    relation_name: &str,
) -> Vec<u32> {
    let Some(kid) = db.interner.id_of(ATTR_AXI_RELATION) else {
        return Vec::new();
    };
    let Some(vid) = db.interner.id_of(relation_name) else {
        return Vec::new();
    };

    let mut candidates = db.entities.entities_with_attr_value(kid, vid);
    for (k, v) in [
        (ATTR_AXI_MODULE, module_name),
        (ATTR_AXI_SCHEMA, schema_name),
        (ATTR_AXI_INSTANCE, instance_name),
    ] {
        let Some(kid) = db.interner.id_of(k) else {
            return Vec::new();
        };
        let Some(vid) = db.interner.id_of(v) else {
            return Vec::new();
        };
        candidates &= db.entities.entities_with_attr_value(kid, vid);
    }
    candidates.iter().collect()
}

fn render_tuple_instance(
    db: &PathDB,
    tuple_entity: u32,
    fields: &[(String, String)],
) -> Result<String> {
    let mut parts: Vec<String> = Vec::with_capacity(fields.len());
    for (field_name, _ty) in fields {
        let targets = db.follow_one(tuple_entity, field_name);
        if targets.len() != 1 {
            return Err(anyhow!(
                "tuple entity {tuple_entity} expected exactly 1 target for field `{field_name}`, got {}",
                targets.len()
            ));
        }
        let target = targets.iter().next().expect("len==1");
        let value_name = entity_attr(db, target, META_ATTR_NAME)
            .ok_or_else(|| anyhow!("entity {target} missing `name`"))?;
        parts.push(format!("{field_name}={value_name}"));
    }
    Ok(format!("({})", parts.join(", ")))
}

fn format_ident_set(items: &[String]) -> String {
    if items.is_empty() {
        return "{}".to_string();
    }

    // Wrap long sets for readability.
    let inline = format!("{{{}}}", items.join(", "));
    if inline.len() <= 100 && items.len() <= 6 {
        return inline;
    }

    let mut out = String::new();
    out.push_str("{\n");
    for (i, it) in items.iter().enumerate() {
        let comma = if i + 1 == items.len() { "" } else { "," };
        out.push_str(&format!("    {it}{comma}\n"));
    }
    out.push_str("  }");
    out
}

fn format_tuple_set(items: &[String]) -> String {
    if items.is_empty() {
        return "{}".to_string();
    }

    let inline = format!("{{{}}}", items.join(", "));
    if inline.len() <= 120 && items.len() <= 3 {
        return inline;
    }

    let mut out = String::new();
    out.push_str("{\n");
    for (i, it) in items.iter().enumerate() {
        let comma = if i + 1 == items.len() { "" } else { "," };
        out.push_str(&format!("    {it}{comma}\n"));
    }
    out.push_str("  }");
    out
}
