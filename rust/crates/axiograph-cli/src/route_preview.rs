use anyhow::{anyhow, Result};
use axiograph_pathdb::{
    AxiDigest, FixedPointProbability, NoProof, PathDB, PathExprV2, ProofProducingOptimizer,
};
use serde::{Deserialize, Serialize};

pub const ROUTE_PREVIEW_MAX_SEGMENTS: usize = 128;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RouteDirectionV1 {
    #[default]
    Forward,
    Reverse,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteSegmentRefV1 {
    pub relation_id: u32,
    #[serde(default)]
    pub direction: RouteDirectionV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteChainV1 {
    pub start_entity: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub segments: Vec<RouteSegmentRefV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoutePreviewRequestV1 {
    pub route: RouteChainV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equivalent_to: Option<RouteChainV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteHopPreviewV1 {
    pub relation_id: u32,
    pub relation: String,
    pub direction: RouteDirectionV1,
    pub from: u32,
    pub to: u32,
    pub confidence_fp: FixedPointProbability,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NormalizedRouteHopV1 {
    pub relation: String,
    pub direction: RouteDirectionV1,
    pub from: u32,
    pub to: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NormalizedRoutePreviewV1 {
    pub start_entity: u32,
    pub end_entity: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hops: Vec<NormalizedRouteHopV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolvedRoutePreviewV1 {
    pub requested: RouteChainV1,
    pub end_entity: u32,
    pub confidence_fp: FixedPointProbability,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hops: Vec<RouteHopPreviewV1>,
    pub normalized: NormalizedRoutePreviewV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteEquivalencePreviewV1 {
    pub equivalent: bool,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shared_normalized_route: Option<NormalizedRoutePreviewV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteCertificatePreviewV1 {
    pub kind: String,
    pub anchor_digest: AxiDigest,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscoverRoutePreviewReportV1 {
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_digest: Option<AxiDigest>,
    pub trust: crate::trust_contract::TrustContractV1,
    pub route: ResolvedRoutePreviewV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equivalent_to: Option<ResolvedRoutePreviewV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equivalence: Option<RouteEquivalencePreviewV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub certificate_preview: Option<RouteCertificatePreviewV1>,
}

fn relation_name(db: &PathDB, rel_type: u32) -> Result<String> {
    db.interner
        .lookup(axiograph_pathdb::StrId::new(rel_type))
        .map(|name| name.to_string())
        .ok_or_else(|| anyhow!("internal error: missing relation name for {rel_type}"))
}

fn identity_confidence() -> FixedPointProbability {
    FixedPointProbability::from_f32(1.0)
}

fn route_expr_from_atoms(start_entity: u32, atoms: Vec<PathExprV2>) -> PathExprV2 {
    let mut atoms = atoms.into_iter();
    let Some(first) = atoms.next() else {
        return PathExprV2::Reflexive {
            entity: start_entity,
        };
    };
    atoms.fold(first, |left, right| PathExprV2::Trans {
        left: Box::new(left),
        right: Box::new(right),
    })
}

fn collect_normalized_hops(
    db: &PathDB,
    expr: &PathExprV2,
    hops: &mut Vec<NormalizedRouteHopV1>,
) -> Result<()> {
    match expr {
        PathExprV2::Reflexive { .. } => Ok(()),
        PathExprV2::Step { from, rel_type, to } => {
            hops.push(NormalizedRouteHopV1 {
                relation: relation_name(db, *rel_type)?,
                direction: RouteDirectionV1::Forward,
                from: *from,
                to: *to,
            });
            Ok(())
        }
        PathExprV2::Inv { path } => match path.as_ref() {
            PathExprV2::Step { from, rel_type, to } => {
                hops.push(NormalizedRouteHopV1 {
                    relation: relation_name(db, *rel_type)?,
                    direction: RouteDirectionV1::Reverse,
                    from: *to,
                    to: *from,
                });
                Ok(())
            }
            _ => Err(anyhow!(
                "normalized route unexpectedly contained a non-atomic inverse"
            )),
        },
        PathExprV2::Trans { left, right } => {
            collect_normalized_hops(db, left, hops)?;
            collect_normalized_hops(db, right, hops)
        }
    }
}

fn normalized_route_preview(db: &PathDB, expr: &PathExprV2) -> Result<NormalizedRoutePreviewV1> {
    let mut hops = Vec::new();
    collect_normalized_hops(db, expr, &mut hops)?;
    Ok(NormalizedRoutePreviewV1 {
        start_entity: expr.start(),
        end_entity: expr.end(),
        hops,
    })
}

fn resolve_route(
    db: &PathDB,
    optimizer: &ProofProducingOptimizer,
    route: &RouteChainV1,
) -> Result<(ResolvedRoutePreviewV1, PathExprV2)> {
    validate_route_chain(db, route)?;
    let mut current = route.start_entity;
    let mut atoms = Vec::with_capacity(route.segments.len());
    let mut hops = Vec::with_capacity(route.segments.len());
    let mut confidence_fp = identity_confidence();

    for segment in &route.segments {
        let relation = db
            .relations
            .get_relation(segment.relation_id)
            .ok_or_else(|| anyhow!("unknown relation_id {}", segment.relation_id))?;
        let step_confidence = FixedPointProbability::from_f32(relation.confidence);
        let relation_name = relation_name(db, relation.rel_type.raw())?;

        let (atom, from, to, next_entity) = match segment.direction {
            RouteDirectionV1::Forward => {
                if relation.source != current {
                    return Err(anyhow!(
                        "route segment relation_id {} expected source={} but started at {}",
                        segment.relation_id,
                        relation.source,
                        current
                    ));
                }
                (
                    PathExprV2::Step {
                        from: relation.source,
                        rel_type: relation.rel_type.raw(),
                        to: relation.target,
                    },
                    relation.source,
                    relation.target,
                    relation.target,
                )
            }
            RouteDirectionV1::Reverse => {
                if relation.target != current {
                    return Err(anyhow!(
                        "route segment relation_id {} expected reverse start={} but started at {}",
                        segment.relation_id,
                        relation.target,
                        current
                    ));
                }
                (
                    PathExprV2::Inv {
                        path: Box::new(PathExprV2::Step {
                            from: relation.source,
                            rel_type: relation.rel_type.raw(),
                            to: relation.target,
                        }),
                    },
                    relation.target,
                    relation.source,
                    relation.source,
                )
            }
        };

        confidence_fp = confidence_fp.mul(step_confidence);
        hops.push(RouteHopPreviewV1 {
            relation_id: segment.relation_id,
            relation: relation_name,
            direction: segment.direction,
            from,
            to,
            confidence_fp: step_confidence,
        });
        atoms.push(atom);
        current = next_entity;
    }

    let expr = route_expr_from_atoms(route.start_entity, atoms);
    let normalized_expr = optimizer.normalize_path_v2::<NoProof>(expr.clone()).value;
    let normalized = normalized_route_preview(db, &normalized_expr)?;

    Ok((
        ResolvedRoutePreviewV1 {
            requested: route.clone(),
            end_entity: current,
            confidence_fp,
            hops,
            normalized,
        },
        expr,
    ))
}

fn validate_route_chain(db: &PathDB, route: &RouteChainV1) -> Result<()> {
    if route.segments.len() > ROUTE_PREVIEW_MAX_SEGMENTS {
        return Err(anyhow!(
            "route has {} segments; max supported preview length is {}",
            route.segments.len(),
            ROUTE_PREVIEW_MAX_SEGMENTS
        ));
    }
    if db.get_entity(route.start_entity).is_none() {
        return Err(anyhow!("unknown start_entity {}", route.start_entity));
    }
    Ok(())
}

fn try_anchor_digest(_db: &PathDB) -> Option<AxiDigest> {
    // A derived PathDB cannot reconstruct the exact accepted `.axi` bytes.
    None
}

fn preview_trust(
    anchor_digest: Option<&AxiDigest>,
    comparing: bool,
    equivalent: Option<bool>,
) -> crate::trust_contract::TrustContractV1 {
    let mut reasons = vec![
        "requested routes are resolved against concrete PathDB relation ids and directions".to_string(),
        "preview reuses the existing path normalization/equivalence runtime rather than inventing a parallel route engine".to_string(),
        "normalized routes intentionally expose typed hops and endpoints instead of raw homotopy/path-expression objects".to_string(),
        "preview does not claim exhaustive route search, completeness, or ontology closure beyond the provided route chain(s)".to_string(),
    ];

    if anchor_digest.is_some() {
        reasons.push(
            "an explicit canonical `.axi` anchor was supplied, so anchored certificate availability can be previewed".to_string(),
        );
    } else {
        reasons.push(
            "no explicit canonical `.axi` bytes and digest were supplied; derived PathDB rows cannot reconstruct the anchor, so certificate preview metadata is omitted".to_string(),
        );
    }

    if comparing {
        match equivalent {
            Some(true) => reasons.push(
                "both routes normalize to the same typed route, so an equivalence certificate can be advertised when anchored".to_string(),
            ),
            Some(false) => reasons.push(
                "the compared routes normalize differently, so no equivalence certificate preview is advertised".to_string(),
            ),
            None => {}
        }
    }

    crate::trust_contract::TrustContractV1 {
        trust_class: "runtime_guarded".to_string(),
        soundness: if comparing {
            "route_equivalence_preview_reuses_path_equiv_runtime".to_string()
        } else {
            "route_preview_reuses_path_normalization_runtime".to_string()
        },
        coverage: if comparing {
            "paired_route_equivalence_preview".to_string()
        } else {
            "single_route_normalization_preview".to_string()
        },
        scope: crate::trust_contract::TrustScopeV1 {
            anchor: "snapshot_scoped".to_string(),
            context: "not_applicable".to_string(),
        },
        reasons,
        certifiable_disjuncts: None,
        execution_only_disjuncts: None,
        semantic_coverage: None,
        semantic_claims: Vec::new(),
        gaps: Vec::new(),
    }
}

pub fn discover_route_preview_from_request(
    db: &PathDB,
    request: &RoutePreviewRequestV1,
) -> Result<DiscoverRoutePreviewReportV1> {
    let optimizer = ProofProducingOptimizer;
    let anchor_digest = try_anchor_digest(db);

    let (route, route_expr) = resolve_route(db, &optimizer, &request.route)?;

    let (equivalent_to, equivalence, certificate_preview) = if let Some(other) =
        request.equivalent_to.as_ref()
    {
        let (other_preview, other_expr) = resolve_route(db, &optimizer, other)?;
        match optimizer.path_equiv_v2::<NoProof>(route_expr.clone(), other_expr.clone()) {
            Ok(proved) => {
                let shared_normalized_route = normalized_route_preview(db, &proved.value)?;
                let certificate_preview =
                    anchor_digest
                        .as_ref()
                        .map(|anchor| RouteCertificatePreviewV1 {
                            kind: "path_equiv_v2".to_string(),
                            anchor_digest: anchor.clone(),
                        });
                (
                    Some(other_preview),
                    Some(RouteEquivalencePreviewV1 {
                        equivalent: true,
                        method: "shared_normal_form".to_string(),
                        shared_normalized_route: Some(shared_normalized_route),
                        reason: Some(
                            "routes share the same normalized typed route under the current path-equivalence rules"
                                .to_string(),
                        ),
                    }),
                    certificate_preview,
                )
            }
            Err(err) => (
                Some(other_preview),
                Some(RouteEquivalencePreviewV1 {
                    equivalent: false,
                    method: "shared_normal_form".to_string(),
                    shared_normalized_route: None,
                    reason: Some(err.to_string()),
                }),
                None,
            ),
        }
    } else {
        let certificate_preview = anchor_digest
            .as_ref()
            .map(|anchor| RouteCertificatePreviewV1 {
                kind: "normalize_path_v2".to_string(),
                anchor_digest: anchor.clone(),
            });
        (None, None, certificate_preview)
    };

    let trust = preview_trust(
        anchor_digest.as_ref(),
        equivalent_to.is_some(),
        equivalence.as_ref().map(|preview| preview.equivalent),
    );

    Ok(DiscoverRoutePreviewReportV1 {
        version: "axiograph_discover_route_preview_v1".to_string(),
        anchor_digest,
        trust,
        route,
        equivalent_to,
        equivalence,
        certificate_preview,
    })
}

pub fn discover_route_preview_from_request_json(
    db: &PathDB,
    request_json: &str,
) -> Result<DiscoverRoutePreviewReportV1> {
    let request: RoutePreviewRequestV1 = crate::security::parse_json_bounded(
        request_json.as_bytes(),
        crate::security::MAX_JSON_INPUT_BYTES,
        "route preview request",
    )?;
    discover_route_preview_from_request(db, &request)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find_named_entity(db: &PathDB, type_name: &str, name: &str) -> Result<u32> {
        let Some(ids) = db.find_by_type(type_name) else {
            return Err(anyhow!("missing type `{type_name}`"));
        };
        ids.iter()
            .find(|id| {
                db.get_entity(*id)
                    .and_then(|entity| entity.attrs.get("name").map(|value| value == name))
                    .unwrap_or(false)
            })
            .ok_or_else(|| anyhow!("missing entity `{name}` of type `{type_name}`"))
    }

    #[test]
    fn route_equivalence_preview_resolves_and_normalizes_without_anchor() -> Result<()> {
        let mut db = PathDB::new();
        let a = db.add_entity("Node", vec![("name", "A")]);
        let b = db.add_entity("Node", vec![("name", "B")]);
        let c = db.add_entity("Node", vec![("name", "C")]);
        let ab = db.add_relation("road", a, b, 0.9, Vec::new());
        let ac = db.add_relation("road", a, c, 0.8, Vec::new());
        db.build_indexes();

        let request_json = serde_json::to_string(&RoutePreviewRequestV1 {
            route: RouteChainV1 {
                start_entity: a,
                segments: vec![
                    RouteSegmentRefV1 {
                        relation_id: ab,
                        direction: RouteDirectionV1::Forward,
                    },
                    RouteSegmentRefV1 {
                        relation_id: ab,
                        direction: RouteDirectionV1::Reverse,
                    },
                    RouteSegmentRefV1 {
                        relation_id: ac,
                        direction: RouteDirectionV1::Forward,
                    },
                ],
            },
            equivalent_to: Some(RouteChainV1 {
                start_entity: a,
                segments: vec![RouteSegmentRefV1 {
                    relation_id: ac,
                    direction: RouteDirectionV1::Forward,
                }],
            }),
        })?;

        let report = discover_route_preview_from_request_json(&db, &request_json)?;
        assert_eq!(report.version, "axiograph_discover_route_preview_v1");
        assert!(report.anchor_digest.is_none());
        assert_eq!(report.trust.trust_class, "runtime_guarded");
        assert_eq!(report.route.normalized.hops.len(), 1);
        assert_eq!(report.route.normalized.hops[0].relation, "road");
        assert_eq!(report.route.normalized.end_entity, c);
        assert!(report
            .equivalence
            .as_ref()
            .is_some_and(|equivalence| equivalence.equivalent));
        assert!(report.certificate_preview.is_none());
        Ok(())
    }

    #[test]
    fn route_preview_does_not_reconstruct_anchor_from_derived_rows() -> Result<()> {
        let axi = r#"
module Demo

schema S:
  object Node

instance I of S:
  Node = {a}
"#;
        let mut db = PathDB::new();
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, axi)?;
        db.build_indexes();
        let a = find_named_entity(&db, "Node", "a")?;

        let request_json = serde_json::to_string(&RoutePreviewRequestV1 {
            route: RouteChainV1 {
                start_entity: a,
                segments: Vec::new(),
            },
            equivalent_to: None,
        })?;

        let report = discover_route_preview_from_request_json(&db, &request_json)?;
        assert!(report.anchor_digest.is_none());
        assert_eq!(report.route.normalized.start_entity, a);
        assert_eq!(report.route.normalized.end_entity, a);
        assert!(report.route.normalized.hops.is_empty());
        assert!(report.certificate_preview.is_none());
        Ok(())
    }

    #[test]
    fn route_preview_rejects_unknown_start_entity_for_empty_route() {
        let db = PathDB::new();
        let err = discover_route_preview_from_request(
            &db,
            &RoutePreviewRequestV1 {
                route: RouteChainV1 {
                    start_entity: 999,
                    segments: Vec::new(),
                },
                equivalent_to: None,
            },
        )
        .expect_err("unknown entity should fail");
        assert!(err.to_string().contains("unknown start_entity 999"));
    }

    #[test]
    fn route_preview_rejects_overlong_route() {
        let mut db = PathDB::new();
        let a = db.add_entity("Node", vec![("name", "A")]);
        let b = db.add_entity("Node", vec![("name", "B")]);
        let ab = db.add_relation("road", a, b, 0.9, Vec::new());
        db.build_indexes();
        let err = discover_route_preview_from_request(
            &db,
            &RoutePreviewRequestV1 {
                route: RouteChainV1 {
                    start_entity: a,
                    segments: vec![
                        RouteSegmentRefV1 {
                            relation_id: ab,
                            direction: RouteDirectionV1::Forward,
                        };
                        ROUTE_PREVIEW_MAX_SEGMENTS + 1
                    ],
                },
                equivalent_to: None,
            },
        )
        .expect_err("overlong route should fail");
        assert!(err.to_string().contains("max supported preview length"));
    }
}
