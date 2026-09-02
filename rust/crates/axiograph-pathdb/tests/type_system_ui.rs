#[test]
fn lifecycle_and_anchor_misuse_fail_to_compile() {
    // The checked-in diagnostics are intentionally pinned to the release/MSRV
    // compiler selected by `make check-rust-toolchain` (Rust 1.98.0). Re-bless
    // them only with that exact toolchain so newer diagnostic rendering cannot
    // silently redefine the release contract.
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
