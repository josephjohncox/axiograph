use axiograph_dsl::schema_v1::SchemaV1Module;
use axiograph_pathdb::{review_axi_v1_module, ReviewStamp};

fn bogus_module() -> SchemaV1Module {
    unimplemented!()
}

fn main() {
    let raw = bogus_module();
    let _ = review_axi_v1_module(
        raw,
        ReviewStamp {
            reviewer: None,
            note: None,
        },
    );
}
