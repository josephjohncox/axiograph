//! IGES file parser for Axiograph
//!
//! Parses IGES (Initial Graphics Exchange Specification) files
//! and extracts entity structure for knowledge graphs.

use anyhow::{anyhow, Result};
use axiograph_dsl as dsl;
use std::collections::HashMap;

/// IGES entity types we care about
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IgesEntityType {
    CircularArc,           // 100
    CompositeCurve,        // 102
    Line,                  // 110
    ParametricSpline,      // 112
    Point,                 // 116
    RuledSurface,          // 118
    SurfaceOfRevolution,   // 120
    TabulatedCylinder,     // 122
    TransformationMatrix,  // 124
    RationalBSplineCurve,  // 126
    RationalBSplineSurface,// 128
    TrimmedSurface,        // 144
    BoundedSurface,        // 143
    Other(i32),
}

impl From<i32> for IgesEntityType {
    fn from(code: i32) -> Self {
        match code {
            100 => Self::CircularArc,
            102 => Self::CompositeCurve,
            110 => Self::Line,
            112 => Self::ParametricSpline,
            116 => Self::Point,
            118 => Self::RuledSurface,
            120 => Self::SurfaceOfRevolution,
            122 => Self::TabulatedCylinder,
            124 => Self::TransformationMatrix,
            126 => Self::RationalBSplineCurve,
            128 => Self::RationalBSplineSurface,
            144 => Self::TrimmedSurface,
            143 => Self::BoundedSurface,
            n => Self::Other(n),
        }
    }
}

/// IGES entity
#[derive(Debug, Clone)]
pub struct IgesEntity {
    pub id: usize,
    pub entity_type: IgesEntityType,
    pub type_code: i32,
    pub parameters: Vec<String>,
    pub references: Vec<usize>,
}

/// IGES file structure
#[derive(Debug, Clone, Default)]
pub struct IgesFile {
    pub entities: HashMap<usize, IgesEntity>,
    pub start_section: String,
    pub global_section: String,
}

/// Parse IGES file
pub fn parse_iges(content: &str) -> Result<IgesFile> {
    let mut file = IgesFile::default();

    // IGES format: 80-character records with section code in column 73
    let lines: Vec<&str> = content.lines().collect();

    let mut directory_entries: Vec<(usize, i32)> = Vec::new();  // (seq, type)
    let mut parameter_data: HashMap<usize, Vec<String>> = HashMap::new();

    for line in &lines {
        if line.len() < 73 {
            continue;
        }

        let section = line.chars().nth(72).unwrap_or(' ');
        let content = &line[..72];

        match section {
            'S' => {
                file.start_section.push_str(content.trim());
            }
            'G' => {
                file.global_section.push_str(content.trim());
            }
            'D' => {
                // Directory entry: pairs of lines
                // First line has entity type in columns 1-8
                let type_code = content[0..8].trim().parse::<i32>().unwrap_or(0);
                let seq_str = line[73..].trim();
                let seq = seq_str.parse::<usize>().unwrap_or(0);
                directory_entries.push((seq, type_code));
            }
            'P' => {
                // Parameter data
                let seq_str = line[73..].trim();
                if let Ok(seq) = seq_str.parse::<usize>() {
                    let params: Vec<String> = content
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .collect();
                    parameter_data.entry(seq).or_default().extend(params);
                }
            }
            _ => {}
        }
    }

    // Build entities
    for (i, (seq, type_code)) in directory_entries.iter().enumerate() {
        let entity = IgesEntity {
            id: i + 1,
            entity_type: IgesEntityType::from(*type_code),
            type_code: *type_code,
            parameters: parameter_data.get(seq).cloned().unwrap_or_default(),
            references: vec![],  // Would need more parsing
        };
        file.entities.insert(i + 1, entity);
    }

    Ok(file)
}

/// Convert IGES file to Axiograph module
pub fn iges_to_module(iges: &IgesFile, module_name: &str) -> dsl::Module {
    let mut module = dsl::Module::new(module_name);

    let schema = dsl::Schema {
        name: "IGES".to_string(),
        objects: vec![
            "Entity".to_string(),
            "EntityType".to_string(),
            "Point".to_string(),
            "Curve".to_string(),
            "Surface".to_string(),
        ],
        arrows: vec![
            dsl::ArrowDecl {
                name: "entityType".to_string(),
                src: "Entity".to_string(),
                dst: "EntityType".to_string(),
            },
        ],
        subtypes: vec![
            dsl::SubtypeDecl {
                sub: "Point".to_string(),
                sup: "Entity".to_string(),
                incl: "pointIncl".to_string(),
            },
            dsl::SubtypeDecl {
                sub: "Curve".to_string(),
                sup: "Entity".to_string(),
                incl: "curveIncl".to_string(),
            },
            dsl::SubtypeDecl {
                sub: "Surface".to_string(),
                sup: "Entity".to_string(),
                incl: "surfaceIncl".to_string(),
            },
        ],
        relations: vec![
            dsl::RelationDecl {
                name: "References".to_string(),
                fields: vec![
                    dsl::FieldDecl { field: "from".to_string(), ty: "Entity".to_string() },
                    dsl::FieldDecl { field: "to".to_string(), ty: "Entity".to_string() },
                ],
                context: None,
                temporal: None,
            },
        ],
        equations: vec![],
    };
    module.schemas.push(schema);

    // Create instance
    let mut instance = dsl::Instance {
        name: "FromIGES".to_string(),
        schema: "IGES".to_string(),
        objects: vec![],
        arrows: vec![],
        relations: vec![],
    };

    // Entity types
    let mut entity_types: std::collections::HashSet<String> = std::collections::HashSet::new();
    for ent in iges.entities.values() {
        entity_types.insert(format!("Type_{}", ent.type_code));
    }

    instance.objects.push(dsl::ObjElems {
        obj: "Entity".to_string(),
        elems: iges.entities.keys().map(|id| format!("E{}", id)).collect(),
    });

    instance.objects.push(dsl::ObjElems {
        obj: "EntityType".to_string(),
        elems: entity_types.into_iter().collect(),
    });

    // Arrow mappings
    instance.arrows.push(dsl::ArrowMapEntry {
        arrow: "entityType".to_string(),
        pairs: iges
            .entities
            .iter()
            .map(|(id, ent)| (format!("E{}", id), format!("Type_{}", ent.type_code)))
            .collect(),
    });

    module.instances.push(instance);
    module
}

