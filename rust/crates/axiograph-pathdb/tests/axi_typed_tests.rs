use axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb;
use axiograph_pathdb::axi_typed::{AxiTypingContext, AxiTypingError};
use axiograph_pathdb::DbTokenMismatch;
use axiograph_pathdb::PathDB;

fn entity_by_name(db: &PathDB, name: &str) -> u32 {
    let name_key = db
        .interner
        .id_of("name")
        .unwrap_or_else(|| panic!("interner missing `name` key"));
    let name_value = db
        .interner
        .id_of(name)
        .unwrap_or_else(|| panic!("interner missing `{name}`"));
    let ids = db.entities.entities_with_attr_value(name_key, name_value);
    assert_eq!(ids.len(), 1, "expected exactly one entity named `{name}`");
    ids.iter().next().expect("non-empty").into()
}

#[test]
fn find_by_axi_type_scopes_by_schema() {
    let mut db = PathDB::new();

    let mod1 = r#"
module Mod1

schema S1:
  object Text

instance I1 of S1:
  Text = { TextA }
"#;

    let mod2 = r#"
module Mod2

schema S2:
  object Text

instance I2 of S2:
  Text = { TextB }
"#;

    import_axi_schema_v1_into_pathdb(&mut db, mod1).expect("import mod1");
    import_axi_schema_v1_into_pathdb(&mut db, mod2).expect("import mod2");

    let all_text = db.find_by_type("Text").cloned().unwrap_or_default();
    assert_eq!(
        all_text.len(),
        2,
        "expected both TextA and TextB under `Text`"
    );

    let s1_text = db.find_by_axi_type("S1", "Text");
    let s2_text = db.find_by_axi_type("S2", "Text");

    assert_eq!(s1_text.len(), 1);
    assert_eq!(s2_text.len(), 1);
    assert_ne!(s1_text, s2_text, "schema scoping should separate Text sets");
}

#[test]
fn typed_entity_is_schema_scoped_and_checks_subtyping() {
    let mut db = PathDB::new();

    let text = r#"
module Mod

schema S:
  object Super
  object Sub
  subtype Sub < Super

instance I of S:
  Sub = { x }
"#;

    import_axi_schema_v1_into_pathdb(&mut db, text).expect("import module");

    let x = entity_by_name(&db, "x");

    let ctx = AxiTypingContext::from_db(&db).expect("build typing context");
    let schema = ctx.schema("S").expect("schema exists");

    // Subtype check: x : Sub, so x : Super should be allowed.
    let typed = schema.typed_entity(&db, x, "Super").expect("x : Super");
    assert_eq!(typed.entity_id(&db), Ok(x));
    assert_eq!(typed.schema, "S");
    assert_eq!(typed.expected_type, "Super");

    // Reject wrong schema.
    let err = ctx.schema("Other").unwrap_err();
    assert_eq!(
        err,
        AxiTypingError::UnknownSchema {
            schema: "Other".to_string()
        }
    );

    // Reject wrong type.
    let bad = schema.typed_entity(&db, x, "DoesNotExist").unwrap_err();
    match bad {
        AxiTypingError::TypeMismatch { entity, .. } => assert_eq!(entity, x),
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn typed_values_cannot_cross_pathdb_instances() {
    let text = r#"
module Mod

schema S:
  object Text

instance I of S:
  Text = { x }
"#;

    let mut db1 = PathDB::new();
    let mut db2 = PathDB::new();
    import_axi_schema_v1_into_pathdb(&mut db1, text).expect("import into db1");
    import_axi_schema_v1_into_pathdb(&mut db2, text).expect("import into db2");

    let x1 = entity_by_name(&db1, "x");

    let ctx1 = AxiTypingContext::from_db(&db1).expect("build typing context");
    let schema1 = ctx1.schema("S").expect("schema exists");
    let typed_x1 = schema1.typed_entity(&db1, x1, "Text").expect("x : Text");

    assert_eq!(typed_x1.entity_id(&db1), Ok(x1));

    let err = typed_x1.entity_id(&db2).expect_err("db token mismatch");
    assert!(
        matches!(err, DbTokenMismatch { .. }),
        "unexpected error: {err:?}"
    );
}
