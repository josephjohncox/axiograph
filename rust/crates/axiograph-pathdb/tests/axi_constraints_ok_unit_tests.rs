use axiograph_pathdb::axi_module_constraints::check_axi_constraints_ok_v1;

#[test]
fn symmetric_param_detects_functional_violation_introduced_by_swap() {
    // The original tuples satisfy the unary functional dependency:
    //   Spouse.a -> Spouse.b
    //
    // But once we interpret Spouse as symmetric within each ctx fiber, the swapped
    // tuple can introduce a new mapping for the same `a`, violating the FD.
    let text = r#"
module SymmetricParamFunctionalViolation

schema S:
  object Person
  object Context
  relation Spouse(a: Person, b: Person, ctx: Context)

theory Rules on S:
  constraint symmetric Spouse param (ctx)
  constraint functional Spouse.a -> Spouse.b

instance Demo of S:
  Person = {Alice, Bob, Carol}
  Context = {C0}
  Spouse = {
    (a=Alice, b=Bob, ctx=C0),
    (a=Bob, b=Carol, ctx=C0)
  }
"#;

    let module = axiograph_dsl::axi_v1::parse_axi_v1(text).expect("parse axi");
    let err = check_axi_constraints_ok_v1(&module).expect_err("should fail");
    let msg = err.to_string();
    assert!(
        msg.contains("functional violation") && msg.contains("Spouse") && msg.contains("a ->"),
        "unexpected error: {msg}"
    );
}

#[test]
fn symmetric_param_rejects_duplicate_param_fields() {
    let text = r#"
module SymmetricParamDuplicate

schema S:
  object Person
  object Context
  relation Spouse(a: Person, b: Person, ctx: Context)

theory Rules on S:
  constraint symmetric Spouse param (ctx, ctx)

instance Demo of S:
  Person = {Alice, Bob}
  Context = {C0}
  Spouse = {(a=Alice, b=Bob, ctx=C0)}
"#;

    let module = axiograph_dsl::axi_v1::parse_axi_v1(text).expect("parse axi");
    let err = check_axi_constraints_ok_v1(&module).expect_err("should fail");
    assert!(err.to_string().contains("duplicate param field"), "err={err}");
}

#[test]
fn symmetric_param_rejects_param_field_that_is_a_carrier() {
    let text = r#"
module SymmetricParamCarrier

schema S:
  object Person
  relation Spouse(a: Person, b: Person)

theory Rules on S:
  constraint symmetric Spouse param (a)

instance Demo of S:
  Person = {Alice, Bob}
  Spouse = {(a=Alice, b=Bob)}
"#;

    let module = axiograph_dsl::axi_v1::parse_axi_v1(text).expect("parse axi");
    let err = check_axi_constraints_ok_v1(&module).expect_err("should fail");
    assert!(
        err.to_string().contains("must not be a carrier field"),
        "err={err}"
    );
}

#[test]
fn symmetric_param_rejects_unknown_param_field() {
    let text = r#"
module SymmetricParamUnknownField

schema S:
  object Person
  relation Spouse(a: Person, b: Person)

theory Rules on S:
  constraint symmetric Spouse param (ctx)

instance Demo of S:
  Person = {Alice, Bob}
  Spouse = {(a=Alice, b=Bob)}
"#;

    let module = axiograph_dsl::axi_v1::parse_axi_v1(text).expect("parse axi");
    let err = check_axi_constraints_ok_v1(&module).expect_err("should fail");
    assert!(
        err.to_string().contains("param field `ctx`") && err.to_string().contains("not a declared field"),
        "err={err}"
    );
}

#[test]
fn transitive_param_detects_key_violation_introduced_by_closure() {
    // Key says: for each (from, ctx), there is at most one `to`.
    // The explicit tuples satisfy it, but transitive closure introduces (A,C)
    // in the same ctx fiber, violating the key.
    let text = r#"
module TransitiveParamKeyViolation

schema S:
  object World
  object Context
  relation Accessible(from: World, to: World, ctx: Context)

theory Rules on S:
  constraint transitive Accessible param (ctx)
  constraint key Accessible(from, ctx)

instance Demo of S:
  World = {A, B, C}
  Context = {C0}
  Accessible = {
    (from=A, to=B, ctx=C0),
    (from=B, to=C, ctx=C0)
  }
"#;

    let module = axiograph_dsl::axi_v1::parse_axi_v1(text).expect("parse axi");
    let err = check_axi_constraints_ok_v1(&module).expect_err("should fail");
    assert!(err.to_string().contains("key violation"), "err={err}");
}

#[test]
fn transitive_param_does_not_mix_context_fibers() {
    // Same key as above, but the two edges live in different ctx fibers,
    // so the fibered closure does NOT infer (A,C) and the key remains OK.
    let text = r#"
module TransitiveParamNoFiberMixing

schema S:
  object World
  object Context
  relation Accessible(from: World, to: World, ctx: Context)

theory Rules on S:
  constraint transitive Accessible param (ctx)
  constraint key Accessible(from, ctx)

instance Demo of S:
  World = {A, B, C}
  Context = {C0, C1}
  Accessible = {
    (from=A, to=B, ctx=C0),
    (from=B, to=C, ctx=C1)
  }
"#;

    let module = axiograph_dsl::axi_v1::parse_axi_v1(text).expect("parse axi");
    check_axi_constraints_ok_v1(&module).expect("should pass");
}

#[test]
fn transitive_param_rejects_duplicate_param_fields() {
    let text = r#"
module TransitiveParamDuplicate

schema S:
  object World
  object Context
  relation Accessible(from: World, to: World, ctx: Context)

theory Rules on S:
  constraint transitive Accessible param (ctx, ctx)

instance Demo of S:
  World = {A, B}
  Context = {C0}
  Accessible = {(from=A, to=B, ctx=C0)}
"#;

    let module = axiograph_dsl::axi_v1::parse_axi_v1(text).expect("parse axi");
    let err = check_axi_constraints_ok_v1(&module).expect_err("should fail");
    assert!(err.to_string().contains("duplicate param field"), "err={err}");
}

#[test]
fn transitive_param_rejects_param_field_that_is_a_carrier() {
    let text = r#"
module TransitiveParamCarrier

schema S:
  object World
  relation Accessible(from: World, to: World)

theory Rules on S:
  constraint transitive Accessible param (to)

instance Demo of S:
  World = {A, B}
  Accessible = {(from=A, to=B)}
"#;

    let module = axiograph_dsl::axi_v1::parse_axi_v1(text).expect("parse axi");
    let err = check_axi_constraints_ok_v1(&module).expect_err("should fail");
    assert!(
        err.to_string().contains("must not be a carrier field"),
        "err={err}"
    );
}

#[test]
fn transitive_param_rejects_unknown_param_field() {
    let text = r#"
module TransitiveParamUnknownField

schema S:
  object World
  relation Accessible(from: World, to: World)

theory Rules on S:
  constraint transitive Accessible param (ctx)

instance Demo of S:
  World = {A, B}
  Accessible = {(from=A, to=B)}
"#;

    let module = axiograph_dsl::axi_v1::parse_axi_v1(text).expect("parse axi");
    let err = check_axi_constraints_ok_v1(&module).expect_err("should fail");
    assert!(
        err.to_string().contains("param field `ctx`") && err.to_string().contains("not a declared field"),
        "err={err}"
    );
}

#[test]
fn constraints_ok_fails_closed_on_unknown_constraints() {
    let text = r#"
module UnknownConstraintFailsClosed

schema S:
  object A
  object B
  relation R(from: A, to: B)

theory Rules on S:
  constraint this is not a known constraint form

instance Demo of S:
  A = {a0}
  B = {b0}
  R = {(from=a0, to=b0)}
"#;

    let module = axiograph_dsl::axi_v1::parse_axi_v1(text).expect("parse axi");
    let err = check_axi_constraints_ok_v1(&module).expect_err("should fail");
    assert!(
        err.to_string().contains("refused") && err.to_string().contains("unknown/unsupported"),
        "err={err}"
    );
}

#[test]
fn named_block_constraints_do_not_block_constraints_ok_v1() {
    let text = r#"
module NamedBlockDoesNotBlock

schema S:
  object A
  object B
  relation R(from: A, to: B)

theory Rules on S:
  constraint FutureRule:
    This is a human-authored block that is preserved.
    It is not yet executable/certifiable.

instance Demo of S:
  A = {a0}
  B = {b0}
  R = {(from=a0, to=b0)}
"#;

    let module = axiograph_dsl::axi_v1::parse_axi_v1(text).expect("parse axi");
    check_axi_constraints_ok_v1(&module).expect("should pass");
}
