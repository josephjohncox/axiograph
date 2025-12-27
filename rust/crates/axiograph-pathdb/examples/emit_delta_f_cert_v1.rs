use axiograph_pathdb::{
    ArrowDeclV1, ArrowMapV1, ArrowMappingV1, InstanceV1, ObjectElementsV1, ObjectMappingV1,
    ProofProducingOptimizer, SchemaMorphismV1, SchemaV1, WithProof,
};

fn main() {
    // Minimal example: Δ_F along a functor that maps a single arrow to a single arrow.
    //
    // S1: A --f--> B
    // S2: X --g--> Y
    //
    // F : S1 → S2  maps A↦X, B↦Y, f↦g
    //
    // Target instance I2 on S2 provides:
    //   X = {x1, x2}
    //   Y = {y1, y2}
    //   g(x1)=y1, g(x2)=y2
    //
    // Δ_F(I2) is an instance on S1 with:
    //   A = {x1, x2}
    //   B = {y1, y2}
    //   f(x1)=y1, f(x2)=y2
    let source_schema = SchemaV1 {
        name: "S1".to_string(),
        objects: vec!["A".to_string(), "B".to_string()],
        arrows: vec![ArrowDeclV1 {
            name: "f".to_string(),
            src: "A".to_string(),
            dst: "B".to_string(),
        }],
        subtypes: vec![],
    };

    let target_instance = InstanceV1 {
        name: "I2".to_string(),
        schema: "S2".to_string(),
        objects: vec![
            ObjectElementsV1 {
                obj: "X".to_string(),
                elems: vec!["x1".to_string(), "x2".to_string()],
            },
            ObjectElementsV1 {
                obj: "Y".to_string(),
                elems: vec!["y1".to_string(), "y2".to_string()],
            },
        ],
        arrows: vec![ArrowMapV1 {
            arrow: "g".to_string(),
            pairs: vec![
                ("x1".to_string(), "y1".to_string()),
                ("x2".to_string(), "y2".to_string()),
            ],
        }],
    };

    let morphism = SchemaMorphismV1 {
        source_schema: "S1".to_string(),
        target_schema: "S2".to_string(),
        objects: vec![
            ObjectMappingV1 {
                source_object: "A".to_string(),
                target_object: "X".to_string(),
            },
            ObjectMappingV1 {
                source_object: "B".to_string(),
                target_object: "Y".to_string(),
            },
        ],
        arrows: vec![ArrowMappingV1 {
            source_arrow: "f".to_string(),
            target_path: vec!["g".to_string()],
        }],
    };

    let optimizer = ProofProducingOptimizer::default();
    let proved = optimizer
        .delta_f_certificate_v1::<WithProof>(morphism, source_schema, target_instance)
        .expect("delta_f certificate should be emitted");

    let cert = proved.proof;
    println!(
        "{}",
        serde_json::to_string_pretty(&cert).expect("serialize delta_f certificate")
    );
}
