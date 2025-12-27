//! JSON schema discovery for Axiograph
//!
//! Infers ontology structure from JSON data:
//! - Objects from JSON object types
//! - Arrays as relations
//! - Nested objects as compositions

#![allow(unused_imports)]

use anyhow::Result;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// Inferred JSON schema
#[derive(Debug, Clone, Default)]
pub struct JsonSchema {
    pub types: HashMap<String, JsonType>,
    pub root_type: Option<String>,
}

#[derive(Debug, Clone)]
pub enum JsonType {
    Object {
        fields: HashMap<String, JsonFieldType>,
    },
    Array {
        element_type: Box<JsonFieldType>,
    },
    Primitive(String),
}

#[derive(Debug, Clone)]
pub enum JsonFieldType {
    Required(String), // type name
    Optional(String),
    Array(String),
}

/// Infer schema from JSON value
pub fn infer_schema(value: &Value, root_name: &str) -> JsonSchema {
    let mut schema = JsonSchema::default();
    let root_type = infer_type(value, root_name, &mut schema);
    schema.root_type = Some(root_type);
    schema
}

fn infer_type(value: &Value, name: &str, schema: &mut JsonSchema) -> String {
    match value {
        Value::Null => "Null".to_string(),
        Value::Bool(_) => "Boolean".to_string(),
        Value::Number(_) => "Number".to_string(),
        Value::String(_) => "String".to_string(),
        Value::Array(arr) => {
            if arr.is_empty() {
                "EmptyArray".to_string()
            } else {
                let elem_name = format!("{}Item", name);
                let elem_type = infer_type(&arr[0], &elem_name, schema);
                schema.types.insert(
                    name.to_string(),
                    JsonType::Array {
                        element_type: Box::new(JsonFieldType::Required(elem_type)),
                    },
                );
                name.to_string()
            }
        }
        Value::Object(obj) => {
            let mut fields = HashMap::new();
            for (key, val) in obj {
                let field_type_name = format!("{}_{}", name, key);
                let field_type = infer_type(val, &field_type_name, schema);
                fields.insert(key.clone(), JsonFieldType::Required(field_type));
            }
            schema
                .types
                .insert(name.to_string(), JsonType::Object { fields });
            name.to_string()
        }
    }
}

// Note: this crate intentionally does *not* emit `.axi` directly. Ingestion
// produces untrusted `proposals.json` first; promotion into canonical `.axi`
// is explicit and reviewable.
