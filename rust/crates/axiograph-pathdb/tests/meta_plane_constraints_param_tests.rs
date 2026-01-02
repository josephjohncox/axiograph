use axiograph_pathdb::axi_semantics::{ConstraintDecl, MetaPlaneIndex};
use axiograph_pathdb::PathDB;

#[test]
fn meta_plane_roundtrips_param_fields_for_closure_constraints() {
    let text = r#"
module MetaPlaneParamRoundtrip

schema S:
  object World
  object Context
  object Time
  object Evidence
  relation Accessible(from: World, to: World, ctx: Context, time: Time, witness: Evidence)

theory Rules on S:
  constraint symmetric Accessible on (from, to) param (ctx, time)
  constraint transitive Accessible on (from, to) param (ctx, time)
  constraint key Accessible(from, to, ctx, time)

instance Demo of S:
  World = {A, B}
  Context = {C0}
  Time = {T0}
  Evidence = {E0}
  Accessible = {(from=A, to=B, ctx=C0, time=T0, witness=E0)}
"#;

    let mut db = PathDB::new();
    let module = axiograph_dsl::axi_v1::parse_axi_v1(text).expect("parse axi");
    axiograph_pathdb::axi_module_import::import_axi_schema_v1_module_into_pathdb(&mut db, &module)
        .expect("import module");
    db.build_indexes();

    let meta = MetaPlaneIndex::from_db(&db).expect("meta-plane index");
    let schema = meta.schemas.get("S").expect("schema S");
    let decls = schema
        .constraints_by_relation
        .get("Accessible")
        .expect("constraints for Accessible");

    assert!(
        decls.iter().any(|d| matches!(d,
            ConstraintDecl::Symmetric { params: Some(p), .. } if p == &vec!["ctx".to_string(), "time".to_string()]
        )),
        "expected Symmetric params in meta-plane, got {decls:?}"
    );
    assert!(
        decls.iter().any(|d| matches!(d,
            ConstraintDecl::Transitive { params: Some(p), .. } if p == &vec!["ctx".to_string(), "time".to_string()]
        )),
        "expected Transitive params in meta-plane, got {decls:?}"
    );

    let exported =
        axiograph_pathdb::axi_module_export::export_axi_schema_v1_module_from_pathdb(&db, "MetaPlaneParamRoundtrip")
            .expect("export");
    assert!(
        exported.contains("constraint symmetric Accessible on (from, to) param (ctx, time)"),
        "export missing symmetric param clause:\n{exported}"
    );
    assert!(
        exported.contains("constraint transitive Accessible on (from, to) param (ctx, time)"),
        "export missing transitive param clause:\n{exported}"
    );
}

