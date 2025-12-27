//! STEP/IGES/B-Rep ingestion for Axiograph
//!
//! This crate parses CAD files (STEP ISO-10303-21) and extracts:
//! - Assembly/product structure
//! - B-Rep topology (solids, shells, faces, edges, vertices)
//! - Feature recognition hints (holes, pockets, bosses)
//!
//! **FFI Policy**: This is heavy IO/parsing. Semantics live in Idris.

use anyhow::{anyhow, Result};
use axiograph_dsl as dsl;
use regex::Regex;
use std::collections::{HashMap, HashSet};

// ============================================================================
// STEP value types
// ============================================================================

#[derive(Debug, Clone)]
pub enum StepValue {
    Ref(u32),
    Str(String),
    Enum(String),
    Bool(bool),
    Int(i64),
    Real(f64),
    Null,
    Omitted,
    List(Vec<StepValue>),
    Typed(String, Vec<StepValue>),
}

#[derive(Debug, Clone)]
pub struct StepEntity {
    pub id: u32,
    pub name: String,
    pub args: Vec<StepValue>,
}

// ============================================================================
// B-Rep lift result
// ============================================================================

#[derive(Debug, Clone, Default)]
pub struct BrepLift {
    pub solids: HashSet<u32>,
    pub shells: HashSet<u32>,
    pub faces: HashSet<u32>,
    pub edges: HashSet<u32>,
    pub vertices: HashSet<u32>,

    pub solid_shell: Vec<(u32, u32)>,
    pub shell_face: Vec<(u32, u32)>,
    pub face_edge: Vec<(u32, u32)>,
    pub edge_vertex: Vec<(u32, u32, u32)>,

    /// Feature tags: (tag, face_id)
    pub features: Vec<(String, u32)>,

    // Assembly structure
    pub product_defs: HashSet<u32>,
    pub assembly_uses: Vec<(u32, u32)>,
    pub def_solid: Vec<(u32, u32)>,
}

// ============================================================================
// Parser
// ============================================================================

/// Parse a STEP file into entity map
pub fn parse_step(input: &str) -> Result<HashMap<u32, StepEntity>> {
    let upper = input.to_ascii_uppercase();
    let mut entities = HashMap::new();

    // Find DATA sections
    let mut pos = 0;
    while let Some(start) = upper[pos..].find("DATA;") {
        let start = pos + start + 5;
        let Some(end_offset) = upper[start..].find("ENDSEC;") else {
            break;
        };
        let end = start + end_offset;

        // Parse entities in this section
        let data = &input[start..end];
        for ent in parse_entities(data)? {
            entities.insert(ent.id, ent);
        }

        pos = end + 7;
    }

    if entities.is_empty() {
        return Err(anyhow!("No DATA section found in STEP file"));
    }

    Ok(entities)
}

fn parse_entities(data: &str) -> Result<Vec<StepEntity>> {
    let re_comment = Regex::new(r"(?s)/\*.*?\*/").unwrap();
    let clean = re_comment.replace_all(data, "").replace('\n', " ");

    let mut entities = Vec::new();
    let mut buf = String::new();
    let mut depth = 0;

    for ch in clean.chars() {
        match ch {
            '(' => {
                depth += 1;
                buf.push(ch);
            }
            ')' => {
                depth -= 1;
                buf.push(ch);
            }
            ';' if depth == 0 => {
                let rec = buf.trim();
                if rec.starts_with('#') {
                    if let Ok(ent) = parse_entity_record(rec) {
                        entities.push(ent);
                    }
                }
                buf.clear();
            }
            _ => buf.push(ch),
        }
    }

    Ok(entities)
}

fn parse_entity_record(rec: &str) -> Result<StepEntity> {
    let rec = rec.trim();
    let re = Regex::new(r"^#(\d+)\s*=\s*([A-Za-z0-9_]+)\s*\((.*)$").unwrap();

    let Some(caps) = re.captures(rec) else {
        return Err(anyhow!("Invalid entity record: {}", rec));
    };

    let id: u32 = caps[1].parse()?;
    let name = caps[2].to_string();

    // For simplicity, we just store the raw args string
    // A full parser would recursively parse the argument list
    let args_str = caps[3].trim_end_matches(')');
    let args = parse_args_simple(args_str);

    Ok(StepEntity { id, name, args })
}

fn parse_args_simple(s: &str) -> Vec<StepValue> {
    // Simple argument parsing: split on commas (not inside parens)
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut depth = 0;

    for ch in s.chars() {
        match ch {
            '(' => {
                depth += 1;
                cur.push(ch);
            }
            ')' => {
                depth -= 1;
                cur.push(ch);
            }
            ',' if depth == 0 => {
                args.push(parse_value(cur.trim()));
                cur.clear();
            }
            _ => cur.push(ch),
        }
    }
    if !cur.trim().is_empty() {
        args.push(parse_value(cur.trim()));
    }

    args
}

fn parse_value(s: &str) -> StepValue {
    let s = s.trim();
    if s.starts_with('#') {
        if let Ok(id) = s[1..].parse::<u32>() {
            return StepValue::Ref(id);
        }
    }
    if s.starts_with('\'') && s.ends_with('\'') {
        return StepValue::Str(s[1..s.len() - 1].to_string());
    }
    if s == "$" {
        return StepValue::Null;
    }
    if s == "*" {
        return StepValue::Omitted;
    }
    if s.starts_with('.') && s.ends_with('.') {
        let inner = &s[1..s.len() - 1];
        if inner == "T" {
            return StepValue::Bool(true);
        }
        if inner == "F" {
            return StepValue::Bool(false);
        }
        return StepValue::Enum(inner.to_string());
    }
    if s.starts_with('(') && s.ends_with(')') {
        let inner = &s[1..s.len() - 1];
        return StepValue::List(parse_args_simple(inner));
    }
    if let Ok(n) = s.parse::<i64>() {
        return StepValue::Int(n);
    }
    if let Ok(n) = s.parse::<f64>() {
        return StepValue::Real(n);
    }
    StepValue::Str(s.to_string())
}

// ============================================================================
// B-Rep lifting
// ============================================================================

/// Lift B-Rep topology from STEP entities
pub fn lift_brep(entities: &HashMap<u32, StepEntity>) -> BrepLift {
    let mut brep = BrepLift::default();

    // Collect by entity type
    for (id, ent) in entities {
        let name_upper = ent.name.to_ascii_uppercase();

        match name_upper.as_str() {
            "MANIFOLD_SOLID_BREP" => {
                brep.solids.insert(*id);
                if let Some(StepValue::Ref(shell)) = ent.args.get(1) {
                    brep.shells.insert(*shell);
                    brep.solid_shell.push((*id, *shell));
                }
            }
            "CLOSED_SHELL" | "OPEN_SHELL" => {
                brep.shells.insert(*id);
                if let Some(StepValue::List(faces)) = ent.args.get(1) {
                    for f in faces {
                        if let StepValue::Ref(fid) = f {
                            brep.faces.insert(*fid);
                            brep.shell_face.push((*id, *fid));
                        }
                    }
                }
            }
            "ADVANCED_FACE" => {
                brep.faces.insert(*id);
            }
            "EDGE_CURVE" => {
                brep.edges.insert(*id);
                // Extract vertex endpoints
                if let (Some(StepValue::Ref(v1)), Some(StepValue::Ref(v2))) =
                    (ent.args.get(1), ent.args.get(2))
                {
                    brep.vertices.insert(*v1);
                    brep.vertices.insert(*v2);
                    brep.edge_vertex.push((*id, *v1, *v2));
                }
            }
            "VERTEX_POINT" => {
                brep.vertices.insert(*id);
            }
            "PRODUCT_DEFINITION" => {
                brep.product_defs.insert(*id);
            }
            "NEXT_ASSEMBLY_USAGE_OCCURRENCE" => {
                // Assembly structure
                if let (Some(StepValue::Ref(parent)), Some(StepValue::Ref(child))) =
                    (ent.args.get(3), ent.args.get(4))
                {
                    brep.product_defs.insert(*parent);
                    brep.product_defs.insert(*child);
                    brep.assembly_uses.push((*parent, *child));
                }
            }
            _ => {}
        }
    }

    // Feature recognition (heuristic)
    for (id, ent) in entities {
        if ent.name.eq_ignore_ascii_case("ADVANCED_FACE") {
            if let Some(StepValue::Ref(surf)) = ent.args.get(1) {
                if let Some(surf_ent) = entities.get(surf) {
                    let surf_type = surf_ent.name.to_ascii_uppercase();
                    if surf_type == "CYLINDRICAL_SURFACE" {
                        brep.features.push(("HoleOrBoss:Cylindrical".to_string(), *id));
                    } else if surf_type == "PLANE" {
                        // Check for inner loops (pocket candidate)
                        if let Some(StepValue::List(bounds)) = ent.args.get(0) {
                            if bounds.len() > 1 {
                                brep.features.push(("PocketCandidate:PlanarWithInnerLoop".to_string(), *id));
                            }
                        }
                    }
                }
            }
        }
    }

    brep
}

// ============================================================================
// Convert to Axiograph module
// ============================================================================

/// Convert STEP file to Axiograph module
pub fn step_to_module(step_text: &str, module_name: &str) -> Result<dsl::Module> {
    let entities = parse_step(step_text)?;
    let brep = lift_brep(&entities);

    let mut module = dsl::Module::new(module_name);

    // Create B-Rep schema
    let schema = dsl::Schema {
        name: "BRep".to_string(),
        objects: vec![
            "Solid".to_string(),
            "Shell".to_string(),
            "Face".to_string(),
            "Edge".to_string(),
            "Vertex".to_string(),
            "FeatureTag".to_string(),
            "ProductDef".to_string(),
        ],
        arrows: vec![],
        subtypes: vec![],
        relations: vec![
            dsl::RelationDecl {
                name: "SolidShell".to_string(),
                fields: vec![
                    dsl::FieldDecl { field: "solid".to_string(), ty: "Solid".to_string() },
                    dsl::FieldDecl { field: "shell".to_string(), ty: "Shell".to_string() },
                ],
                context: None,
                temporal: None,
            },
            dsl::RelationDecl {
                name: "ShellFace".to_string(),
                fields: vec![
                    dsl::FieldDecl { field: "shell".to_string(), ty: "Shell".to_string() },
                    dsl::FieldDecl { field: "face".to_string(), ty: "Face".to_string() },
                ],
                context: None,
                temporal: None,
            },
            dsl::RelationDecl {
                name: "FaceEdge".to_string(),
                fields: vec![
                    dsl::FieldDecl { field: "face".to_string(), ty: "Face".to_string() },
                    dsl::FieldDecl { field: "edge".to_string(), ty: "Edge".to_string() },
                ],
                context: None,
                temporal: None,
            },
            dsl::RelationDecl {
                name: "EdgeVertex".to_string(),
                fields: vec![
                    dsl::FieldDecl { field: "edge".to_string(), ty: "Edge".to_string() },
                    dsl::FieldDecl { field: "vStart".to_string(), ty: "Vertex".to_string() },
                    dsl::FieldDecl { field: "vEnd".to_string(), ty: "Vertex".to_string() },
                ],
                context: None,
                temporal: None,
            },
            dsl::RelationDecl {
                name: "FaceFeature".to_string(),
                fields: vec![
                    dsl::FieldDecl { field: "face".to_string(), ty: "Face".to_string() },
                    dsl::FieldDecl { field: "tag".to_string(), ty: "FeatureTag".to_string() },
                ],
                context: None,
                temporal: None,
            },
            dsl::RelationDecl {
                name: "AssemblyUses".to_string(),
                fields: vec![
                    dsl::FieldDecl { field: "parent".to_string(), ty: "ProductDef".to_string() },
                    dsl::FieldDecl { field: "child".to_string(), ty: "ProductDef".to_string() },
                ],
                context: None,
                temporal: None,
            },
        ],
        equations: vec![],
    };
    module.schemas.push(schema);

    // Create instance with data
    let mut instance = dsl::Instance {
        name: "FromSTEP".to_string(),
        schema: "BRep".to_string(),
        objects: vec![],
        arrows: vec![],
        relations: vec![],
    };

    // Populate object carriers
    instance.objects.push(dsl::ObjElems {
        obj: "Solid".to_string(),
        elems: brep.solids.iter().map(|id| format!("S{}", id)).collect(),
    });
    instance.objects.push(dsl::ObjElems {
        obj: "Shell".to_string(),
        elems: brep.shells.iter().map(|id| format!("Sh{}", id)).collect(),
    });
    instance.objects.push(dsl::ObjElems {
        obj: "Face".to_string(),
        elems: brep.faces.iter().map(|id| format!("F{}", id)).collect(),
    });
    instance.objects.push(dsl::ObjElems {
        obj: "Edge".to_string(),
        elems: brep.edges.iter().map(|id| format!("E{}", id)).collect(),
    });
    instance.objects.push(dsl::ObjElems {
        obj: "Vertex".to_string(),
        elems: brep.vertices.iter().map(|id| format!("V{}", id)).collect(),
    });

    // Feature tags
    let feature_tags: HashSet<String> = brep.features.iter().map(|(t, _)| t.clone()).collect();
    instance.objects.push(dsl::ObjElems {
        obj: "FeatureTag".to_string(),
        elems: feature_tags.into_iter().collect(),
    });

    // Product definitions
    instance.objects.push(dsl::ObjElems {
        obj: "ProductDef".to_string(),
        elems: brep.product_defs.iter().map(|id| format!("PD{}", id)).collect(),
    });

    // Relation tuples
    instance.relations.push(dsl::RelInstanceEntry {
        rel: "SolidShell".to_string(),
        tuples: brep
            .solid_shell
            .iter()
            .map(|(s, sh)| dsl::RelTuple {
                fields: vec![
                    ("solid".to_string(), format!("S{}", s)),
                    ("shell".to_string(), format!("Sh{}", sh)),
                ],
            })
            .collect(),
    });

    instance.relations.push(dsl::RelInstanceEntry {
        rel: "ShellFace".to_string(),
        tuples: brep
            .shell_face
            .iter()
            .map(|(sh, f)| dsl::RelTuple {
                fields: vec![
                    ("shell".to_string(), format!("Sh{}", sh)),
                    ("face".to_string(), format!("F{}", f)),
                ],
            })
            .collect(),
    });

    instance.relations.push(dsl::RelInstanceEntry {
        rel: "EdgeVertex".to_string(),
        tuples: brep
            .edge_vertex
            .iter()
            .map(|(e, v1, v2)| dsl::RelTuple {
                fields: vec![
                    ("edge".to_string(), format!("E{}", e)),
                    ("vStart".to_string(), format!("V{}", v1)),
                    ("vEnd".to_string(), format!("V{}", v2)),
                ],
            })
            .collect(),
    });

    instance.relations.push(dsl::RelInstanceEntry {
        rel: "FaceFeature".to_string(),
        tuples: brep
            .features
            .iter()
            .map(|(tag, fid)| dsl::RelTuple {
                fields: vec![
                    ("face".to_string(), format!("F{}", fid)),
                    ("tag".to_string(), tag.clone()),
                ],
            })
            .collect(),
    });

    instance.relations.push(dsl::RelInstanceEntry {
        rel: "AssemblyUses".to_string(),
        tuples: brep
            .assembly_uses
            .iter()
            .map(|(p, c)| dsl::RelTuple {
                fields: vec![
                    ("parent".to_string(), format!("PD{}", p)),
                    ("child".to_string(), format!("PD{}", c)),
                ],
            })
            .collect(),
    });

    module.instances.push(instance);

    Ok(module)
}

