//! STEP File Parser
//!
//! Real implementation for parsing STEP (ISO 10303) files.
//! Supports AP203 (Configuration Controlled Design) and AP214 (Core Data for Automotive Design).

use nom::{
    IResult,
    branch::alt,
    bytes::complete::{tag, take_until, take_while, take_while1},
    character::complete::{alpha1, alphanumeric1, char, multispace0, digit1},
    combinator::{map, opt, recognize, value},
    multi::{many0, separated_list0},
    sequence::{delimited, pair, preceded, separated_pair, terminated, tuple},
};
use std::collections::HashMap;

// ============================================================================
// STEP Data Types
// ============================================================================

/// A complete STEP file
#[derive(Debug, Clone)]
pub struct StepFile {
    pub header: StepHeader,
    pub data: Vec<StepEntity>,
}

/// STEP file header section
#[derive(Debug, Clone)]
pub struct StepHeader {
    pub file_description: Vec<String>,
    pub file_name: String,
    pub file_schema: Vec<String>,
}

/// A STEP entity instance
#[derive(Debug, Clone)]
pub struct StepEntity {
    pub id: u64,
    pub type_name: String,
    pub attributes: Vec<StepValue>,
}

/// STEP attribute value
#[derive(Debug, Clone)]
pub enum StepValue {
    Null,
    Missing,
    Integer(i64),
    Real(f64),
    String(String),
    Enum(String),
    Binary(Vec<u8>),
    Reference(u64),
    List(Vec<StepValue>),
    TypedValue(String, Box<StepValue>),
}

// ============================================================================
// Parser Implementation
// ============================================================================

/// Parse a complete STEP file
pub fn parse_step(input: &str) -> IResult<&str, StepFile> {
    let (input, _) = tag("ISO-10303-21;")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, header) = parse_header(input)?;
    let (input, _) = multispace0(input)?;
    let (input, data) = parse_data(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("END-ISO-10303-21;")(input)?;
    
    Ok((input, StepFile { header, data }))
}

/// Parse header section
fn parse_header(input: &str) -> IResult<&str, StepHeader> {
    let (input, _) = tag("HEADER;")(input)?;
    let (input, _) = multispace0(input)?;
    
    // Parse FILE_DESCRIPTION
    let (input, _) = tag("FILE_DESCRIPTION")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = char('(')(input)?;
    let (input, file_description) = parse_string_list(input)?;
    let (input, _) = char(')')(input)?;
    let (input, _) = char(';')(input)?;
    let (input, _) = multispace0(input)?;
    
    // Parse FILE_NAME
    let (input, _) = tag("FILE_NAME")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = char('(')(input)?;
    let (input, file_name) = parse_step_string(input)?;
    let (input, _) = take_until(")")(input)?;
    let (input, _) = char(')')(input)?;
    let (input, _) = char(';')(input)?;
    let (input, _) = multispace0(input)?;
    
    // Parse FILE_SCHEMA
    let (input, _) = tag("FILE_SCHEMA")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = char('(')(input)?;
    let (input, file_schema) = parse_string_list(input)?;
    let (input, _) = char(')')(input)?;
    let (input, _) = char(';')(input)?;
    let (input, _) = multispace0(input)?;
    
    let (input, _) = tag("ENDSEC;")(input)?;
    
    Ok((input, StepHeader { file_description, file_name, file_schema }))
}

/// Parse data section
fn parse_data(input: &str) -> IResult<&str, Vec<StepEntity>> {
    let (input, _) = tag("DATA;")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, entities) = many0(terminated(parse_entity, multispace0))(input)?;
    let (input, _) = tag("ENDSEC;")(input)?;
    
    Ok((input, entities))
}

/// Parse a single entity
fn parse_entity(input: &str) -> IResult<&str, StepEntity> {
    let (input, _) = char('#')(input)?;
    let (input, id_str) = digit1(input)?;
    let id: u64 = id_str.parse().unwrap_or(0);
    let (input, _) = multispace0(input)?;
    let (input, _) = char('=')(input)?;
    let (input, _) = multispace0(input)?;
    let (input, type_name) = parse_identifier(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = char('(')(input)?;
    let (input, attributes) = separated_list0(
        tuple((multispace0, char(','), multispace0)),
        parse_value
    )(input)?;
    let (input, _) = char(')')(input)?;
    let (input, _) = char(';')(input)?;
    
    Ok((input, StepEntity { id, type_name: type_name.to_string(), attributes }))
}

/// Parse an identifier
fn parse_identifier(input: &str) -> IResult<&str, &str> {
    recognize(pair(
        alt((alpha1, tag("_"))),
        many0(alt((alphanumeric1, tag("_"))))
    ))(input)
}

/// Parse a STEP value
fn parse_value(input: &str) -> IResult<&str, StepValue> {
    let (input, _) = multispace0(input)?;
    alt((
        value(StepValue::Null, char('$')),
        value(StepValue::Missing, char('*')),
        map(parse_reference, StepValue::Reference),
        map(parse_step_string, StepValue::String),
        map(parse_enum, StepValue::Enum),
        map(parse_list, StepValue::List),
        map(parse_real, StepValue::Real),
        map(parse_integer, StepValue::Integer),
    ))(input)
}

/// Parse a reference #123
fn parse_reference(input: &str) -> IResult<&str, u64> {
    let (input, _) = char('#')(input)?;
    let (input, num) = digit1(input)?;
    Ok((input, num.parse().unwrap_or(0)))
}

/// Parse a STEP string 'text'
fn parse_step_string(input: &str) -> IResult<&str, String> {
    let (input, _) = char('\'')(input)?;
    let (input, content) = take_while(|c| c != '\'')(input)?;
    let (input, _) = char('\'')(input)?;
    Ok((input, content.to_string()))
}

/// Parse an enum .VALUE.
fn parse_enum(input: &str) -> IResult<&str, String> {
    let (input, _) = char('.')(input)?;
    let (input, value) = take_while1(|c: char| c.is_alphanumeric() || c == '_')(input)?;
    let (input, _) = char('.')(input)?;
    Ok((input, value.to_string()))
}

/// Parse a list (a, b, c)
fn parse_list(input: &str) -> IResult<&str, Vec<StepValue>> {
    let (input, _) = char('(')(input)?;
    let (input, items) = separated_list0(
        tuple((multispace0, char(','), multispace0)),
        parse_value
    )(input)?;
    let (input, _) = char(')')(input)?;
    Ok((input, items))
}

/// Parse a string list ('a', 'b')
fn parse_string_list(input: &str) -> IResult<&str, Vec<String>> {
    let (input, _) = char('(')(input)?;
    let (input, items) = separated_list0(
        tuple((multispace0, char(','), multispace0)),
        parse_step_string
    )(input)?;
    let (input, _) = char(')')(input)?;
    Ok((input, items))
}

/// Parse an integer
fn parse_integer(input: &str) -> IResult<&str, i64> {
    let (input, sign) = opt(char('-'))(input)?;
    let (input, num) = digit1(input)?;
    let value: i64 = num.parse().unwrap_or(0);
    Ok((input, if sign.is_some() { -value } else { value }))
}

/// Parse a real number
fn parse_real(input: &str) -> IResult<&str, f64> {
    let (input, sign) = opt(char('-'))(input)?;
    let (input, int_part) = digit1(input)?;
    let (input, _) = char('.')(input)?;
    let (input, frac_part) = digit1(input)?;
    let (input, exp) = opt(pair(
        alt((char('E'), char('e'))),
        pair(opt(alt((char('+'), char('-')))), digit1)
    ))(input)?;
    
    let mut s = String::new();
    if sign.is_some() { s.push('-'); }
    s.push_str(int_part);
    s.push('.');
    s.push_str(frac_part);
    if let Some((_, (exp_sign, exp_val))) = exp {
        s.push('E');
        if let Some(es) = exp_sign { s.push(es); }
        s.push_str(exp_val);
    }
    
    Ok((input, s.parse().unwrap_or(0.0)))
}

// ============================================================================
// B-Rep Extraction
// ============================================================================

/// Extract B-Rep geometry from STEP entities
pub struct BRepExtractor {
    entities: HashMap<u64, StepEntity>,
}

impl BRepExtractor {
    pub fn new(entities: Vec<StepEntity>) -> Self {
        let map: HashMap<u64, StepEntity> = entities.into_iter()
            .map(|e| (e.id, e))
            .collect();
        Self { entities: map }
    }

    /// Get all solid bodies
    pub fn get_solids(&self) -> Vec<&StepEntity> {
        self.entities.values()
            .filter(|e| e.type_name == "MANIFOLD_SOLID_BREP" || 
                       e.type_name == "BREP_WITH_VOIDS")
            .collect()
    }

    /// Get all faces for a solid
    pub fn get_faces(&self, solid_id: u64) -> Vec<&StepEntity> {
        // Get shell reference from solid
        if let Some(solid) = self.entities.get(&solid_id) {
            if let Some(StepValue::Reference(shell_id)) = solid.attributes.get(1) {
                if let Some(shell) = self.entities.get(shell_id) {
                    // Shell contains face list
                    if let Some(StepValue::List(faces)) = shell.attributes.get(0) {
                        return faces.iter()
                            .filter_map(|v| {
                                if let StepValue::Reference(id) = v {
                                    self.entities.get(id)
                                } else {
                                    None
                                }
                            })
                            .collect();
                    }
                }
            }
        }
        Vec::new()
    }

    /// Get surface for a face
    pub fn get_surface(&self, face_id: u64) -> Option<SurfaceInfo> {
        let face = self.entities.get(&face_id)?;
        
        // Face -> Face_bound -> Edge_loop -> Edges
        // Face -> Surface
        if let Some(StepValue::Reference(surface_id)) = face.attributes.get(1) {
            let surface = self.entities.get(surface_id)?;
            
            match surface.type_name.as_str() {
                "PLANE" => Some(SurfaceInfo::Plane),
                "CYLINDRICAL_SURFACE" => {
                    if let Some(StepValue::Real(radius)) = surface.attributes.get(1) {
                        Some(SurfaceInfo::Cylinder { radius: *radius })
                    } else {
                        None
                    }
                }
                "SPHERICAL_SURFACE" => {
                    if let Some(StepValue::Real(radius)) = surface.attributes.get(1) {
                        Some(SurfaceInfo::Sphere { radius: *radius })
                    } else {
                        None
                    }
                }
                "TOROIDAL_SURFACE" => {
                    if let (Some(StepValue::Real(major)), Some(StepValue::Real(minor))) = 
                        (surface.attributes.get(1), surface.attributes.get(2)) {
                        Some(SurfaceInfo::Torus { major_radius: *major, minor_radius: *minor })
                    } else {
                        None
                    }
                }
                _ => Some(SurfaceInfo::Other(surface.type_name.clone())),
            }
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub enum SurfaceInfo {
    Plane,
    Cylinder { radius: f64 },
    Sphere { radius: f64 },
    Torus { major_radius: f64, minor_radius: f64 },
    Other(String),
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    const SIMPLE_STEP: &str = r#"ISO-10303-21;
HEADER;
FILE_DESCRIPTION(('Test file'));
FILE_NAME('test.step');
FILE_SCHEMA(('AUTOMOTIVE_DESIGN'));
ENDSEC;
DATA;
#1=CARTESIAN_POINT('Origin',(0.0,0.0,0.0));
#2=DIRECTION('Z',(0.0,0.0,1.0));
#3=AXIS2_PLACEMENT_3D('',#1,#2,$);
ENDSEC;
END-ISO-10303-21;"#;

    #[test]
    fn test_parse_step() {
        let result = parse_step(SIMPLE_STEP);
        assert!(result.is_ok());
        
        let (_, file) = result.unwrap();
        assert_eq!(file.data.len(), 3);
        assert_eq!(file.data[0].type_name, "CARTESIAN_POINT");
    }

    #[test]
    fn test_parse_reference() {
        let (_, val) = parse_reference("#123").unwrap();
        assert_eq!(val, 123);
    }

    #[test]
    fn test_parse_string() {
        let (_, val) = parse_step_string("'Hello World'").unwrap();
        assert_eq!(val, "Hello World");
    }

    #[test]
    fn test_parse_real() {
        let (_, val) = parse_real("3.14159").unwrap();
        assert!((val - 3.14159).abs() < 0.00001);
    }
}

