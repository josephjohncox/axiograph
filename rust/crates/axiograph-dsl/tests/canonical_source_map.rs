use axiograph_dsl::axi_v1::{parse_axi_v1, parse_axi_v1_with_source_map};

#[test]
fn canonical_source_map_tracks_parser_occurrences_and_nested_carriers() {
    for newline in ["\n", "\r\n"] {
        let text = [
            "module M",
            "# Compny 😀",
            "schema Earlier:",
            "  object Compny",
            "  relation Previous(value: Compny)",
            "schema Actual:",
            "  object Company",
            "  relation Employment(",
            "    first: refined(Company; eq(Compny)), # Compny",
            "\u{2003} next: indexed(refined(Compny; eq(Compny)); first),",
            "    fact: refined(relation(Employmnt); cardinality(0|1)))",
        ]
        .join(newline);
        let (module, map) = parse_axi_v1_with_source_map(&text).unwrap();
        assert_eq!(
            serde_json::to_vec(&module).unwrap(),
            serde_json::to_vec(&parse_axi_v1(&text).unwrap()).unwrap()
        );
        let spans = &map.role_carriers;
        assert_eq!(spans.len(), 4);
        assert_eq!(
            (
                spans[2].schema_index,
                spans[2].relation_index,
                spans[2].role_index
            ),
            (1, 0, 1)
        );
        assert_eq!(&text[spans[2].bytes.clone()], "Compny");
        assert_eq!(
            spans[2].bytes.start,
            text.find("Compny; eq(Compny)").unwrap()
        );
        assert_eq!(&text[spans[3].bytes.clone()], "Employmnt");
        let wire = serde_json::to_string(&module).unwrap();
        assert!(!wire.contains("role_carriers"));
        assert!(!wire.contains("byte_start"));
    }
}

#[test]
fn canonical_source_map_large_multiline_relation_scales_with_ordered_segments() {
    use std::fmt::Write;

    // The production cursor has at most S advances and R containment checks for
    // S ordered segments and R carriers. Timings supplement that O(S + R) bound;
    // they are not a flaky wall-clock assertion or a reduction of accepted limits.
    for roles in [25_000, 50_000, 100_000] {
        let mut text = String::from("module Large\nschema S:\n  object A\n  relation R(\n");
        let mut expected = Vec::with_capacity(roles);
        for role in 0..roles {
            write!(text, "    role{role}: ").unwrap();
            expected.push(text.len()..text.len() + 1);
            text.push_str(if role + 1 == roles { "A)\n" } else { "A,\n" });
        }
        assert!(text.len() <= 4 * 1024 * 1024);
        assert!(text.lines().count() <= 200_000);
        let start = std::time::Instant::now();
        let (module, map) = parse_axi_v1_with_source_map(&text).unwrap();
        eprintln!(
            "source-map roles={roles} bytes={} elapsed_us={}",
            text.len(),
            start.elapsed().as_micros()
        );
        assert_eq!(module.schemas[0].relations[0].fields.len(), roles);
        assert_eq!(map.role_carriers.len(), roles);
        for (role, (span, expected)) in map.role_carriers.iter().zip(expected).enumerate() {
            assert_eq!(
                (span.schema_index, span.relation_index, span.role_index),
                (0, 0, role)
            );
            assert_eq!(span.bytes, expected);
            assert_eq!(&text[span.bytes.clone()], "A");
        }
        let start = std::time::Instant::now();
        let ordinary = parse_axi_v1(&text).unwrap();
        eprintln!(
            "ordinary-parse roles={roles} elapsed_us={}",
            start.elapsed().as_micros()
        );
        assert_eq!(
            serde_json::to_vec(&module).unwrap(),
            serde_json::to_vec(&ordinary).unwrap()
        );
    }
}
