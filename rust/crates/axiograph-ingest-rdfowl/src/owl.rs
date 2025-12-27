//! OWL/RDF Parsing Module
//!
//! Parses OWL ontologies and RDF graphs for knowledge graph ingestion.
//!
//! Note: this is a prototype parser. The repo-level plan is to enable
//! best-in-class RDF parsers (rio/oxrdf) behind a feature for Turtle/RDFXML,
//! while keeping the emitted artifacts *boundary-layer* (untrusted) until
//! promoted via the normal `.axi` + certificate gates.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

// ============================================================================
// OWL Ontology Types
// ============================================================================

/// Parsed OWL ontology
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Ontology {
    pub iri: String,
    pub version: Option<String>,
    pub imports: Vec<String>,
    pub classes: Vec<OwlClass>,
    pub properties: Vec<OwlProperty>,
    pub individuals: Vec<OwlIndividual>,
    pub axioms: Vec<OwlAxiom>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwlClass {
    pub iri: String,
    pub label: Option<String>,
    pub comment: Option<String>,
    pub subclass_of: Vec<String>,
    pub equivalent_to: Vec<String>,
    pub disjoint_with: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwlProperty {
    pub iri: String,
    pub label: Option<String>,
    pub property_type: PropertyType,
    pub domain: Vec<String>,
    pub range: Vec<String>,
    pub subproperty_of: Vec<String>,
    pub inverse_of: Option<String>,
    pub characteristics: Vec<PropertyCharacteristic>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PropertyType {
    ObjectProperty,
    DataProperty,
    AnnotationProperty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PropertyCharacteristic {
    Functional,
    InverseFunctional,
    Transitive,
    Symmetric,
    Asymmetric,
    Reflexive,
    Irreflexive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwlIndividual {
    pub iri: String,
    pub label: Option<String>,
    pub types: Vec<String>,
    pub properties: Vec<(String, PropertyValue)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PropertyValue {
    Individual(String),
    Literal(String, Option<String>), // value, datatype
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OwlAxiom {
    SubClassOf {
        sub: String,
        super_class: String,
    },
    EquivalentClasses {
        classes: Vec<String>,
    },
    DisjointClasses {
        classes: Vec<String>,
    },
    SubPropertyOf {
        sub: String,
        super_prop: String,
    },
    PropertyDomain {
        property: String,
        domain: String,
    },
    PropertyRange {
        property: String,
        range: String,
    },
    ClassAssertion {
        individual: String,
        class: String,
    },
    PropertyAssertion {
        subject: String,
        property: String,
        object: PropertyValue,
    },
}

// ============================================================================
// OWL Parser
// ============================================================================

pub struct OwlParser {
    base_iri: Option<String>,
}

impl Default for OwlParser {
    fn default() -> Self {
        Self { base_iri: None }
    }
}

impl OwlParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_base(mut self, iri: &str) -> Self {
        self.base_iri = Some(iri.to_string());
        self
    }

    /// Parse OWL from file (RDF/XML or Turtle)
    #[cfg(feature = "rdf")]
    pub fn parse_file(&self, path: &Path) -> Result<Ontology, OwlError> {
        use sophia::api::prelude::*;
        use sophia::turtle::parser::turtle;
        use sophia::xml::parser::RdfXmlParser;
        use std::fs::File;
        use std::io::BufReader;

        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");

        let file = File::open(path)?;
        let reader = BufReader::new(file);

        let triples: Vec<_> = match ext {
            "ttl" | "turtle" => {
                let parser = turtle::TurtleParser::new(reader, self.base_iri.clone());
                parser.collect_triples()?
            }
            "rdf" | "xml" | "owl" => {
                let parser = RdfXmlParser::new(reader, self.base_iri.clone());
                parser.collect_triples()?
            }
            _ => return Err(OwlError::UnsupportedFormat(ext.to_string())),
        };

        self.triples_to_ontology(triples)
    }

    /// Parse OWL from string
    #[cfg(feature = "rdf")]
    pub fn parse_string(&self, content: &str, format: &str) -> Result<Ontology, OwlError> {
        use sophia::api::prelude::*;
        use sophia::turtle::parser::turtle;

        let triples: Vec<_> = match format {
            "turtle" | "ttl" => {
                let parser = turtle::TurtleParser::new(content.as_bytes(), self.base_iri.clone());
                parser.collect_triples()?
            }
            _ => return Err(OwlError::UnsupportedFormat(format.to_string())),
        };

        self.triples_to_ontology(triples)
    }

    #[cfg(not(feature = "rdf"))]
    pub fn parse_file(&self, _path: &Path) -> Result<Ontology, OwlError> {
        Err(OwlError::FeatureNotEnabled)
    }

    #[cfg(not(feature = "rdf"))]
    pub fn parse_string(&self, _content: &str, _format: &str) -> Result<Ontology, OwlError> {
        Err(OwlError::FeatureNotEnabled)
    }

    /// Parse from simple N-Triples-like format (always available)
    pub fn parse_ntriples(&self, content: &str) -> Result<Ontology, OwlError> {
        let mut ontology = Ontology::default();
        let mut classes: HashMap<String, OwlClass> = HashMap::new();
        let mut properties: HashMap<String, OwlProperty> = HashMap::new();
        let mut individuals: HashMap<String, OwlIndividual> = HashMap::new();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Parse triple: <subject> <predicate> <object> .
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 4 {
                continue;
            }

            let subject = extract_iri(parts[0]);
            let predicate = extract_iri(parts[1]);
            let object = parts[2..parts.len() - 1].join(" ");
            let object = extract_iri(&object);

            // Process based on predicate
            match predicate.as_str() {
                "rdf:type" | "a" => match object.as_str() {
                    "owl:Class" => {
                        classes.entry(subject.clone()).or_insert_with(|| OwlClass {
                            iri: subject.clone(),
                            label: None,
                            comment: None,
                            subclass_of: Vec::new(),
                            equivalent_to: Vec::new(),
                            disjoint_with: Vec::new(),
                        });
                    }
                    "owl:ObjectProperty" => {
                        properties
                            .entry(subject.clone())
                            .or_insert_with(|| OwlProperty {
                                iri: subject.clone(),
                                label: None,
                                property_type: PropertyType::ObjectProperty,
                                domain: Vec::new(),
                                range: Vec::new(),
                                subproperty_of: Vec::new(),
                                inverse_of: None,
                                characteristics: Vec::new(),
                            });
                    }
                    "owl:DatatypeProperty" => {
                        properties
                            .entry(subject.clone())
                            .or_insert_with(|| OwlProperty {
                                iri: subject.clone(),
                                label: None,
                                property_type: PropertyType::DataProperty,
                                domain: Vec::new(),
                                range: Vec::new(),
                                subproperty_of: Vec::new(),
                                inverse_of: None,
                                characteristics: Vec::new(),
                            });
                    }
                    class_iri => {
                        let ind =
                            individuals
                                .entry(subject.clone())
                                .or_insert_with(|| OwlIndividual {
                                    iri: subject.clone(),
                                    label: None,
                                    types: Vec::new(),
                                    properties: Vec::new(),
                                });
                        ind.types.push(class_iri.to_string());
                    }
                },
                "rdfs:subClassOf" => {
                    let class = classes.entry(subject.clone()).or_insert_with(|| OwlClass {
                        iri: subject.clone(),
                        label: None,
                        comment: None,
                        subclass_of: Vec::new(),
                        equivalent_to: Vec::new(),
                        disjoint_with: Vec::new(),
                    });
                    class.subclass_of.push(object.clone());
                    ontology.axioms.push(OwlAxiom::SubClassOf {
                        sub: subject,
                        super_class: object,
                    });
                }
                "rdfs:domain" => {
                    if let Some(prop) = properties.get_mut(&subject) {
                        prop.domain.push(object.clone());
                    }
                    ontology.axioms.push(OwlAxiom::PropertyDomain {
                        property: subject,
                        domain: object,
                    });
                }
                "rdfs:range" => {
                    if let Some(prop) = properties.get_mut(&subject) {
                        prop.range.push(object.clone());
                    }
                    ontology.axioms.push(OwlAxiom::PropertyRange {
                        property: subject,
                        range: object,
                    });
                }
                "rdfs:label" => {
                    let label = extract_literal(&object);
                    if let Some(class) = classes.get_mut(&subject) {
                        class.label = Some(label.clone());
                    }
                    if let Some(prop) = properties.get_mut(&subject) {
                        prop.label = Some(label.clone());
                    }
                    if let Some(ind) = individuals.get_mut(&subject) {
                        ind.label = Some(label);
                    }
                }
                "rdfs:comment" => {
                    let comment = extract_literal(&object);
                    if let Some(class) = classes.get_mut(&subject) {
                        class.comment = Some(comment);
                    }
                }
                _ => {
                    // Property assertion
                    if let Some(ind) = individuals.get_mut(&subject) {
                        let value = if object.starts_with('<') || object.starts_with(':') {
                            PropertyValue::Individual(object.clone())
                        } else {
                            PropertyValue::Literal(extract_literal(&object), None)
                        };
                        ind.properties.push((predicate.clone(), value));
                    }
                }
            }
        }

        ontology.classes = classes.into_values().collect();
        ontology.properties = properties.into_values().collect();
        ontology.individuals = individuals.into_values().collect();

        Ok(ontology)
    }

    #[cfg(feature = "rdf")]
    fn triples_to_ontology<T>(&self, _triples: Vec<T>) -> Result<Ontology, OwlError> {
        // Full implementation would process sophia triples
        Ok(Ontology::default())
    }
}

fn extract_iri(s: &str) -> String {
    s.trim_start_matches('<').trim_end_matches('>').to_string()
}

fn extract_literal(s: &str) -> String {
    let s = s.trim_matches('"');
    // Remove language tag or datatype
    if let Some(pos) = s.find("@") {
        s[..pos].to_string()
    } else if let Some(pos) = s.find("^^") {
        s[..pos].to_string()
    } else {
        s.to_string()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OwlError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),
    #[error("RDF feature not enabled. Compile with --features rdf")]
    FeatureNotEnabled,
}

// ============================================================================
// Convert to Axiograph
// ============================================================================

/// Convert OWL ontology to Axiograph schema definition
pub fn ontology_to_axi(ontology: &Ontology) -> String {
    let mut output = String::new();

    output.push_str(&format!(
        "module Ontology_{}\n\n",
        sanitize_name(&ontology.iri)
    ));

    // Classes as objects
    output.push_str("schema OntologySchema {\n");
    output.push_str("  objects {\n");
    for class in &ontology.classes {
        let name = local_name(&class.iri);
        let label = class.label.as_deref().unwrap_or(&name);
        output.push_str(&format!("    {} -- \"{}\"\n", name, label));
    }
    output.push_str("  }\n\n");

    // Properties as morphisms
    output.push_str("  morphisms {\n");
    for prop in &ontology.properties {
        let name = local_name(&prop.iri);
        for domain in &prop.domain {
            for range in &prop.range {
                output.push_str(&format!(
                    "    {} : {} -> {}\n",
                    name,
                    local_name(domain),
                    local_name(range)
                ));
            }
        }
    }
    output.push_str("  }\n\n");

    // SubClass as equations
    output.push_str("  equations {\n");
    for axiom in &ontology.axioms {
        if let OwlAxiom::SubClassOf { sub, super_class } = axiom {
            output.push_str(&format!(
                "    -- {} is subclass of {}\n",
                local_name(sub),
                local_name(super_class)
            ));
        }
    }
    output.push_str("  }\n");
    output.push_str("}\n\n");

    // Individuals as instance data
    if !ontology.individuals.is_empty() {
        output.push_str("instance OntologyInstance : OntologySchema {\n");
        for ind in &ontology.individuals {
            let name = local_name(&ind.iri);
            for type_iri in &ind.types {
                output.push_str(&format!("  {} : {}\n", name, local_name(type_iri)));
            }
        }
        output.push_str("}\n");
    }

    output
}

fn sanitize_name(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .collect()
}

fn local_name(iri: &str) -> String {
    iri.rsplit(&['/', '#'][..])
        .next()
        .unwrap_or(iri)
        .to_string()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_NTRIPLES: &str = r#"
<http://example.org/Material> rdf:type owl:Class .
<http://example.org/Metal> rdf:type owl:Class .
<http://example.org/Metal> rdfs:subClassOf <http://example.org/Material> .
<http://example.org/hasDensity> rdf:type owl:DatatypeProperty .
<http://example.org/hasDensity> rdfs:domain <http://example.org/Material> .
<http://example.org/Steel> rdf:type <http://example.org/Metal> .
<http://example.org/Steel> rdfs:label "Steel" .
"#;

    #[test]
    fn test_parse_ntriples() {
        let parser = OwlParser::new();
        let ontology = parser.parse_ntriples(SAMPLE_NTRIPLES).unwrap();

        assert_eq!(ontology.classes.len(), 2);
        assert!(ontology.classes.iter().any(|c| c.iri.ends_with("Material")));

        assert_eq!(ontology.properties.len(), 1);

        assert_eq!(ontology.individuals.len(), 1);
        assert_eq!(ontology.individuals[0].label, Some("Steel".to_string()));
    }

    #[test]
    fn test_to_axi() {
        let parser = OwlParser::new();
        let ontology = parser.parse_ntriples(SAMPLE_NTRIPLES).unwrap();

        let axi = ontology_to_axi(&ontology);
        assert!(axi.contains("Material"));
        assert!(axi.contains("Metal"));
        assert!(axi.contains("subclass"));
    }
}
