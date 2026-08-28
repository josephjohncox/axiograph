use axiograph_dsl::schema_v1::SchemaV1Module;
use axiograph_pathdb::{review_axi_v1_module, validate_axi_v1_module, ReviewStamp};

fn validated_module() -> axiograph_pathdb::Module<axiograph_pathdb::Validated> {
    let module = SchemaV1Module {
        module_name: "Demo".to_string(),
        imports: Vec::new(),
        schemas: Vec::new(),
        theories: Vec::new(),
        instances: Vec::new(),
    };
    validate_axi_v1_module(module).expect("validated module")
}

fn main() {
    let reviewed = review_axi_v1_module(
        validated_module(),
        ReviewStamp {
            reviewer: None,
            note: None,
        },
    );
    let _ = review_axi_v1_module(
        reviewed,
        ReviewStamp {
            reviewer: None,
            note: None,
        },
    );
}
