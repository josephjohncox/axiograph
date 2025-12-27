use axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb;
use axiograph_pathdb::learning::{
    extract_learning_graph, REL_CONCEPT_DESCRIPTION, REL_DEMONSTRATES, REL_EXPLAINS, REL_REQUIRES,
    TYPE_CONCEPT,
};
use axiograph_pathdb::PathDB;

#[test]
fn extracts_learning_graph_edges_from_axi_instance() {
    let mut db = PathDB::new();

    let text = r#"
module LearnMod

schema Learn:
  object Concept
  object SafetyGuideline
  object Example
  object Text

  relation requires(concept: Concept, prereq: Concept)
  relation explains(concept: Concept, guideline: SafetyGuideline)
  relation demonstrates(example: Example, concept: Concept)
  relation conceptDescription(concept: Concept, text: Text)

instance LearnInst of Learn:
  Concept = { C1, C2 }
  SafetyGuideline = { G1 }
  Example = { E1 }
  Text = { T1 }

  requires = { (concept=C2, prereq=C1) }
  explains = { (concept=C2, guideline=G1) }
  demonstrates = { (example=E1, concept=C2) }
  conceptDescription = { (concept=C2, text=T1) }
"#;

    import_axi_schema_v1_into_pathdb(&mut db, text).expect("import module");

    let g = extract_learning_graph(&db, "Learn").expect("extract learning graph");

    assert_eq!(g.schema, "Learn");
    assert_eq!(g.concepts.len(), 2);
    assert!(g.concepts.iter().all(|c| c.expected_type == TYPE_CONCEPT));

    assert_eq!(g.requires.len(), 1);
    assert_eq!(g.explains.len(), 1);
    assert_eq!(g.demonstrates.len(), 1);
    assert_eq!(g.concept_descriptions.len(), 1);

    assert_eq!(g.requires[0].rel_type, REL_REQUIRES);
    assert_eq!(g.explains[0].rel_type, REL_EXPLAINS);
    assert_eq!(g.demonstrates[0].rel_type, REL_DEMONSTRATES);
    assert_eq!(g.concept_descriptions[0].rel_type, REL_CONCEPT_DESCRIPTION);

    // Edges should be “real PathDB edges” (relation ids exist) because the `.axi`
    // importer emits derived binary edges for binary relations.
    assert!(g.requires[0].relation_id.is_some());
    assert!(g.explains[0].relation_id.is_some());
    assert!(g.demonstrates[0].relation_id.is_some());
    assert!(g.concept_descriptions[0].relation_id.is_some());
}
