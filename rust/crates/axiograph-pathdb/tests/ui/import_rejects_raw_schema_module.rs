use axiograph_dsl::schema_v1::SchemaV1Module;
use axiograph_pathdb::axi_module_import::import_axi_schema_v1_module_into_pathdb;
use axiograph_pathdb::PathDB;

fn bogus_module() -> SchemaV1Module {
    unimplemented!()
}

fn main() {
    let mut db = PathDB::new();
    let module = bogus_module();
    let _ = import_axi_schema_v1_module_into_pathdb(&mut db, &module);
}
