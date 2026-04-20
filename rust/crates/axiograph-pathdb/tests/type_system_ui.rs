#[test]
fn lifecycle_and_anchor_misuse_fail_to_compile() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
