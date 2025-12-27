use axiograph_pathdb::axi_export::export_pathdb_to_axi_v1;
use axiograph_pathdb::PathDB;

fn main() {
    // Minimal PathDB instance that is rich enough to exercise the export schema:
    // - entities with attributes
    // - a relation with confidence + attributes
    let mut db = PathDB::new();

    let titanium = db.add_entity("Material", vec![("name", "Titanium"), ("hardness", "36")]);
    let end_mill = db.add_entity("Tool", vec![("name", "EndMill")]);
    db.add_relation(
        "usedWith",
        end_mill,
        titanium,
        0.9,
        vec![("source", "emit_pathdb_export_axi_v1")],
    );

    db.build_indexes();

    let axi = export_pathdb_to_axi_v1(&db).expect("export PathDB to .axi (PathDBExportV1)");
    print!("{axi}");
}
