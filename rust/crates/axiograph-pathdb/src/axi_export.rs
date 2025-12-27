//! PathDB ↔ `.axi` (schema_v1) round-trip export/import.
//!
//! `.axpd` is the compact, binary on-disk format for PathDB. For version control
//! and human review, we also want a *textual, deterministic* representation.
//!
//! This module defines a **reversible** `.axi` export schema (`PathDBExportV1`)
//! that:
//! - preserves **entity ids**, **relation ids**, and **string ids**,
//! - avoids floats by storing confidences as **IEEE-754 bit patterns**,
//! - stores interned strings as **UTF-8 hex** (so it can represent any text),
//! - is intended as an *engineering interchange* format (not a user-facing DSL).
//!
//! The goal is faithful round-tripping:
//!
//! ```text
//!   PathDB (.axpd) → export (.axi) → import (.axi) → PathDB (.axpd)
//! ```
//!
//! NOTE: This export format is distinct from the domain `.axi` examples (like
//! `EconomicFlows.axi`). Those files are canonical *source*; this schema is a
//! stable "snapshot rendering" of the derived PathDB state.

use crate::{PathDB, StrId, StringInterner};
use anyhow::{anyhow, Result};
use axiograph_dsl::schema_v1::{parse_schema_v1, SchemaV1Instance, SchemaV1Module, SetItemV1};
use std::collections::BTreeSet;
use std::sync::atomic::Ordering;

pub const PATHDB_EXPORT_MODULE_NAME_V1: &str = "PathDBExport";
pub const PATHDB_EXPORT_SCHEMA_NAME_V1: &str = "PathDBExportV1";
pub const PATHDB_EXPORT_INSTANCE_NAME_V1: &str = "SnapshotV1";

// Objects
const OBJ_ENTITY: &str = "Entity";
const OBJ_RELATION: &str = "Relation";
const OBJ_INTERNED_STRING_ID: &str = "InternedStringId";
const OBJ_UTF8_STRING: &str = "Utf8String";
const OBJ_FLOAT32_BITS: &str = "Float32Bits";

// Relations
const REL_INTERNED_STRING: &str = "interned_string";
const REL_ENTITY_TYPE: &str = "entity_type";
const REL_ENTITY_ATTRIBUTE: &str = "entity_attribute";
const REL_RELATION_INFO: &str = "relation_info";
const REL_RELATION_ATTRIBUTE: &str = "relation_attribute";
const REL_EQUIVALENCE: &str = "equivalence";

// Token prefixes
const PREFIX_ENTITY: &str = "Entity_";
const PREFIX_RELATION: &str = "Relation_";
const PREFIX_STRING_ID: &str = "StringId_";
const PREFIX_STR_UTF8_HEX: &str = "StrUtf8Hex_";
const PREFIX_F32_HEX: &str = "F32Hex_";

fn token_u32(prefix: &str, value: u32) -> String {
    format!("{prefix}{value}")
}

fn parse_token_u32(prefix: &str, token: &str) -> Result<u32> {
    let rest = token
        .strip_prefix(prefix)
        .ok_or_else(|| anyhow!("expected token prefix `{prefix}`, got `{token}`"))?;
    rest.parse::<u32>()
        .map_err(|e| anyhow!("invalid u32 in token `{token}`: {e}"))
}

fn encode_utf8_hex(s: &str) -> String {
    let mut hex = String::with_capacity(s.len() * 2);
    for b in s.as_bytes() {
        use std::fmt::Write as _;
        let _ = write!(&mut hex, "{:02x}", b);
    }
    format!("{PREFIX_STR_UTF8_HEX}{hex}")
}

fn decode_utf8_hex(token: &str) -> Result<String> {
    let hex = token
        .strip_prefix(PREFIX_STR_UTF8_HEX)
        .ok_or_else(|| anyhow!("expected `{PREFIX_STR_UTF8_HEX}...`, got `{token}`"))?;
    if hex.len() % 2 != 0 {
        return Err(anyhow!("utf8 hex token has odd length: `{token}`"));
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    let mut i = 0usize;
    while i < hex.len() {
        let chunk = &hex[i..i + 2];
        let b = u8::from_str_radix(chunk, 16)
            .map_err(|e| anyhow!("invalid hex byte `{chunk}` in `{token}`: {e}"))?;
        bytes.push(b);
        i += 2;
    }
    String::from_utf8(bytes).map_err(|e| anyhow!("invalid UTF-8 in `{token}`: {e}"))
}

fn encode_f32_bits(x: f32) -> String {
    format!("{PREFIX_F32_HEX}{:08x}", x.to_bits())
}

fn decode_f32_bits(token: &str) -> Result<f32> {
    let hex = token
        .strip_prefix(PREFIX_F32_HEX)
        .ok_or_else(|| anyhow!("expected `{PREFIX_F32_HEX}...`, got `{token}`"))?;
    if hex.len() != 8 {
        return Err(anyhow!(
            "expected 8 hex digits for f32 bits, got {} in `{token}`",
            hex.len()
        ));
    }
    let bits =
        u32::from_str_radix(hex, 16).map_err(|e| anyhow!("invalid f32 bits `{token}`: {e}"))?;
    Ok(f32::from_bits(bits))
}

fn format_set(name: &str, items: &[String]) -> String {
    if items.is_empty() {
        return format!("  {name} = {{}}\n");
    }
    // Keep small sets on one line for readability in tests/examples.
    if items.len() <= 6 && items.iter().map(|s| s.len()).sum::<usize>() <= 72 {
        return format!("  {name} = {{{}}}\n", items.join(", "));
    }

    let mut out = String::new();
    out.push_str(&format!("  {name} = {{\n"));
    for (i, item) in items.iter().enumerate() {
        if i + 1 == items.len() {
            out.push_str(&format!("    {item}\n"));
        } else {
            out.push_str(&format!("    {item},\n"));
        }
    }
    out.push_str("  }\n");
    out
}

fn tuple(fields: &[(&str, String)]) -> String {
    let inner = fields
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("({inner})")
}

fn format_tuple_set(name: &str, tuples: &[String]) -> String {
    format_set(name, tuples)
}

fn find_export_instance<'a>(m: &'a SchemaV1Module) -> Result<&'a SchemaV1Instance> {
    m.instances
        .iter()
        .find(|i| i.schema == PATHDB_EXPORT_SCHEMA_NAME_V1)
        .ok_or_else(|| {
            anyhow!(
                "no instance `... of {}` found (this importer expects the PathDB export schema)",
                PATHDB_EXPORT_SCHEMA_NAME_V1
            )
        })
}

fn get_assignment<'a>(inst: &'a SchemaV1Instance, name: &str) -> Result<&'a [SetItemV1]> {
    let assignment = inst
        .assignments
        .iter()
        .find(|a| a.name == name)
        .ok_or_else(|| anyhow!("missing assignment `{name} = {{...}}` in export instance"))?;
    Ok(&assignment.value.items)
}

fn parse_ident_set(items: &[SetItemV1]) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for item in items {
        match item {
            SetItemV1::Ident { name } => out.push(name.clone()),
            SetItemV1::Tuple { .. } => {
                return Err(anyhow!("expected identifier set items, found tuple"));
            }
        }
    }
    Ok(out)
}

fn parse_tuple_set(items: &[SetItemV1]) -> Result<Vec<Vec<(String, String)>>> {
    let mut out = Vec::new();
    for item in items {
        match item {
            SetItemV1::Tuple { fields } => out.push(fields.clone()),
            SetItemV1::Ident { .. } => {
                return Err(anyhow!("expected tuple set items, found identifier"));
            }
        }
    }
    Ok(out)
}

fn tuple_field<'a>(fields: &'a [(String, String)], key: &str) -> Result<&'a str> {
    fields
        .iter()
        .find_map(|(k, v)| (k == key).then_some(v.as_str()))
        .ok_or_else(|| anyhow!("tuple missing field `{key}`"))
}

fn require_contiguous_ids(prefix: &str, tokens: &[String]) -> Result<u32> {
    let mut ids = tokens
        .iter()
        .map(|t| parse_token_u32(prefix, t))
        .collect::<Result<Vec<_>>>()?;
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        return Ok(0);
    }
    if ids[0] != 0 {
        return Err(anyhow!("expected `{prefix}0` to be present"));
    }
    for (expected, got) in (0u32..).zip(ids.iter().copied()) {
        if expected != got {
            return Err(anyhow!(
                "expected contiguous ids `{prefix}0..{prefix}N`, missing `{prefix}{expected}`"
            ));
        }
    }
    Ok(ids.len() as u32)
}

fn add_interned_strings(interner: &StringInterner, strings: &[String]) -> Result<()> {
    for (expected, s) in strings.iter().enumerate() {
        let got = interner.intern(s);
        if got.raw() != expected as u32 {
            return Err(anyhow!(
                "interner id mismatch: expected {expected}, got {} for string `{s}`",
                got.raw()
            ));
        }
    }
    Ok(())
}

/// Export a PathDB snapshot as `.axi` (schema_v1) using the `PathDBExportV1` schema.
pub fn export_pathdb_to_axi_v1(db: &PathDB) -> Result<String> {
    // Strings in stable id order (0..next_id).
    let max = db.interner.next_id.load(Ordering::SeqCst);
    let mut strings: Vec<String> = Vec::with_capacity(max as usize);
    for raw in 0..max {
        let Some(value) = db
            .interner
            .id_to_str
            .get(&StrId::new(raw))
            .map(|s| s.clone())
        else {
            return Err(anyhow!("missing interned string for id {raw}"));
        };
        strings.push(value);
    }

    let string_id_tokens: Vec<String> = (0..max).map(|i| token_u32(PREFIX_STRING_ID, i)).collect();
    let string_value_tokens: Vec<String> = strings.iter().map(|s| encode_utf8_hex(s)).collect();

    let entity_count = db.entities.next_id;
    let entity_tokens: Vec<String> = (0..entity_count)
        .map(|i| token_u32(PREFIX_ENTITY, i))
        .collect();

    let relation_count = db.relations.relations.len() as u32;
    let relation_tokens: Vec<String> = (0..relation_count)
        .map(|i| token_u32(PREFIX_RELATION, i))
        .collect();

    let mut float_tokens: BTreeSet<String> = BTreeSet::new();
    for rel in &db.relations.relations {
        float_tokens.insert(encode_f32_bits(rel.confidence));
    }

    // Relations: interned_string
    let interned_string_tuples: Vec<String> = (0..max)
        .map(|i| {
            tuple(&[
                ("interned_id", token_u32(PREFIX_STRING_ID, i)),
                ("value", string_value_tokens[i as usize].clone()),
            ])
        })
        .collect();

    // Relations: entity_type
    let mut entity_type_tuples: Vec<String> = Vec::with_capacity(entity_count as usize);
    for entity_id in 0..entity_count {
        let Some(type_id) = db.entities.types.get(entity_id as usize).copied() else {
            return Err(anyhow!("missing type for entity id {entity_id}"));
        };
        entity_type_tuples.push(tuple(&[
            ("entity", token_u32(PREFIX_ENTITY, entity_id)),
            ("type_id", token_u32(PREFIX_STRING_ID, type_id.raw())),
        ]));
    }

    // Relations: entity_attribute
    let mut entity_attr_rows: Vec<(u32, u32, u32)> = Vec::new();
    for (key_id, col) in &db.entities.attrs {
        for (entity_id, value_id) in col {
            entity_attr_rows.push((*entity_id, key_id.raw(), value_id.raw()));
        }
    }
    entity_attr_rows.sort_unstable();
    let entity_attribute_tuples: Vec<String> = entity_attr_rows
        .into_iter()
        .map(|(entity_id, key_id, value_id)| {
            tuple(&[
                ("entity", token_u32(PREFIX_ENTITY, entity_id)),
                ("key_id", token_u32(PREFIX_STRING_ID, key_id)),
                ("value_id", token_u32(PREFIX_STRING_ID, value_id)),
            ])
        })
        .collect();

    // Relations: relation_info + relation_attribute
    let mut relation_info_tuples: Vec<String> = Vec::with_capacity(relation_count as usize);
    let mut relation_attr_rows: Vec<(u32, u32, u32)> = Vec::new();
    for (rel_id, rel) in db.relations.relations.iter().enumerate() {
        let rel_id = rel_id as u32;
        let conf = encode_f32_bits(rel.confidence);
        relation_info_tuples.push(tuple(&[
            ("relation", token_u32(PREFIX_RELATION, rel_id)),
            (
                "rel_type_id",
                token_u32(PREFIX_STRING_ID, rel.rel_type.raw()),
            ),
            ("source", token_u32(PREFIX_ENTITY, rel.source)),
            ("target", token_u32(PREFIX_ENTITY, rel.target)),
            ("confidence", conf),
        ]));

        for (k, v) in &rel.attrs {
            relation_attr_rows.push((rel_id, k.raw(), v.raw()));
        }
    }
    relation_attr_rows.sort_unstable();
    let relation_attribute_tuples: Vec<String> = relation_attr_rows
        .into_iter()
        .map(|(rel_id, key_id, value_id)| {
            tuple(&[
                ("relation", token_u32(PREFIX_RELATION, rel_id)),
                ("key_id", token_u32(PREFIX_STRING_ID, key_id)),
                ("value_id", token_u32(PREFIX_STRING_ID, value_id)),
            ])
        })
        .collect();

    // Relations: equivalence (deduplicated, stable order)
    let mut equiv_rows: BTreeSet<(u32, u32, u32)> = BTreeSet::new();
    for (&e1, list) in &db.equivalences {
        for (e2, equiv_type) in list {
            let (a, b) = if e1 <= *e2 { (e1, *e2) } else { (*e2, e1) };
            equiv_rows.insert((a, b, equiv_type.raw()));
        }
    }
    let equivalence_tuples: Vec<String> = equiv_rows
        .into_iter()
        .map(|(a, b, equiv_type)| {
            tuple(&[
                ("entity", token_u32(PREFIX_ENTITY, a)),
                ("other", token_u32(PREFIX_ENTITY, b)),
                ("equiv_type_id", token_u32(PREFIX_STRING_ID, equiv_type)),
            ])
        })
        .collect();

    // ---------------------------------------------------------------------
    // Emit `.axi` text
    // ---------------------------------------------------------------------
    let mut out = String::new();
    out.push_str(&format!("module {PATHDB_EXPORT_MODULE_NAME_V1}\n\n"));
    out.push_str(&format!("schema {PATHDB_EXPORT_SCHEMA_NAME_V1}:\n"));
    out.push_str(&format!("  object {OBJ_ENTITY}\n"));
    out.push_str(&format!("  object {OBJ_RELATION}\n"));
    out.push_str(&format!("  object {OBJ_INTERNED_STRING_ID}\n"));
    out.push_str(&format!("  object {OBJ_UTF8_STRING}\n"));
    out.push_str(&format!("  object {OBJ_FLOAT32_BITS}\n"));
    out.push_str(&format!(
        "  relation {REL_INTERNED_STRING}(interned_id: {OBJ_INTERNED_STRING_ID}, value: {OBJ_UTF8_STRING})\n"
    ));
    out.push_str(&format!(
        "  relation {REL_ENTITY_TYPE}(entity: {OBJ_ENTITY}, type_id: {OBJ_INTERNED_STRING_ID})\n"
    ));
    out.push_str(&format!(
        "  relation {REL_ENTITY_ATTRIBUTE}(entity: {OBJ_ENTITY}, key_id: {OBJ_INTERNED_STRING_ID}, value_id: {OBJ_INTERNED_STRING_ID})\n"
    ));
    out.push_str(&format!(
        "  relation {REL_RELATION_INFO}(relation: {OBJ_RELATION}, rel_type_id: {OBJ_INTERNED_STRING_ID}, source: {OBJ_ENTITY}, target: {OBJ_ENTITY}, confidence: {OBJ_FLOAT32_BITS})\n"
    ));
    out.push_str(&format!(
        "  relation {REL_RELATION_ATTRIBUTE}(relation: {OBJ_RELATION}, key_id: {OBJ_INTERNED_STRING_ID}, value_id: {OBJ_INTERNED_STRING_ID})\n"
    ));
    out.push_str(&format!(
        "  relation {REL_EQUIVALENCE}(entity: {OBJ_ENTITY}, other: {OBJ_ENTITY}, equiv_type_id: {OBJ_INTERNED_STRING_ID})\n"
    ));
    out.push('\n');

    out.push_str(&format!(
        "instance {PATHDB_EXPORT_INSTANCE_NAME_V1} of {PATHDB_EXPORT_SCHEMA_NAME_V1}:\n"
    ));
    out.push_str(&format_set(OBJ_ENTITY, &entity_tokens));
    out.push_str(&format_set(OBJ_RELATION, &relation_tokens));
    out.push_str(&format_set(OBJ_INTERNED_STRING_ID, &string_id_tokens));
    out.push_str(&format_set(OBJ_UTF8_STRING, &string_value_tokens));
    out.push_str(&format_set(
        OBJ_FLOAT32_BITS,
        &float_tokens.into_iter().collect::<Vec<_>>(),
    ));
    out.push('\n');
    out.push_str(&format_tuple_set(
        REL_INTERNED_STRING,
        &interned_string_tuples,
    ));
    out.push_str(&format_tuple_set(REL_ENTITY_TYPE, &entity_type_tuples));
    out.push_str(&format_tuple_set(
        REL_ENTITY_ATTRIBUTE,
        &entity_attribute_tuples,
    ));
    out.push_str(&format_tuple_set(REL_RELATION_INFO, &relation_info_tuples));
    out.push_str(&format_tuple_set(
        REL_RELATION_ATTRIBUTE,
        &relation_attribute_tuples,
    ));
    out.push_str(&format_tuple_set(REL_EQUIVALENCE, &equivalence_tuples));

    Ok(out)
}

/// Import a PathDB snapshot from `.axi` (schema_v1) in the `PathDBExportV1` schema.
pub fn import_pathdb_from_axi_v1(text: &str) -> Result<PathDB> {
    let module =
        parse_schema_v1(text).map_err(|e| anyhow!("failed to parse axi_schema_v1 export: {e}"))?;
    import_pathdb_from_axi_v1_module(&module)
}

pub fn import_pathdb_from_axi_v1_module(module: &SchemaV1Module) -> Result<PathDB> {
    let inst = find_export_instance(module)?;

    // ---------------------------------------------------------------------
    // Decode interned string table (stable ids).
    // ---------------------------------------------------------------------
    let string_id_tokens = parse_ident_set(get_assignment(inst, OBJ_INTERNED_STRING_ID)?)?;
    let string_count = require_contiguous_ids(PREFIX_STRING_ID, &string_id_tokens)?;

    let interned_tuples = parse_tuple_set(get_assignment(inst, REL_INTERNED_STRING)?)?;
    let mut strings_by_id: Vec<Option<String>> = vec![None; string_count as usize];
    for fields in interned_tuples {
        let sid_tok = tuple_field(&fields, "interned_id")?;
        let value_tok = tuple_field(&fields, "value")?;
        let sid = parse_token_u32(PREFIX_STRING_ID, sid_tok)?;
        if sid >= string_count {
            return Err(anyhow!(
                "interned_string references out-of-range string id {sid} (count={string_count})"
            ));
        }
        let value = decode_utf8_hex(value_tok)?;
        strings_by_id[sid as usize] = Some(value);
    }
    let mut strings: Vec<String> = Vec::with_capacity(string_count as usize);
    for (i, opt) in strings_by_id.into_iter().enumerate() {
        let Some(s) = opt else {
            return Err(anyhow!(
                "missing interned_string mapping for {PREFIX_STRING_ID}{i}"
            ));
        };
        strings.push(s);
    }

    // ---------------------------------------------------------------------
    // Decode entities.
    // ---------------------------------------------------------------------
    let entity_tokens = parse_ident_set(get_assignment(inst, OBJ_ENTITY)?)?;
    let entity_count = require_contiguous_ids(PREFIX_ENTITY, &entity_tokens)?;

    let entity_type_tuples = parse_tuple_set(get_assignment(inst, REL_ENTITY_TYPE)?)?;
    let mut entity_type: Vec<Option<u32>> = vec![None; entity_count as usize];
    for fields in entity_type_tuples {
        let entity_tok = tuple_field(&fields, "entity")?;
        let type_tok = tuple_field(&fields, "type_id")?;
        let entity_id = parse_token_u32(PREFIX_ENTITY, entity_tok)?;
        let type_id = parse_token_u32(PREFIX_STRING_ID, type_tok)?;
        if entity_id >= entity_count {
            return Err(anyhow!(
                "entity_type references out-of-range entity id {entity_id} (count={entity_count})"
            ));
        }
        if type_id >= string_count {
            return Err(anyhow!(
                "entity_type references out-of-range string id {type_id} (count={string_count})"
            ));
        }
        entity_type[entity_id as usize] = Some(type_id);
    }

    let entity_attr_tuples = parse_tuple_set(get_assignment(inst, REL_ENTITY_ATTRIBUTE)?)?;
    let mut entity_attrs: Vec<Vec<(u32, u32)>> = vec![Vec::new(); entity_count as usize];
    for fields in entity_attr_tuples {
        let entity_tok = tuple_field(&fields, "entity")?;
        let key_tok = tuple_field(&fields, "key_id")?;
        let value_tok = tuple_field(&fields, "value_id")?;
        let entity_id = parse_token_u32(PREFIX_ENTITY, entity_tok)?;
        let key_id = parse_token_u32(PREFIX_STRING_ID, key_tok)?;
        let value_id = parse_token_u32(PREFIX_STRING_ID, value_tok)?;
        if entity_id >= entity_count {
            return Err(anyhow!(
                "entity_attribute references out-of-range entity id {entity_id} (count={entity_count})"
            ));
        }
        if key_id >= string_count || value_id >= string_count {
            return Err(anyhow!(
                "entity_attribute references out-of-range string id (key={key_id}, value={value_id}, count={string_count})"
            ));
        }
        entity_attrs[entity_id as usize].push((key_id, value_id));
    }

    // ---------------------------------------------------------------------
    // Decode relations.
    // ---------------------------------------------------------------------
    let relation_tokens = parse_ident_set(get_assignment(inst, OBJ_RELATION)?)?;
    let relation_count = require_contiguous_ids(PREFIX_RELATION, &relation_tokens)?;

    let relation_info_tuples = parse_tuple_set(get_assignment(inst, REL_RELATION_INFO)?)?;
    let mut relation_info: Vec<Option<(u32, u32, u32, f32)>> = vec![None; relation_count as usize];
    for fields in relation_info_tuples {
        let rel_tok = tuple_field(&fields, "relation")?;
        let rel_type_tok = tuple_field(&fields, "rel_type_id")?;
        let source_tok = tuple_field(&fields, "source")?;
        let target_tok = tuple_field(&fields, "target")?;
        let conf_tok = tuple_field(&fields, "confidence")?;

        let rel_id = parse_token_u32(PREFIX_RELATION, rel_tok)?;
        let rel_type_id = parse_token_u32(PREFIX_STRING_ID, rel_type_tok)?;
        let source = parse_token_u32(PREFIX_ENTITY, source_tok)?;
        let target = parse_token_u32(PREFIX_ENTITY, target_tok)?;
        let confidence = decode_f32_bits(conf_tok)?;

        if rel_id >= relation_count {
            return Err(anyhow!(
                "relation_info references out-of-range relation id {rel_id} (count={relation_count})"
            ));
        }
        if rel_type_id >= string_count {
            return Err(anyhow!(
                "relation_info references out-of-range string id {rel_type_id} (count={string_count})"
            ));
        }
        if source >= entity_count || target >= entity_count {
            return Err(anyhow!(
                "relation_info references out-of-range entity id (source={source}, target={target}, entity_count={entity_count})"
            ));
        }

        relation_info[rel_id as usize] = Some((rel_type_id, source, target, confidence));
    }

    let relation_attr_tuples = parse_tuple_set(get_assignment(inst, REL_RELATION_ATTRIBUTE)?)?;
    let mut relation_attrs: Vec<Vec<(u32, u32)>> = vec![Vec::new(); relation_count as usize];
    for fields in relation_attr_tuples {
        let rel_tok = tuple_field(&fields, "relation")?;
        let key_tok = tuple_field(&fields, "key_id")?;
        let value_tok = tuple_field(&fields, "value_id")?;
        let rel_id = parse_token_u32(PREFIX_RELATION, rel_tok)?;
        let key_id = parse_token_u32(PREFIX_STRING_ID, key_tok)?;
        let value_id = parse_token_u32(PREFIX_STRING_ID, value_tok)?;
        if rel_id >= relation_count {
            return Err(anyhow!(
                "relation_attribute references out-of-range relation id {rel_id} (count={relation_count})"
            ));
        }
        if key_id >= string_count || value_id >= string_count {
            return Err(anyhow!(
                "relation_attribute references out-of-range string id (key={key_id}, value={value_id}, count={string_count})"
            ));
        }
        relation_attrs[rel_id as usize].push((key_id, value_id));
    }

    let equivalence_tuples = parse_tuple_set(get_assignment(inst, REL_EQUIVALENCE)?)?;
    let mut equivalences: Vec<(u32, u32, u32)> = Vec::new();
    for fields in equivalence_tuples {
        let entity_tok = tuple_field(&fields, "entity")?;
        let other_tok = tuple_field(&fields, "other")?;
        let equiv_type_tok = tuple_field(&fields, "equiv_type_id")?;
        let e1 = parse_token_u32(PREFIX_ENTITY, entity_tok)?;
        let e2 = parse_token_u32(PREFIX_ENTITY, other_tok)?;
        let t = parse_token_u32(PREFIX_STRING_ID, equiv_type_tok)?;
        if e1 >= entity_count || e2 >= entity_count {
            return Err(anyhow!(
                "equivalence references out-of-range entity id (e1={e1}, e2={e2}, entity_count={entity_count})"
            ));
        }
        if t >= string_count {
            return Err(anyhow!(
                "equivalence references out-of-range string id {t} (count={string_count})"
            ));
        }
        equivalences.push((e1, e2, t));
    }

    // ---------------------------------------------------------------------
    // Build PathDB (stable ids) and rebuild derived indexes.
    // ---------------------------------------------------------------------
    let mut db = PathDB::new();
    add_interned_strings(&db.interner, &strings)?;

    for entity_id in 0..entity_count {
        let Some(type_id) = entity_type[entity_id as usize] else {
            return Err(anyhow!(
                "missing entity_type row for {PREFIX_ENTITY}{entity_id}"
            ));
        };
        let type_name = strings
            .get(type_id as usize)
            .ok_or_else(|| anyhow!("missing string for type_id {type_id}"))?;

        let attrs = &entity_attrs[entity_id as usize];
        let mut attrs_str: Vec<(&str, &str)> = Vec::with_capacity(attrs.len());
        for (k, v) in attrs {
            let k = strings
                .get(*k as usize)
                .ok_or_else(|| anyhow!("missing string for key_id {k}"))?;
            let v = strings
                .get(*v as usize)
                .ok_or_else(|| anyhow!("missing string for value_id {v}"))?;
            attrs_str.push((k.as_str(), v.as_str()));
        }

        let got = db.add_entity(type_name.as_str(), attrs_str);
        if got != entity_id {
            return Err(anyhow!(
                "entity id mismatch: expected {entity_id}, got {got} (input must be contiguous and ordered)"
            ));
        }
    }

    for rel_id in 0..relation_count {
        let Some((rel_type_id, source, target, confidence)) = relation_info[rel_id as usize] else {
            return Err(anyhow!(
                "missing relation_info row for {PREFIX_RELATION}{rel_id}"
            ));
        };
        let rel_type = strings
            .get(rel_type_id as usize)
            .ok_or_else(|| anyhow!("missing string for rel_type_id {rel_type_id}"))?;

        let attrs = &relation_attrs[rel_id as usize];
        let mut attrs_str: Vec<(&str, &str)> = Vec::with_capacity(attrs.len());
        for (k, v) in attrs {
            let k = strings
                .get(*k as usize)
                .ok_or_else(|| anyhow!("missing string for key_id {k}"))?;
            let v = strings
                .get(*v as usize)
                .ok_or_else(|| anyhow!("missing string for value_id {v}"))?;
            attrs_str.push((k.as_str(), v.as_str()));
        }

        let got = db.add_relation(rel_type.as_str(), source, target, confidence, attrs_str);
        if got != rel_id {
            return Err(anyhow!(
                "relation id mismatch: expected {rel_id}, got {got} (input must be contiguous and ordered)"
            ));
        }
    }

    // Equivalences are undirected; keep them as explicit edges.
    // De-duplicate to avoid multiplying entries on import.
    let mut seen: BTreeSet<(u32, u32, u32)> = BTreeSet::new();
    for (e1, e2, t) in equivalences {
        let (a, b) = if e1 <= e2 { (e1, e2) } else { (e2, e1) };
        if !seen.insert((a, b, t)) {
            continue;
        }
        let equiv_type = strings
            .get(t as usize)
            .ok_or_else(|| anyhow!("missing string for equiv_type_id {t}"))?;
        db.add_equivalence(a, b, equiv_type.as_str());
    }

    db.build_indexes();
    Ok(db)
}
