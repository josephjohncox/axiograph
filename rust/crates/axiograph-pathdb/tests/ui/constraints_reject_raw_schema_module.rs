use axiograph_pathdb::axi_module_constraints::check_axi_constraints_ok_v1;
use axiograph_dsl::schema_v1::SchemaV1Module;

fn bogus_module() -> SchemaV1Module {
    unimplemented!()
}

fn main() {
    let module = bogus_module();
    let _ = check_axi_constraints_ok_v1(&module);
}
