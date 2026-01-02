//! AST-level checking for a conservative subset of canonical `.axi` theory constraints.
//!
//! This is the Rust-side implementation for the `axi_constraints_ok_v1` certificate kind.
//!
//! Scope (initial release)
//! -----------------------
//! We intentionally start with constraints that are:
//! - easy to explain,
//! - common in schema-directed optimization (keys/functionals),
//! - and low ambiguity across dialects.
//!
//! Supported constraint kinds:
//! - `constraint key Rel(field, ...)`
//! - `constraint functional Rel.field -> Rel.field`
//! - `constraint symmetric Rel`
//! - `constraint symmetric Rel where Rel.field in {A, B, ...}`
//! - `constraint transitive Rel` (closure-compatibility for keys/functionals on carrier fields)
//! - `constraint typing Rel: rule_name` (small builtin rule set)
//!
//! Not yet certified:
//! - global entailment / inference,
//! - relational algebra beyond simple uniqueness checks.

use std::collections::HashMap;

use anyhow::{anyhow, Result};

use axiograph_dsl::schema_v1::{ConstraintV1, SchemaV1Instance, SchemaV1Module, SetItemV1};

use crate::certificate::AxiConstraintsOkProofV1;

#[derive(Debug, Clone)]
enum CoreConstraint<'a> {
    Key {
        schema: &'a str,
        relation: &'a str,
        fields: &'a [String],
    },
    Functional {
        schema: &'a str,
        relation: &'a str,
        src_field: &'a str,
        dst_field: &'a str,
    },
    Symmetric {
        schema: &'a str,
        relation: &'a str,
    },
    SymmetricWhereIn {
        schema: &'a str,
        relation: &'a str,
        field: &'a str,
        values: &'a [String],
    },
    Transitive {
        schema: &'a str,
        relation: &'a str,
    },
    Typing {
        schema: &'a str,
        relation: &'a str,
        rule: &'a str,
    },
}

fn gather_core_constraints(module: &SchemaV1Module) -> Vec<CoreConstraint<'_>> {
    let mut out: Vec<CoreConstraint<'_>> = Vec::new();
    for th in &module.theories {
        for c in &th.constraints {
            match c {
                ConstraintV1::Key { relation, fields } => out.push(CoreConstraint::Key {
                    schema: &th.schema,
                    relation,
                    fields,
                }),
                ConstraintV1::Functional {
                    relation,
                    src_field,
                    dst_field,
                } => out.push(CoreConstraint::Functional {
                    schema: &th.schema,
                    relation,
                    src_field,
                    dst_field,
                }),
                ConstraintV1::Symmetric { relation } => out.push(CoreConstraint::Symmetric {
                    schema: &th.schema,
                    relation,
                }),
                ConstraintV1::SymmetricWhereIn {
                    relation,
                    field,
                    values,
                } => out.push(CoreConstraint::SymmetricWhereIn {
                    schema: &th.schema,
                    relation,
                    field,
                    values,
                }),
                ConstraintV1::Transitive { relation } => out.push(CoreConstraint::Transitive {
                    schema: &th.schema,
                    relation,
                }),
                ConstraintV1::Typing { relation, rule } => out.push(CoreConstraint::Typing {
                    schema: &th.schema,
                    relation,
                    rule,
                }),
                _ => {}
            }
        }
    }
    out
}

#[derive(Debug, Clone, Default)]
struct RelationFieldIndex {
    /// Map `schema_name -> relation_name -> ordered field names`.
    fields_by_schema_relation: HashMap<String, HashMap<String, Vec<String>>>,
}

impl RelationFieldIndex {
    fn from_module(module: &SchemaV1Module) -> Self {
        let mut fields_by_schema_relation: HashMap<String, HashMap<String, Vec<String>>> =
            HashMap::new();
        for s in &module.schemas {
            let rels = fields_by_schema_relation.entry(s.name.clone()).or_default();
            for r in &s.relations {
                let field_names = r.fields.iter().map(|f| f.field.clone()).collect::<Vec<_>>();
                rels.insert(r.name.clone(), field_names);
            }
        }
        Self {
            fields_by_schema_relation,
        }
    }

    fn relation_fields(&self, schema: &str, relation: &str) -> Result<&[String]> {
        let Some(rels) = self.fields_by_schema_relation.get(schema) else {
            return Err(anyhow!("unknown schema `{schema}`"));
        };
        let Some(fields) = rels.get(relation) else {
            return Err(anyhow!("unknown relation `{relation}` in schema `{schema}`"));
        };
        Ok(fields.as_slice())
    }
}

fn relation_tuples<'a>(
    inst: &'a SchemaV1Instance,
    relation_name: &'a str,
) -> impl Iterator<Item = &'a Vec<(String, String)>> + 'a {
    inst.assignments
        .iter()
        .filter(move |a| a.name == relation_name)
        .flat_map(|a| a.value.items.iter())
        .filter_map(|it| match it {
            SetItemV1::Tuple { fields } => Some(fields),
            _ => None,
        })
}

fn tuple_values_in_order(
    inst_name: &str,
    relation_name: &str,
    tuple: &[(String, String)],
    ordered_fields: &[String],
) -> Result<Vec<String>> {
    let mut map: HashMap<&str, &str> = HashMap::new();
    for (k, v) in tuple {
        if map.contains_key(k.as_str()) {
            return Err(anyhow!(
                "instance `{inst_name}` relation `{relation_name}`: duplicate field `{k}` in tuple",
            ));
        }
        map.insert(k.as_str(), v.as_str());
    }

    let mut out: Vec<String> = Vec::with_capacity(ordered_fields.len());
    for f in ordered_fields {
        let Some(v) = map.get(f.as_str()) else {
            return Err(anyhow!(
                "instance `{inst_name}` relation `{relation_name}`: missing field `{f}` in tuple"
            ));
        };
        out.push((*v).to_string());
    }
    Ok(out)
}

fn check_key_on_tuples(
    inst_name: &str,
    relation_name: &str,
    relation_fields: &[String],
    tuples: impl Iterator<Item = Vec<String>>,
    key_fields: &[String],
) -> Result<()> {
    if key_fields.is_empty() {
        return Ok(());
    }

    let mut key_idxs: Vec<usize> = Vec::with_capacity(key_fields.len());
    for f in key_fields {
        let Some(idx) = relation_fields.iter().position(|x| x == f) else {
            return Err(anyhow!(
                "instance `{inst_name}` relation `{relation_name}`: key field `{f}` is not a declared field",
            ));
        };
        key_idxs.push(idx);
    }

    let mut seen: HashMap<Vec<String>, usize> = HashMap::new();
    for (i, tuple) in tuples.enumerate() {
        let mut key: Vec<String> = Vec::with_capacity(key_idxs.len());
        for idx in &key_idxs {
            key.push(tuple[*idx].clone());
        }
        if let Some(prev) = seen.insert(key.clone(), i) {
            return Err(anyhow!(
                "key violation in instance `{inst_name}` on `{relation_name}({})`: duplicate key at tuples {prev} and {i}",
                key_fields.join(", ")
            ));
        }
    }
    Ok(())
}

fn check_functional_on_tuples(
    inst_name: &str,
    relation_name: &str,
    relation_fields: &[String],
    tuples: impl Iterator<Item = Vec<String>>,
    src_field: &str,
    dst_field: &str,
) -> Result<()> {
    let Some(src_idx) = relation_fields.iter().position(|x| x == src_field) else {
        return Err(anyhow!(
            "instance `{inst_name}` relation `{relation_name}`: functional src field `{src_field}` is not a declared field",
        ));
    };
    let Some(dst_idx) = relation_fields.iter().position(|x| x == dst_field) else {
        return Err(anyhow!(
            "instance `{inst_name}` relation `{relation_name}`: functional dst field `{dst_field}` is not a declared field",
        ));
    };

    let mut map: HashMap<String, String> = HashMap::new();
    for (i, tuple) in tuples.enumerate() {
        let src = tuple[src_idx].clone();
        let dst = tuple[dst_idx].clone();
        if let Some(prev) = map.get(&src) {
            if prev != &dst {
                return Err(anyhow!(
                    "functional violation in instance `{inst_name}` on `{relation_name}`.{src_field} -> {relation_name}.{dst_field}: src `{src}` maps to both `{prev}` and `{dst}` (tuple {i})",
                ));
            }
        } else {
            map.insert(src, dst);
        }
    }
    Ok(())
}

fn check_symmetric_closure_compatible_with_keys_and_functionals(
    inst: &SchemaV1Instance,
    schema_name: &str,
    relation_name: &str,
    relation_fields: &[String],
    where_field: Option<&str>,
    where_values: &[String],
    all_constraints: &[CoreConstraint<'_>],
) -> Result<()> {
    if relation_fields.len() < 2 {
        return Err(anyhow!(
            "instance `{}` relation `{relation_name}`: symmetric constraint requires at least 2 fields",
            inst.name
        ));
    }

    let mut cond_idx: Option<usize> = None;
    if let Some(field) = where_field {
        let Some(idx) = relation_fields.iter().position(|x| x == field) else {
            return Err(anyhow!(
                "instance `{}` relation `{relation_name}`: symmetric where-field `{field}` is not a declared field",
                inst.name
            ));
        };
        cond_idx = Some(idx);
    }

    let mut closed: Vec<Vec<String>> = Vec::new();
    let mut seen: std::collections::HashSet<Vec<String>> = std::collections::HashSet::new();

    for tuple in relation_tuples(inst, relation_name) {
        let vals = tuple_values_in_order(&inst.name, relation_name, tuple, relation_fields)?;
        if seen.insert(vals.clone()) {
            closed.push(vals.clone());
        }

        let apply = match cond_idx {
            None => true,
            Some(idx) => where_values.iter().any(|v| v == &vals[idx]),
        };
        if apply {
            let mut swapped = vals;
            swapped.swap(0, 1);
            if seen.insert(swapped.clone()) {
                closed.push(swapped);
            }
        }
    }

    // Re-check keys/functionals for this relation on the symmetric closure.
    for c in all_constraints.iter() {
        match c {
            CoreConstraint::Key {
                schema,
                relation,
                fields,
            } if *schema == schema_name && *relation == relation_name => {
                check_key_on_tuples(
                    &inst.name,
                    relation_name,
                    relation_fields,
                    closed.iter().cloned(),
                    fields,
                )?;
            }
            CoreConstraint::Functional {
                schema,
                relation,
                src_field,
                dst_field,
            } if *schema == schema_name && *relation == relation_name => {
                check_functional_on_tuples(
                    &inst.name,
                    relation_name,
                    relation_fields,
                    closed.iter().cloned(),
                    src_field,
                    dst_field,
                )?;
            }
            _ => {}
        }
    }

    Ok(())
}

fn check_transitive_closure_compatible_with_keys_and_functionals(
    inst: &SchemaV1Instance,
    schema_name: &str,
    relation_name: &str,
    relation_fields: &[String],
    all_constraints: &[CoreConstraint<'_>],
) -> Result<()> {
    if relation_fields.len() < 2 {
        return Err(anyhow!(
            "instance `{}` relation `{relation_name}`: transitive constraint requires at least 2 fields",
            inst.name
        ));
    }
    let carrier0 = relation_fields[0].as_str();
    let carrier1 = relation_fields[1].as_str();

    // We only certify "closure compatibility" when keys/functionals are present,
    // and only for constraints that talk about the carrier fields.
    let mut has_relevant_checks = false;
    for c in all_constraints.iter() {
        match c {
            CoreConstraint::Key {
                schema,
                relation,
                fields,
            } if *schema == schema_name && *relation == relation_name => {
                has_relevant_checks = true;
                for f in *fields {
                    if f != carrier0 && f != carrier1 {
                        return Err(anyhow!(
                            "transitive `{schema_name}.{relation_name}`: key constraint mentions non-carrier field `{f}` (only `{carrier0}` and `{carrier1}` are supported for transitive closure-compatibility checks)",
                        ));
                    }
                }
            }
            CoreConstraint::Functional {
                schema,
                relation,
                src_field,
                dst_field,
            } if *schema == schema_name && *relation == relation_name => {
                has_relevant_checks = true;
                if *src_field != carrier0
                    && *src_field != carrier1
                    && *dst_field != carrier0
                    && *dst_field != carrier1
                {
                    return Err(anyhow!(
                        "transitive `{schema_name}.{relation_name}`: functional constraint mentions non-carrier fields (`{src_field}` -> `{dst_field}`); only `{carrier0}` and `{carrier1}` are supported for transitive closure-compatibility checks",
                    ));
                }
                if *src_field != carrier0 && *src_field != carrier1 {
                    return Err(anyhow!(
                        "transitive `{schema_name}.{relation_name}`: functional src field `{src_field}` is not a carrier field (`{carrier0}` or `{carrier1}`)",
                    ));
                }
                if *dst_field != carrier0 && *dst_field != carrier1 {
                    return Err(anyhow!(
                        "transitive `{schema_name}.{relation_name}`: functional dst field `{dst_field}` is not a carrier field (`{carrier0}` or `{carrier1}`)",
                    ));
                }
            }
            _ => {}
        }
    }

    if !has_relevant_checks {
        return Ok(());
    }

    // Build transitive closure on the carrier pair (field0, field1).
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    for tuple in relation_tuples(inst, relation_name) {
        let vals = tuple_values_in_order(&inst.name, relation_name, tuple, relation_fields)?;
        let src = vals[0].clone();
        let dst = vals[1].clone();
        adj.entry(src).or_default().push(dst);
    }

    let mut closure: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    let sources: Vec<String> = adj.keys().cloned().collect();
    for src in sources {
        let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut queue: std::collections::VecDeque<String> = std::collections::VecDeque::new();
        if let Some(neigh) = adj.get(&src) {
            for v in neigh {
                queue.push_back(v.clone());
            }
        }
        while let Some(v) = queue.pop_front() {
            if !visited.insert(v.clone()) {
                continue;
            }
            closure.insert((src.clone(), v.clone()));
            if let Some(more) = adj.get(&v) {
                for w in more {
                    queue.push_back(w.clone());
                }
            }
        }
    }

    let closure_pairs: Vec<Vec<String>> = closure
        .into_iter()
        .map(|(a, b)| vec![a, b])
        .collect();
    let carrier_fields = vec![carrier0.to_string(), carrier1.to_string()];

    // Re-check keys/functionals for this relation on the transitive closure of the carrier fields.
    for c in all_constraints.iter() {
        match c {
            CoreConstraint::Key {
                schema,
                relation,
                fields,
            } if *schema == schema_name && *relation == relation_name => {
                check_key_on_tuples(
                    &inst.name,
                    relation_name,
                    &carrier_fields,
                    closure_pairs.iter().cloned(),
                    fields,
                )?;
            }
            CoreConstraint::Functional {
                schema,
                relation,
                src_field,
                dst_field,
            } if *schema == schema_name && *relation == relation_name => {
                check_functional_on_tuples(
                    &inst.name,
                    relation_name,
                    &carrier_fields,
                    closure_pairs.iter().cloned(),
                    src_field,
                    dst_field,
                )?;
            }
            _ => {}
        }
    }

    Ok(())
}

fn parse_nat_const(name: &str) -> Option<i64> {
    let rest = name.strip_prefix("Nat")?;
    if rest.is_empty() {
        return None;
    }
    rest.parse::<i64>().ok()
}

fn nat_const(n: i64) -> Option<String> {
    if n < 0 {
        return None;
    }
    Some(format!("Nat{n}"))
}

fn check_typing_rule_preserves_manifold_and_increments_degree(
    _inst: &SchemaV1Instance,
    input: &str,
    output: &str,
    form_on: &HashMap<String, String>,
    form_degree: &HashMap<String, String>,
    derived_form_on: &mut HashMap<String, String>,
    derived_form_degree: &mut HashMap<String, String>,
) -> Result<()> {
    let Some(m) = form_on.get(input) else {
        return Err(anyhow!(
            "typing ExteriorDerivative: missing FormOn(form={input}, manifold=...)"
        ));
    };
    if let Some(existing) = form_on.get(output) {
        if existing != m {
            return Err(anyhow!(
                "typing ExteriorDerivative: output form `{output}` is on `{existing}`, expected `{m}`"
            ));
        }
    }
    if let Some(prev) = derived_form_on.insert(output.to_string(), m.clone()) {
        if prev != *m {
            return Err(anyhow!(
                "typing ExteriorDerivative: output form `{output}` inferred on both `{prev}` and `{m}`"
            ));
        }
    }

    let Some(k) = form_degree.get(input) else {
        return Err(anyhow!(
            "typing ExteriorDerivative: missing FormDegree(form={input}, degree=...)"
        ));
    };
    let Some(k_num) = parse_nat_const(k) else {
        return Err(anyhow!(
            "typing ExteriorDerivative: unsupported Nat constant `{k}` (expected Nat0, Nat1, ...)"
        ));
    };
    let Some(kp1) = nat_const(k_num + 1) else {
        return Err(anyhow!("typing ExteriorDerivative: degree overflow"));
    };
    if let Some(existing) = form_degree.get(output) {
        if existing != &kp1 {
            return Err(anyhow!(
                "typing ExteriorDerivative: output form `{output}` has degree `{existing}`, expected `{kp1}`"
            ));
        }
    }
    if let Some(prev) = derived_form_degree.insert(output.to_string(), kp1.clone()) {
        if prev != kp1 {
            return Err(anyhow!(
                "typing ExteriorDerivative: output form `{output}` inferred degrees conflict: `{prev}` vs `{kp1}`"
            ));
        }
    }

    Ok(())
}

fn check_typing_rule_preserves_manifold_and_adds_degree(
    _inst: &SchemaV1Instance,
    left: &str,
    right: &str,
    out: &str,
    form_on: &HashMap<String, String>,
    form_degree: &HashMap<String, String>,
    derived_form_on: &mut HashMap<String, String>,
    derived_form_degree: &mut HashMap<String, String>,
) -> Result<()> {
    let Some(m_left) = form_on.get(left) else {
        return Err(anyhow!(
            "typing Wedge: missing FormOn(form={left}, manifold=...)"
        ));
    };
    let Some(m_right) = form_on.get(right) else {
        return Err(anyhow!(
            "typing Wedge: missing FormOn(form={right}, manifold=...)"
        ));
    };
    if m_left != m_right {
        return Err(anyhow!(
            "typing Wedge: forms `{left}` and `{right}` live on different manifolds (`{m_left}` vs `{m_right}`)"
        ));
    }
    if let Some(existing) = form_on.get(out) {
        if existing != m_left {
            return Err(anyhow!(
                "typing Wedge: output form `{out}` is on `{existing}`, expected `{m_left}`"
            ));
        }
    }
    if let Some(prev) = derived_form_on.insert(out.to_string(), m_left.clone()) {
        if prev != *m_left {
            return Err(anyhow!(
                "typing Wedge: output form `{out}` inferred on both `{prev}` and `{m_left}`"
            ));
        }
    }

    let Some(k_left) = form_degree.get(left) else {
        return Err(anyhow!(
            "typing Wedge: missing FormDegree(form={left}, degree=...)"
        ));
    };
    let Some(k_right) = form_degree.get(right) else {
        return Err(anyhow!(
            "typing Wedge: missing FormDegree(form={right}, degree=...)"
        ));
    };
    let Some(k_left_num) = parse_nat_const(k_left) else {
        return Err(anyhow!(
            "typing Wedge: unsupported Nat constant `{k_left}` (expected Nat0, Nat1, ...)"
        ));
    };
    let Some(k_right_num) = parse_nat_const(k_right) else {
        return Err(anyhow!(
            "typing Wedge: unsupported Nat constant `{k_right}` (expected Nat0, Nat1, ...)"
        ));
    };
    let Some(sum) = nat_const(k_left_num + k_right_num) else {
        return Err(anyhow!("typing Wedge: degree overflow"));
    };
    if let Some(existing) = form_degree.get(out) {
        if existing != &sum {
            return Err(anyhow!(
                "typing Wedge: output form `{out}` has degree `{existing}`, expected `{sum}`"
            ));
        }
    }
    if let Some(prev) = derived_form_degree.insert(out.to_string(), sum.clone()) {
        if prev != sum {
            return Err(anyhow!(
                "typing Wedge: output form `{out}` inferred degrees conflict: `{prev}` vs `{sum}`"
            ));
        }
    }

    Ok(())
}

fn check_typing_rule_depends_on_metric_and_dualizes_degree(
    _inst: &SchemaV1Instance,
    metric: &str,
    input: &str,
    output: &str,
    metric_on: &HashMap<String, String>,
    manifold_dim: &HashMap<String, String>,
    form_on: &HashMap<String, String>,
    form_degree: &HashMap<String, String>,
    derived_form_on: &mut HashMap<String, String>,
    derived_form_degree: &mut HashMap<String, String>,
) -> Result<()> {
    let Some(m) = metric_on.get(metric) else {
        return Err(anyhow!(
            "typing HodgeStar: missing MetricOn(metric={metric}, manifold=...)"
        ));
    };
    let Some(m_in) = form_on.get(input) else {
        return Err(anyhow!(
            "typing HodgeStar: missing FormOn(form={input}, manifold=...)"
        ));
    };
    if m_in != m {
        return Err(anyhow!(
            "typing HodgeStar: metric `{metric}` is on `{m}`, but input form `{input}` is on `{m_in}`"
        ));
    }

    if let Some(existing) = form_on.get(output) {
        if existing != m {
            return Err(anyhow!(
                "typing HodgeStar: output form `{output}` is on `{existing}`, expected `{m}`"
            ));
        }
    }
    if let Some(prev) = derived_form_on.insert(output.to_string(), m.clone()) {
        if prev != *m {
            return Err(anyhow!(
                "typing HodgeStar: output form `{output}` inferred on both `{prev}` and `{m}`"
            ));
        }
    }

    let Some(n) = manifold_dim.get(m) else {
        return Err(anyhow!(
            "typing HodgeStar: missing ManifoldDimension(manifold={m}, dim=...)"
        ));
    };
    let Some(k) = form_degree.get(input) else {
        return Err(anyhow!(
            "typing HodgeStar: missing FormDegree(form={input}, degree=...)"
        ));
    };
    let Some(n_num) = parse_nat_const(n) else {
        return Err(anyhow!(
            "typing HodgeStar: unsupported Nat constant `{n}` (expected Nat0, Nat1, ...)"
        ));
    };
    let Some(k_num) = parse_nat_const(k) else {
        return Err(anyhow!(
            "typing HodgeStar: unsupported Nat constant `{k}` (expected Nat0, Nat1, ...)"
        ));
    };
    let Some(out_deg) = nat_const(n_num - k_num) else {
        return Err(anyhow!(
            "typing HodgeStar: cannot compute n-k with n={n} and k={k}"
        ));
    };
    if let Some(existing) = form_degree.get(output) {
        if existing != &out_deg {
            return Err(anyhow!(
                "typing HodgeStar: output form `{output}` has degree `{existing}`, expected `{out_deg}`"
            ));
        }
    }
    if let Some(prev) = derived_form_degree.insert(output.to_string(), out_deg.clone()) {
        if prev != out_deg {
            return Err(anyhow!(
                "typing HodgeStar: output form `{output}` inferred degrees conflict: `{prev}` vs `{out_deg}`"
            ));
        }
    }

    Ok(())
}

fn binary_relation_map(
    inst: &SchemaV1Instance,
    relation_name: &str,
    key_field: &str,
    value_field: &str,
) -> Result<HashMap<String, String>> {
    let mut out: HashMap<String, String> = HashMap::new();
    for tuple in relation_tuples(inst, relation_name) {
        let mut map: HashMap<&str, &str> = HashMap::new();
        for (k, v) in tuple {
            map.insert(k.as_str(), v.as_str());
        }
        let Some(k) = map.get(key_field) else {
            continue;
        };
        let Some(v) = map.get(value_field) else {
            continue;
        };
        if let Some(prev) = out.get(*k) {
            if prev != v {
                return Err(anyhow!(
                    "instance `{}` relation `{relation_name}`: `{key_field}` `{k}` maps to both `{prev}` and `{v}`",
                    inst.name
                ));
            }
        } else {
            out.insert((*k).to_string(), (*v).to_string());
        }
    }
    Ok(out)
}

fn check_typing_constraint(
    inst: &SchemaV1Instance,
    schema_name: &str,
    relation_name: &str,
    rule: &str,
    field_index: &RelationFieldIndex,
) -> Result<()> {
    match rule {
        "preserves_manifold_and_increments_degree" => {
            // Requires: FormOn(form, manifold), FormDegree(form, degree)
            let _ = field_index.relation_fields(schema_name, "FormOn")?;
            let _ = field_index.relation_fields(schema_name, "FormDegree")?;
            let rel_fields = field_index.relation_fields(schema_name, relation_name)?;

            let form_on = binary_relation_map(inst, "FormOn", "form", "manifold")?;
            let form_degree = binary_relation_map(inst, "FormDegree", "form", "degree")?;
            let mut derived_form_on: HashMap<String, String> = HashMap::new();
            let mut derived_form_degree: HashMap<String, String> = HashMap::new();

            for tuple in relation_tuples(inst, relation_name) {
                let mut tmap: HashMap<&str, &str> = HashMap::new();
                for (k, v) in tuple {
                    tmap.insert(k.as_str(), v.as_str());
                }

                // Strict: require the tuple uses the declared field names.
                let input = tmap.get("input").copied().ok_or_else(|| {
                    anyhow!(
                        "typing {relation_name}: missing field `input` in tuple (expected fields: {})",
                        rel_fields.join(", ")
                    )
                })?;
                let output = tmap.get("output").copied().ok_or_else(|| {
                    anyhow!(
                        "typing {relation_name}: missing field `output` in tuple (expected fields: {})",
                        rel_fields.join(", ")
                    )
                })?;

                check_typing_rule_preserves_manifold_and_increments_degree(
                    inst,
                    input,
                    output,
                    &form_on,
                    &form_degree,
                    &mut derived_form_on,
                    &mut derived_form_degree,
                )?;
            }
            Ok(())
        }
        "preserves_manifold_and_adds_degree" => {
            let _ = field_index.relation_fields(schema_name, "FormOn")?;
            let _ = field_index.relation_fields(schema_name, "FormDegree")?;
            let rel_fields = field_index.relation_fields(schema_name, relation_name)?;

            let form_on = binary_relation_map(inst, "FormOn", "form", "manifold")?;
            let form_degree = binary_relation_map(inst, "FormDegree", "form", "degree")?;
            let mut derived_form_on: HashMap<String, String> = HashMap::new();
            let mut derived_form_degree: HashMap<String, String> = HashMap::new();

            for tuple in relation_tuples(inst, relation_name) {
                let mut tmap: HashMap<&str, &str> = HashMap::new();
                for (k, v) in tuple {
                    tmap.insert(k.as_str(), v.as_str());
                }
                let left = tmap.get("left").copied().ok_or_else(|| {
                    anyhow!(
                        "typing {relation_name}: missing field `left` in tuple (expected fields: {})",
                        rel_fields.join(", ")
                    )
                })?;
                let right = tmap.get("right").copied().ok_or_else(|| {
                    anyhow!(
                        "typing {relation_name}: missing field `right` in tuple (expected fields: {})",
                        rel_fields.join(", ")
                    )
                })?;
                let out = tmap.get("out").copied().ok_or_else(|| {
                    anyhow!(
                        "typing {relation_name}: missing field `out` in tuple (expected fields: {})",
                        rel_fields.join(", ")
                    )
                })?;

                check_typing_rule_preserves_manifold_and_adds_degree(
                    inst,
                    left,
                    right,
                    out,
                    &form_on,
                    &form_degree,
                    &mut derived_form_on,
                    &mut derived_form_degree,
                )?;
            }
            Ok(())
        }
        "depends_on_metric_and_dualizes_degree" => {
            let _ = field_index.relation_fields(schema_name, "MetricOn")?;
            let _ = field_index.relation_fields(schema_name, "ManifoldDimension")?;
            let _ = field_index.relation_fields(schema_name, "FormOn")?;
            let _ = field_index.relation_fields(schema_name, "FormDegree")?;
            let rel_fields = field_index.relation_fields(schema_name, relation_name)?;

            let metric_on = binary_relation_map(inst, "MetricOn", "metric", "manifold")?;
            let manifold_dim = binary_relation_map(inst, "ManifoldDimension", "manifold", "dim")?;
            let form_on = binary_relation_map(inst, "FormOn", "form", "manifold")?;
            let form_degree = binary_relation_map(inst, "FormDegree", "form", "degree")?;

            let mut derived_form_on: HashMap<String, String> = HashMap::new();
            let mut derived_form_degree: HashMap<String, String> = HashMap::new();

            for tuple in relation_tuples(inst, relation_name) {
                let mut tmap: HashMap<&str, &str> = HashMap::new();
                for (k, v) in tuple {
                    tmap.insert(k.as_str(), v.as_str());
                }
                let metric = tmap.get("metric").copied().ok_or_else(|| {
                    anyhow!(
                        "typing {relation_name}: missing field `metric` in tuple (expected fields: {})",
                        rel_fields.join(", ")
                    )
                })?;
                let input = tmap.get("input").copied().ok_or_else(|| {
                    anyhow!(
                        "typing {relation_name}: missing field `input` in tuple (expected fields: {})",
                        rel_fields.join(", ")
                    )
                })?;
                let output = tmap.get("output").copied().ok_or_else(|| {
                    anyhow!(
                        "typing {relation_name}: missing field `output` in tuple (expected fields: {})",
                        rel_fields.join(", ")
                    )
                })?;

                check_typing_rule_depends_on_metric_and_dualizes_degree(
                    inst,
                    metric,
                    input,
                    output,
                    &metric_on,
                    &manifold_dim,
                    &form_on,
                    &form_degree,
                    &mut derived_form_on,
                    &mut derived_form_degree,
                )?;
            }
            Ok(())
        }
        _ => Err(anyhow!(
            "unsupported typing constraint rule `{rule}` for relation `{relation_name}`",
        )),
    }
}

/// Check that a canonical `.axi` module satisfies its core constraints.
///
/// Returns an `AxiConstraintsOkProofV1` summary suitable for certificate emission.
pub fn check_axi_constraints_ok_v1(module: &SchemaV1Module) -> Result<AxiConstraintsOkProofV1> {
    // Fail-closed: `axi_constraints_ok_v1` is a conservative gate intended to
    // be meaningful under a well-specified constraint semantics. If the module
    // contains truly unknown/unsupported constraints, we refuse to certify it
    // (even if the known subset happens to pass), because the meaning-plane may
    // contain semantics drift that the checker cannot account for.
    let mut unknown: Vec<(String, String)> = Vec::new();
    for th in &module.theories {
        for c in &th.constraints {
            if let ConstraintV1::Unknown { text } = c {
                unknown.push((th.name.clone(), text.clone()));
            }
        }
    }
    if !unknown.is_empty() {
        let mut msg = String::new();
        msg.push_str(
            "axi_constraints_ok_v1 refused: unknown/unsupported theory constraints found.\n",
        );
        msg.push_str(
            "Rewrite them into canonical structured forms (or use a `constraint Name:` named-block).\n",
        );
        msg.push_str("Unknown constraints:\n");
        for (i, (th_name, text)) in unknown.iter().take(8).enumerate() {
            msg.push_str(&format!("  {i}: theory `{th_name}`: {text}\n"));
        }
        if unknown.len() > 8 {
            msg.push_str(&format!("  ... ({} more)\n", unknown.len() - 8));
        }
        return Err(anyhow!(msg.trim_end().to_string()));
    }

    // Ensure schemas referenced by theories exist.
    let mut schema_names: HashMap<&str, ()> = HashMap::new();
    for s in &module.schemas {
        schema_names.insert(s.name.as_str(), ());
    }
    for th in &module.theories {
        if !schema_names.contains_key(th.schema.as_str()) {
            return Err(anyhow!(
                "theory `{}` references unknown schema `{}`",
                th.name,
                th.schema
            ));
        }
    }

    let constraints = gather_core_constraints(module);
    let field_index = RelationFieldIndex::from_module(module);
    let mut check_count: u32 = 0;

    for inst in &module.instances {
        // Apply constraints only for the instance's schema.
        for c in constraints.iter().filter(|c| match c {
            CoreConstraint::Key { schema, .. } => *schema == inst.schema,
            CoreConstraint::Functional { schema, .. } => *schema == inst.schema,
            CoreConstraint::Symmetric { schema, .. } => *schema == inst.schema,
            CoreConstraint::SymmetricWhereIn { schema, .. } => *schema == inst.schema,
            CoreConstraint::Transitive { schema, .. } => *schema == inst.schema,
            CoreConstraint::Typing { schema, .. } => *schema == inst.schema,
        }) {
            check_count += 1;
            match c {
                CoreConstraint::Key {
                    relation, fields, ..
                } => {
                    let relation_fields = field_index.relation_fields(&inst.schema, relation)?;
                    check_key_on_tuples(
                        &inst.name,
                        relation,
                        relation_fields,
                        relation_tuples(inst, relation).map(|t| {
                            tuple_values_in_order(&inst.name, relation, t, relation_fields)
                        }).collect::<Result<Vec<_>>>()?.into_iter(),
                        fields,
                    )?;
                }
                CoreConstraint::Functional {
                    relation,
                    src_field,
                    dst_field,
                    ..
                } => {
                    let relation_fields = field_index.relation_fields(&inst.schema, relation)?;
                    check_functional_on_tuples(
                        &inst.name,
                        relation,
                        relation_fields,
                        relation_tuples(inst, relation).map(|t| {
                            tuple_values_in_order(&inst.name, relation, t, relation_fields)
                        }).collect::<Result<Vec<_>>>()?.into_iter(),
                        src_field,
                        dst_field,
                    )?;
                }
                CoreConstraint::Symmetric { relation, .. } => {
                    let relation_fields = field_index.relation_fields(&inst.schema, relation)?;
                    check_symmetric_closure_compatible_with_keys_and_functionals(
                        inst,
                        &inst.schema,
                        relation,
                        relation_fields,
                        None,
                        &[],
                        &constraints,
                    )?;
                }
                CoreConstraint::SymmetricWhereIn {
                    relation,
                    field,
                    values,
                    ..
                } => {
                    let relation_fields = field_index.relation_fields(&inst.schema, relation)?;
                    check_symmetric_closure_compatible_with_keys_and_functionals(
                        inst,
                        &inst.schema,
                        relation,
                        relation_fields,
                        Some(field),
                        values,
                        &constraints,
                    )?;
                }
                CoreConstraint::Transitive { relation, .. } => {
                    let relation_fields = field_index.relation_fields(&inst.schema, relation)?;
                    check_transitive_closure_compatible_with_keys_and_functionals(
                        inst,
                        &inst.schema,
                        relation,
                        relation_fields,
                        &constraints,
                    )?;
                }
                CoreConstraint::Typing {
                    relation, rule, ..
                } => {
                    check_typing_constraint(inst, &inst.schema, relation, rule, &field_index)?;
                }
            }
        }
    }

    let constraint_count: u32 = constraints.len() as u32;
    let instance_count: u32 = module.instances.len() as u32;
    Ok(AxiConstraintsOkProofV1 {
        module_name: module.module_name.clone(),
        constraint_count,
        instance_count,
        check_count,
    })
}
