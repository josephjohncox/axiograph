#!/usr/bin/env python3
"""Check the finite axi_v1 AST, exact-byte, and parser contract."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any


CONTRACT_PATH = Path("fixtures/canonical/contract/axi_v1_contract.json")
RUST_AST_PATH = Path("rust/crates/axiograph-dsl/src/schema_v1.rs")
LEAN_AST_PATH = Path("lean/Axiograph/Axi/SchemaV1.lean")


ASCII_ACCEPTED_SEPARATORS = {
    "space": " ",
    "ht": "\t",
    "lf": "\n",
    "cr": "\r",
}
REJECTED_BOUNDARY_SEPARATORS = {
    "vt": "\v",
    "ff": "\f",
    "nbsp": "\u00a0",
    "em_space": "\u2003",
}
CONSTRAINT_FAMILIES = (
    "functional",
    "at_most",
    "typing",
    "symmetric",
    "transitive",
    "key",
)
RECURSIVE_CONSTRUCTORS = (
    "relation",
    "indexed",
    "refined",
    "eq",
    "in",
    "enum",
    "key",
    "cardinality",
    "predicate",
)
U32_SURFACES = ("cardinality", "at_most")
U32_PAYLOAD_POSITIONS = (
    "cardinality.minimum",
    "cardinality.maximum",
    "at_most.bound",
)
THEORY_KEY_INTERIOR_POSITIONS = (
    "before_open",
    "after_open",
    "before_comma",
    "after_comma",
    "before_close",
)
THEORY_KEY_VARIANTS = ("valid", "invalid_reference", "malformed_empty_field")
SUBTYPE_OPERATORS = ("canonical", "colon_alias", "double_lt", "less_equal", "colon")
SUBTYPE_INCLUSION_VARIANTS = (
    "none",
    "named",
    "missing_name",
    "missing_as",
    "duplicate_as",
)
OBJECT_DECLARATION_SEPARATORS = {
    "canonical_space": "",
    "repeated_space": " ",
    "ht_after_space": "\t",
}
CONSTRAINT_DELIMITER_TEMPLATES = {
    "functional.before_arrow": "functional R.left{separator}-> R.right",
    "functional.after_arrow": "functional R.left ->{separator}R.right",
    "at_most.before_arrow": "at_most 1 R.left{separator}-> R.right",
    "at_most.after_arrow": "at_most 1 R.left ->{separator}R.right",
    "at_most.before_param": "at_most 1 R.left -> R.right{separator}param (left, right)",
    "at_most.param_before_open": "at_most 1 R.left -> R.right param{separator}(left, right)",
    "at_most.param_after_open": "at_most 1 R.left -> R.right param ({separator}left, right)",
    "at_most.param_before_comma": "at_most 1 R.left -> R.right param (left{separator}, right)",
    "at_most.param_after_comma": "at_most 1 R.left -> R.right param (left,{separator}right)",
    "at_most.param_before_close": "at_most 1 R.left -> R.right param (left, right{separator})",
    "typing.before_colon": "typing R{separator}: rule_name",
    "typing.after_colon": "typing R:{separator}rule_name",
    "symmetric.before_where": "symmetric R{separator}where R.left in {{A, B}} on (left, right) param (left)",
    "symmetric.after_where": "symmetric R where{separator}R.left in {{A, B}} on (left, right) param (left)",
    "symmetric.before_in": "symmetric R where R.left{separator}in {{A, B}} on (left, right) param (left)",
    "symmetric.after_in": "symmetric R where R.left in{separator}{{A, B}} on (left, right) param (left)",
    "symmetric.set_after_open": "symmetric R where R.left in {{{separator}A, B}} on (left, right) param (left)",
    "symmetric.set_before_comma": "symmetric R where R.left in {{A{separator}, B}} on (left, right) param (left)",
    "symmetric.set_after_comma": "symmetric R where R.left in {{A,{separator}B}} on (left, right) param (left)",
    "symmetric.set_before_close": "symmetric R where R.left in {{A, B{separator}}} on (left, right) param (left)",
    "symmetric.before_on": "symmetric R where R.left in {{A, B}}{separator}on (left, right) param (left)",
    "symmetric.on_before_open": "symmetric R where R.left in {{A, B}} on{separator}(left, right) param (left)",
    "symmetric.on_after_open": "symmetric R where R.left in {{A, B}} on ({separator}left, right) param (left)",
    "symmetric.on_before_comma": "symmetric R where R.left in {{A, B}} on (left{separator}, right) param (left)",
    "symmetric.on_after_comma": "symmetric R where R.left in {{A, B}} on (left,{separator}right) param (left)",
    "symmetric.on_before_close": "symmetric R where R.left in {{A, B}} on (left, right{separator}) param (left)",
    "symmetric.before_param": "symmetric R where R.left in {{A, B}} on (left, right){separator}param (left)",
    "symmetric.param_before_open": "symmetric R where R.left in {{A, B}} on (left, right) param{separator}(left)",
    "symmetric.param_after_open": "symmetric R where R.left in {{A, B}} on (left, right) param ({separator}left)",
    "symmetric.param_before_close": "symmetric R where R.left in {{A, B}} on (left, right) param (left{separator})",
    "transitive.before_on": "transitive R{separator}on (left, right) param (left)",
    "transitive.on_before_open": "transitive R on{separator}(left, right) param (left)",
    "transitive.on_after_open": "transitive R on ({separator}left, right) param (left)",
    "transitive.on_before_comma": "transitive R on (left{separator}, right) param (left)",
    "transitive.on_after_comma": "transitive R on (left,{separator}right) param (left)",
    "transitive.on_before_close": "transitive R on (left, right{separator}) param (left)",
    "transitive.before_param": "transitive R on (left, right){separator}param (left)",
    "transitive.param_before_open": "transitive R on (left, right) param{separator}(left)",
    "transitive.param_after_open": "transitive R on (left, right) param ({separator}left, right)",
    "transitive.param_before_comma": "transitive R on (left, right) param (left{separator}, right)",
    "transitive.param_after_comma": "transitive R on (left, right) param (left,{separator}right)",
    "transitive.param_before_close": "transitive R on (left, right) param (left, right{separator})",
    "symmetric.param_before_comma": "symmetric R where R.left in {{A, B}} on (left, right) param (left{separator}, right)",
    "symmetric.param_after_comma": "symmetric R where R.left in {{A, B}} on (left, right) param (left,{separator}right)",
}
CONSTRAINT_QUALIFIED_FIELD_TEMPLATES = {
    "functional.source_before_dot": "functional R{separator}.left -> R.right",
    "functional.source_after_dot": "functional R.{separator}left -> R.right",
    "functional.target_before_dot": "functional R.left -> R{separator}.right",
    "functional.target_after_dot": "functional R.left -> R.{separator}right",
    "at_most.source_before_dot": "at_most 1 R{separator}.left -> R.right",
    "at_most.source_after_dot": "at_most 1 R.{separator}left -> R.right",
    "at_most.target_before_dot": "at_most 1 R.left -> R{separator}.right",
    "at_most.target_after_dot": "at_most 1 R.left -> R.{separator}right",
    "symmetric.guard_before_dot": "symmetric R where R{separator}.left in {{A, B}}",
    "symmetric.guard_after_dot": "symmetric R where R.{separator}left in {{A, B}}",
}
RECURSIVE_CONSTRUCTOR_TEMPLATES = {
    "relation": {
        "after_open": "relation({separator}Edge)",
        "before_close": "relation(Edge{separator})",
    },
    "indexed": {
        "after_open": "indexed({separator}A; base|other)",
        "before_semicolon": "indexed(A{separator}; base|other)",
        "after_semicolon": "indexed(A;{separator}base|other)",
        "before_pipe": "indexed(A; base{separator}|other)",
        "after_pipe": "indexed(A; base|{separator}other)",
        "before_close": "indexed(A; base|other{separator})",
    },
    "refined": {
        "after_open": "refined({separator}A; eq(A); enum(A|A))",
        "before_semicolon": "refined(A{separator}; eq(A); enum(A|A))",
        "after_semicolon": "refined(A;{separator}eq(A); enum(A|A))",
        "before_close": "refined(A; eq(A); enum(A|A){separator})",
    },
    "eq": {
        "after_open": "refined(A; eq({separator}A))",
        "before_close": "refined(A; eq(A{separator}))",
    },
    "in": {
        "after_open": "refined(A; in({separator}A|A))",
        "before_pipe": "refined(A; in(A{separator}|A))",
        "after_pipe": "refined(A; in(A|{separator}A))",
        "before_close": "refined(A; in(A|A{separator}))",
    },
    "enum": {
        "after_open": "refined(A; enum({separator}A|A))",
        "before_pipe": "refined(A; enum(A{separator}|A))",
        "after_pipe": "refined(A; enum(A|{separator}A))",
        "before_close": "refined(A; enum(A|A{separator}))",
    },
    "key": {
        "after_open": "refined(A; key({separator}base|other))",
        "before_pipe": "refined(A; key(base{separator}|other))",
        "after_pipe": "refined(A; key(base|{separator}other))",
        "before_close": "refined(A; key(base|other{separator}))",
    },
    "cardinality": {
        "after_open": "refined(A; cardinality({separator}0|1))",
        "before_pipe": "refined(A; cardinality(0{separator}|1))",
        "after_pipe": "refined(A; cardinality(0|{separator}1))",
        "before_close": "refined(A; cardinality(0|1{separator}))",
    },
    "predicate": {
        "after_open": "refined(A; predicate({separator}non_empty|base))",
        "before_pipe": "refined(A; predicate(non_empty{separator}|base))",
        "after_pipe": "refined(A; predicate(non_empty|{separator}base))",
        "before_close": "refined(A; predicate(non_empty|base{separator}))",
    },
}
U32_VARIANTS = {
    "empty": ("", False),
    "ascii_digits": ("1", True),
    "leading_plus": ("+1", False),
    "leading_minus": ("-1", False),
    "interior_whitespace": ("1 0", False),
    "unicode_digit": ("١", False),
    "maximum": ("4294967295", True),
    "overflow": ("4294967296", False),
}

# This is the complete taxonomy mechanized by this checker. A class is present
# only when both parser/typechecker executables can independently expose the
# same rejection stage and the checker can classify their real diagnostics.
CROSS_LANGUAGE_REJECTION_STAGES = {
    "boundary.invalid_utf8": "boundary",
    "parse.resource_depth": "parse",
    "parse.delimiter": "parse",
    "parse.header_order": "parse",
    "parse.section_syntax": "parse",
    "parse.relation_axis_shorthand": "parse",
    "parse.role_kind": "parse",
    "parse.constraint_shape": "parse",
    "parse.rewrite_shape": "parse",
    "parse.numeric_domain": "parse",
    "parse.noncanonical_alias": "parse",
    "parse.list_syntax": "parse",
    "parse.lexical_domain": "parse",
    "typecheck.duplicate_declaration": "typecheck",
    "typecheck.schema_formation": "typecheck",
    "typecheck.dependent_role": "typecheck",
    "typecheck.refinement": "typecheck",
    "typecheck.instance_formation": "typecheck",
    "typecheck.theory_formation": "typecheck",
}
NON_CROSS_LANGUAGE_OUTCOME_CLASSES = (
    "formation.import_closure",
    "formation.unsupported_refinement_witness",
    "formation.finite_model",
    "formation.unsupported_equation_fragment",
    "unsupported.opaque_constraint",
    "unsupported.named_constraint_semantics",
    "non_claim",
)


def differential_envelope_contract() -> dict[str, Any]:
    """Return the closed binary-envelope contract used by the differential runner."""
    return {
        "schema": "axiograph.axi_v1_differential_envelope",
        "version": 1,
        "flag": "--contract-envelope-v1",
        "implementations": ["rust", "lean"],
        "requested_stages": ["parse", "typecheck", "formation"],
        "fields": [
            "schema",
            "version",
            "implementation",
            "requested_stage",
            "observed_stage",
            "decision",
            "normalized_ast",
            "summary",
            "rejection_class",
            "diagnostic",
        ],
        "decisions": ["accepted", "rejected"],
        "closed_value_shapes": {
            "version": "integer_literal_1_not_boolean",
            "normalized_ast": "closed_shape_from_ast_inventory_and_ast_payload_types_or_null_by_stage",
            "typecheck_summary": {
                "module": "string",
                "schemas": "nonnegative_integer_not_boolean",
                "theories": "nonnegative_integer_not_boolean",
                "instances": "nonnegative_integer_not_boolean",
                "assignments": "nonnegative_integer_not_boolean",
                "tuples": "nonnegative_integer_not_boolean",
            },
            "formation_summary": {
                "module": "string",
                "import_closure": "nonnegative_integer_not_boolean",
                "schemas": "nonnegative_integer_not_boolean",
                "theories": "nonnegative_integer_not_boolean",
                "instances": "nonnegative_integer_not_boolean",
            },
        },
        "parser_and_typechecker_self_classify_rejections": False,
        "runner_derives_cross_language_class_from_stage_diagnostic_and_source": True,
        "formation_is_rust_decision_procedure_not_lean_proof": True,
        "normalized_ast_input_authority": False,
        "rust_output_authoritative_for_lean": False,
    }


def diagnostic_taxonomy_contract() -> dict[str, Any]:
    """Return the closed stage/class contract checked from process observations."""
    return {
        "version": 1,
        "scope": (
            "class, rejection-stage, and source-specific diagnostic-reason parity derived "
            "from each language's exit status, stderr family, and exact source stimulus; "
            "diagnostic strings need not be equal"
        ),
        "closed_stage_class_mapping": [
            {"class": rejection_class, "stage": stage}
            for rejection_class, stage in CROSS_LANGUAGE_REJECTION_STAGES.items()
        ],
        "non_cross_language_outcome_classes": list(NON_CROSS_LANGUAGE_OUTCOME_CLASSES),
    }


def boundary_coverage_requirements() -> dict[str, Any]:
    """Return the exact Cartesian dimensions owned by the contract gate."""
    return {
        "object_declaration_separator": {
            "forms": list(OBJECT_DECLARATION_SEPARATORS),
            "accepted": ["canonical_space"],
            "rule": "exactly one ASCII space between object and its identifier",
        },
        "constraint_family_separator": {
            "families": list(CONSTRAINT_FAMILIES),
            "separators": ["space", "ht", "lf", "cr", "vt", "ff", "nbsp", "em_space"],
            "accepted": ["space", "ht", "lf", "cr"],
            "variants": ["valid", "invalid_reference", "malformed_shape"],
        },
        "u32_token": {
            "surfaces": list(U32_SURFACES),
            "payload_positions": list(U32_PAYLOAD_POSITIONS),
            "variants": list(U32_VARIANTS),
        },
        "theory_key_interior_whitespace": {
            "positions": list(THEORY_KEY_INTERIOR_POSITIONS),
            "separators": ["space", "ht", "lf", "cr", "vt", "ff", "nbsp", "em_space"],
            "accepted": ["space", "ht", "lf", "cr"],
            "variants": list(THEORY_KEY_VARIANTS),
            "distinct_from": "recursive refinement key(...) role list",
        },
        "constraint_delimiter_whitespace": {
            "positions": list(CONSTRAINT_DELIMITER_TEMPLATES),
            "separators": ["space", "ht", "lf", "cr", "vt", "ff", "nbsp", "em_space"],
            "accepted": ["space", "ht", "lf", "cr"],
        },
        "constraint_qualified_field_adjacency": {
            "positions": list(CONSTRAINT_QUALIFIED_FIELD_TEMPLATES),
            "separators": ["space", "ht", "lf", "cr", "vt", "ff", "nbsp", "em_space"],
            "accepted": [],
        },
        "subtype_surface": {
            "operators": list(SUBTYPE_OPERATORS),
            "inclusion_variants": list(SUBTYPE_INCLUSION_VARIANTS),
            "accepted": ["canonical.none", "canonical.named"],
        },
        "rewrite_path_separator": {
            "forms": ["parenthesized", "words"],
            "separators": ["space", "ht", "cr", "vt", "ff", "nbsp", "em_space"],
            "accepted": ["space", "ht", "cr"],
            "line_feed_applicability": "rewrite vars are one physical line; LF terminates the vars field",
        },
        "recursive_constructor_boundary": {
            "constructors": {
                constructor: list(RECURSIVE_CONSTRUCTOR_TEMPLATES[constructor])
                for constructor in RECURSIVE_CONSTRUCTORS
            },
            "separators": ["space", "ht", "lf", "cr", "vt", "ff", "nbsp", "em_space"],
            "accepted": ["space", "ht", "lf", "cr"],
        },
    }


def generated_boundary_cases() -> list[dict[str, Any]]:
    """Generate every required lexical/parser boundary cell deterministically."""
    cases: list[dict[str, Any]] = []
    schema = (
        "module M\nschema S\n  object A\n"
        "  relation R(left:A,right:A)\ntheory T on S\n"
    )
    for form, additional_separator in OBJECT_DECLARATION_SEPARATORS.items():
        accepted = form == "canonical_space"
        case = {
            "id": f"object-declaration-{form}",
            "dimensions": ["object-declaration-separator", form],
            "coverage": {
                "axis": "object_declaration_separator",
                "form": form,
            },
            "source": f"module M\nschema S\n  object {additional_separator}A\n",
            "parse": accepted,
            "normalized_ast": "compare" if accepted else "reject",
            "class": "accepted" if accepted else "parse.section_syntax",
        }
        if accepted:
            case["typecheck"] = True
        cases.append(case)

    valid_tails = {
        "functional": "R.left -> R.right",
        "at_most": "1 R.left -> R.right",
        "typing": "R: rule_name",
        "symmetric": "R",
        "transitive": "R",
        "key": "R(left)",
    }
    invalid_reference_tails = {
        "functional": "Missing.left -> Missing.right",
        "at_most": "1 Missing.left -> Missing.right",
        "typing": "Missing: rule_name",
        "symmetric": "Missing",
        "transitive": "Missing",
        "key": "Missing(left)",
    }
    malformed_tails = {
        "functional": "R.left ->",
        "at_most": "1 R.left ->",
        "typing": "R:",
        "symmetric": "R where",
        "transitive": "R extra",
        "key": "R()",
    }
    for family in CONSTRAINT_FAMILIES:
        for separator_name in ("space", "ht", "lf", "cr", "vt", "ff", "nbsp", "em_space"):
            separator = {
                **ASCII_ACCEPTED_SEPARATORS,
                **REJECTED_BOUNDARY_SEPARATORS,
            }[separator_name]
            separator_is_accepted = separator_name in ASCII_ACCEPTED_SEPARATORS
            for variant, tails, accepted_parse, accepted_typecheck, accepted_class in (
                ("valid", valid_tails, True, True, "accepted"),
                (
                    "invalid_reference",
                    invalid_reference_tails,
                    True,
                    False,
                    "typecheck.theory_formation",
                ),
                (
                    "malformed_shape",
                    malformed_tails,
                    False,
                    None,
                    "parse.constraint_shape",
                ),
            ):
                parse = accepted_parse if separator_is_accepted else False
                typecheck = accepted_typecheck if separator_is_accepted else None
                outcome_class = (
                    accepted_class if separator_is_accepted else "parse.lexical_domain"
                )
                case: dict[str, Any] = {
                    "id": f"constraint-{family}-{separator_name}-{variant}",
                    "dimensions": [
                        "constraint-family-separator",
                        family,
                        separator_name,
                        variant,
                    ],
                    "coverage": {
                        "axis": "constraint_family_separator",
                        "family": family,
                        "separator": separator_name,
                        "variant": variant,
                    },
                    "source": schema
                    + f"  constraint {family}{separator}{tails[family]}\n",
                    "parse": parse,
                    "class": outcome_class,
                }
                if typecheck is not None:
                    case["typecheck"] = typecheck
                cases.append(case)

    for position in THEORY_KEY_INTERIOR_POSITIONS:
        for separator_name in ("space", "ht", "lf", "cr", "vt", "ff", "nbsp", "em_space"):
            separator = {
                **ASCII_ACCEPTED_SEPARATORS,
                **REJECTED_BOUNDARY_SEPARATORS,
            }[separator_name]
            separator_is_accepted = separator_name in ASCII_ACCEPTED_SEPARATORS
            for variant in THEORY_KEY_VARIANTS:
                relation = "Missing" if variant == "invalid_reference" else "R"
                fields = "left,,right" if variant == "malformed_empty_field" else "left,right"
                fragments = {
                    "before_open": f"key {relation}{{separator}}({fields})",
                    "after_open": f"key {relation}({{separator}}{fields})",
                    "before_comma": f"key {relation}(left{{separator}},{'right' if variant != 'malformed_empty_field' else ',right'})",
                    "after_comma": f"key {relation}(left,{{separator}}{'right' if variant != 'malformed_empty_field' else ',right'})",
                    "before_close": f"key {relation}({fields}{{separator}})",
                }
                tail = fragments[position].format(separator=separator)
                parse = separator_is_accepted and variant != "malformed_empty_field"
                typecheck = variant != "invalid_reference" if parse else None
                if not separator_is_accepted:
                    outcome_class = "parse.lexical_domain"
                elif variant == "malformed_empty_field":
                    outcome_class = "parse.list_syntax"
                elif variant == "invalid_reference":
                    outcome_class = "typecheck.theory_formation"
                else:
                    outcome_class = "accepted"
                case = {
                    "id": f"theory-key-{position}-{separator_name}-{variant}",
                    "dimensions": [
                        "theory-key-interior-whitespace",
                        position,
                        separator_name,
                        variant,
                    ],
                    "coverage": {
                        "axis": "theory_key_interior_whitespace",
                        "position": position,
                        "separator": separator_name,
                        "variant": variant,
                    },
                    "source": schema + f"  constraint {tail}\n",
                    "parse": parse,
                    "class": outcome_class,
                }
                if typecheck is not None:
                    case["typecheck"] = typecheck
                cases.append(case)

    for position, template in CONSTRAINT_DELIMITER_TEMPLATES.items():
        for separator_name in ("space", "ht", "lf", "cr", "vt", "ff", "nbsp", "em_space"):
            separator = {
                **ASCII_ACCEPTED_SEPARATORS,
                **REJECTED_BOUNDARY_SEPARATORS,
            }[separator_name]
            accepted = separator_name in ASCII_ACCEPTED_SEPARATORS
            tail = template.format(separator=separator)
            cases.append(
                {
                    "id": f"constraint-delimiter-{position.replace('.', '-')}-{separator_name}",
                    "dimensions": [
                        "constraint-delimiter-whitespace",
                        position,
                        separator_name,
                    ],
                    "coverage": {
                        "axis": "constraint_delimiter_whitespace",
                        "position": position,
                        "separator": separator_name,
                    },
                    "source": schema + f"  constraint {tail}\n",
                    "parse": accepted,
                    "typecheck": True if accepted else None,
                    "class": "accepted" if accepted else "parse.lexical_domain",
                }
            )
            if not accepted:
                cases[-1].pop("typecheck")

    for position, template in CONSTRAINT_QUALIFIED_FIELD_TEMPLATES.items():
        for separator_name in ("space", "ht", "lf", "cr", "vt", "ff", "nbsp", "em_space"):
            separator = {
                **ASCII_ACCEPTED_SEPARATORS,
                **REJECTED_BOUNDARY_SEPARATORS,
            }[separator_name]
            cases.append(
                {
                    "id": f"constraint-qualified-{position.replace('.', '-')}-{separator_name}",
                    "dimensions": [
                        "constraint-qualified-field-adjacency",
                        position,
                        separator_name,
                    ],
                    "coverage": {
                        "axis": "constraint_qualified_field_adjacency",
                        "position": position,
                        "separator": separator_name,
                    },
                    "source": schema + f"  constraint {template.format(separator=separator)}\n",
                    "parse": False,
                    "class": (
                        "parse.constraint_shape"
                        if separator_name in ASCII_ACCEPTED_SEPARATORS
                        else "parse.lexical_domain"
                    ),
                }
            )

    subtype_operators = {
        "canonical": "<",
        "colon_alias": "<:",
        "double_lt": "<<",
        "less_equal": "<=",
        "colon": ":",
    }
    subtype_suffixes = {
        "none": "",
        "named": " as child_to_parent",
        "missing_name": " as",
        "missing_as": " child_to_parent",
        "duplicate_as": " as child_to_parent as duplicate",
    }
    for operator_name, operator in subtype_operators.items():
        for inclusion_variant, suffix in subtype_suffixes.items():
            accepted = operator_name == "canonical" and inclusion_variant in {"none", "named"}
            outcome_class = (
                "accepted"
                if accepted
                else "parse.noncanonical_alias"
                if operator_name == "colon_alias"
                else "parse.section_syntax"
            )
            case = {
                "id": f"subtype-{operator_name}-{inclusion_variant}",
                "dimensions": ["subtype-surface", operator_name, inclusion_variant],
                "coverage": {
                    "axis": "subtype_surface",
                    "operator": operator_name,
                    "inclusion_variant": inclusion_variant,
                },
                "source": (
                    "module M\nschema S\n  object Child\n  object Parent\n"
                    f"  subtype Child {operator} Parent{suffix}\n"
                ),
                "parse": accepted,
                "class": outcome_class,
            }
            if accepted:
                case["typecheck"] = True
            cases.append(case)

    for payload_position in U32_PAYLOAD_POSITIONS:
        surface = payload_position.split(".", 1)[0]
        for variant, (token, accepted) in U32_VARIANTS.items():
            if payload_position == "cardinality.minimum":
                source = (
                    "module M\nschema S\n  object A\n"
                    f"  relation R(value:refined(A;cardinality({token}|4294967295)))\n"
                )
            elif payload_position == "cardinality.maximum":
                source = (
                    "module M\nschema S\n  object A\n"
                    f"  relation R(value:refined(A;cardinality(0|{token})))\n"
                )
            else:
                source = schema + f"  constraint at_most {token} R.left -> R.right\n"
            position_id = payload_position.replace(".", "-")
            case = {
                "id": f"u32-{position_id}-{variant}",
                "dimensions": ["u32-token", surface, payload_position, variant],
                "coverage": {
                    "axis": "u32_token",
                    "surface": surface,
                    "payload_position": payload_position,
                    "variant": variant,
                },
                "source": source,
                "parse": accepted,
                "class": "accepted" if accepted else "parse.numeric_domain",
            }
            if accepted:
                case["typecheck"] = True
            cases.append(case)

    for form in ("parenthesized", "words"):
        for separator_name in ("space", "ht", "cr", "vt", "ff", "nbsp", "em_space"):
            separator = {
                **ASCII_ACCEPTED_SEPARATORS,
                **REJECTED_BOUNDARY_SEPARATORS,
            }[separator_name]
            accepted = separator_name in {"space", "ht", "cr"}
            path_type = (
                f"Path{separator}(x,y)"
                if form == "parenthesized"
                else f"Path{separator}x y"
            )
            case = {
                "id": f"rewrite-path-{form}-{separator_name}",
                "dimensions": ["rewrite-path-separator", form, separator_name],
                "coverage": {
                    "axis": "rewrite_path_separator",
                    "form": form,
                    "separator": separator_name,
                },
                "source": (
                    "module M\nschema S\n  object A\ntheory T on S\n  rewrite r\n"
                    f"    vars: x: A, y: A, p: {path_type}\n"
                    "    lhs: p\n    rhs: p\n"
                ),
                "parse": accepted,
                "class": "accepted" if accepted else "parse.lexical_domain",
            }
            if accepted:
                case["typecheck"] = True
            cases.append(case)

    for constructor in RECURSIVE_CONSTRUCTORS:
        for position, template in RECURSIVE_CONSTRUCTOR_TEMPLATES[constructor].items():
            for separator_name in ("space", "ht", "lf", "cr", "vt", "ff", "nbsp", "em_space"):
                separator = {
                    **ASCII_ACCEPTED_SEPARATORS,
                    **REJECTED_BOUNDARY_SEPARATORS,
                }[separator_name]
                accepted = separator_name in ASCII_ACCEPTED_SEPARATORS
                role_type = template.format(separator=separator)
                case = {
                    "id": f"recursive-{constructor}-{position}-{separator_name}",
                    "dimensions": [
                        "recursive-constructor-boundary",
                        constructor,
                        position,
                        separator_name,
                    ],
                    "coverage": {
                        "axis": "recursive_constructor_boundary",
                        "constructor": constructor,
                        "position": position,
                        "separator": separator_name,
                    },
                    "source": (
                        "module M\nschema S\n  object A\n"
                        "  relation Edge(value:A)\n"
                        f"  relation Box(base:A, other:A, value:{role_type})\n"
                    ),
                    "parse": accepted,
                    "class": "accepted" if accepted else "parse.lexical_domain",
                }
                if accepted:
                    case["typecheck"] = True
                cases.append(case)
    return cases


def validate_boundary_coverage(matrix: dict[str, Any]) -> list[dict[str, Any]]:
    """Reject any omitted, duplicated, relabeled, or edited Cartesian cell."""
    manual_cases = matrix.get("cases")
    boundary_cases = matrix.get("boundary_cases")
    if not isinstance(manual_cases, list) or not manual_cases:
        fail("adversarial matrix cases must be a non-empty array")
    if matrix.get("coverage_requirements") != boundary_coverage_requirements():
        fail("adversarial matrix Cartesian coverage requirements drift")
    expected_boundary_cases = generated_boundary_cases()
    if boundary_cases != expected_boundary_cases:
        fail(
            "adversarial matrix boundary_cases must equal the complete generated "
            "Cartesian coverage; regenerate instead of omitting or hand-editing cells"
        )
    generated_ids = {case["id"] for case in expected_boundary_cases}
    manual_ids = {
        case.get("id") for case in manual_cases if isinstance(case, dict)
    }
    if generated_ids & manual_ids:
        fail("manual matrix cases must not duplicate generated Cartesian cells")
    return [*manual_cases, *expected_boundary_cases]


def fail(message: str) -> None:
    raise ValueError(message)


def read_json(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        fail(f"{path}: expected a JSON object")
    return value


def matching_brace(text: str, opening: int) -> int:
    depth = 0
    for index in range(opening, len(text)):
        if text[index] == "{":
            depth += 1
        elif text[index] == "}":
            depth -= 1
            if depth == 0:
                return index
    fail("unclosed Rust declaration")
    return 0


def rust_inventory(path: Path) -> dict[str, dict[str, Any]]:
    text = path.read_text(encoding="utf-8")
    start = text.index("// AST")
    end = text.index("// Parser")
    ast = text[start:end]
    result: dict[str, dict[str, Any]] = {}
    for match in re.finditer(r"^pub (struct|enum) ([A-Za-z0-9_]+)\s*\{", ast, re.MULTILINE):
        kind, name = match.group(1), match.group(2)
        opening = ast.index("{", match.start())
        body = ast[opening + 1 : matching_brace(ast, opening)]
        if kind == "struct":
            fields = re.findall(r"^\s{4}pub ([A-Za-z0-9_]+)\s*:", body, re.MULTILINE)
            result[name] = {"kind": kind, "fields": fields}
            continue
        constructors: list[str] = []
        depth = 0
        for line in body.splitlines():
            if depth == 0:
                variant = re.match(r"^\s{4}([A-Z][A-Za-z0-9_]*)\b", line)
                if variant:
                    constructors.append(variant.group(1))
            depth += line.count("{") - line.count("}")
        result[name] = {"kind": kind, "constructors": constructors}
    return result


def lean_inventory(path: Path) -> dict[str, dict[str, Any]]:
    text = path.read_text(encoding="utf-8")
    start = text.index("-- AST")
    end = text.index("-- Parser utilities")
    ast = text[start:end]
    declarations = list(
        re.finditer(r"^(structure|inductive) ([A-Za-z0-9_]+) where\n", ast, re.MULTILINE)
    )
    result: dict[str, dict[str, Any]] = {}
    for index, match in enumerate(declarations):
        kind, name = match.group(1), match.group(2)
        stop = declarations[index + 1].start() if index + 1 < len(declarations) else len(ast)
        body = ast[match.end() : stop]
        if kind == "structure":
            fields = re.findall(r"^  ([a-zA-Z][A-Za-z0-9_]*)\s*:", body, re.MULTILINE)
            result[name] = {"kind": kind, "fields": fields}
        else:
            constructors = re.findall(r"^  \| ([a-zA-Z][A-Za-z0-9_]*)\b", body, re.MULTILINE)
            result[name] = {"kind": kind, "constructors": constructors}
    return result


def unwrap_parenthesized(value: str) -> str:
    value = value.strip()
    while value.startswith("(") and value.endswith(")"):
        depth = 0
        encloses_complete_value = True
        for index, char in enumerate(value):
            if char == "(":
                depth += 1
            elif char == ")":
                depth -= 1
                if depth == 0 and index != len(value) - 1:
                    encloses_complete_value = False
                    break
        if not encloses_complete_value:
            break
        value = value[1:-1].strip()
    return value


def split_top_level(value: str, separator: str) -> list[str]:
    depths = {"(": 0, "[": 0, "{": 0, "<": 0}
    closing = {")": "(", "]": "[", "}": "{", ">": "<"}
    parts: list[str] = []
    start = 0
    for index, char in enumerate(value):
        if char in depths:
            depths[char] += 1
        elif char in closing:
            depths[closing[char]] -= 1
        elif char == separator and not any(depths.values()):
            parts.append(value[start:index].strip())
            start = index + 1
    parts.append(value[start:].strip())
    return parts


def normalize_source_type(
    language: str, source_type: str, declaration_names: dict[str, str]
) -> str:
    value = source_type.strip()
    if language == "lean":
        value = value.split(":=", 1)[0].strip()
        value = unwrap_parenthesized(value)
        product = split_top_level(value, "×")
        if len(product) == 2:
            return "pair<" + ",".join(
                normalize_source_type(language, item, declaration_names) for item in product
            ) + ">"
        for prefix, canonical in (("Array ", "ordered"), ("Option ", "optional")):
            if value.startswith(prefix):
                inner = unwrap_parenthesized(value[len(prefix):])
                return f"{canonical}<{normalize_source_type(language, inner, declaration_names)}>"
        primitives = {"Name": "name", "String": "utf8_text", "Bool": "bool", "UInt32": "u32"}
    else:
        value = " ".join(value.split())
        for prefix, canonical in (("Vec<", "ordered"), ("Option<", "optional"), ("Box<", "boxed")):
            if value.startswith(prefix) and value.endswith(">"):
                inner = value[len(prefix):-1]
                return f"{canonical}<{normalize_source_type(language, inner, declaration_names)}>"
        if value.startswith("(") and value.endswith(")"):
            product = split_top_level(value[1:-1], ",")
            if len(product) == 2:
                return "pair<" + ",".join(
                    normalize_source_type(language, item, declaration_names) for item in product
                ) + ">"
        primitives = {"Name": "name", "String": "utf8_text", "bool": "bool", "u32": "u32"}
    if value in primitives:
        return primitives[value]
    if value in declaration_names:
        return declaration_names[value]
    fail(f"{language}: unsupported AST source payload type {source_type!r}")
    return ""


def lean_binder_groups(rest: str) -> list[str]:
    groups: list[str] = []
    index = 0
    while index < len(rest):
        if rest[index].isspace():
            index += 1
            continue
        if rest[index] != "(":
            fail(f"Lean inductive constructor has unsupported payload syntax: {rest!r}")
        depth = 1
        start = index + 1
        index += 1
        while index < len(rest) and depth:
            if rest[index] == "(":
                depth += 1
            elif rest[index] == ")":
                depth -= 1
            index += 1
        if depth:
            fail(f"Lean inductive constructor has unclosed payload: {rest!r}")
        groups.append(rest[start:index - 1].strip())
    return groups


def source_payload_inventory(
    contract: dict[str, Any], root: Path, language: str
) -> dict[str, Any]:
    inventory = contract["ast_inventory"]
    declaration_names = {
        entry[language]["declaration"]: entry["canonical"] for entry in inventory
    }
    path = RUST_AST_PATH if language == "rust" else LEAN_AST_PATH
    text = (root / path).read_text(encoding="utf-8")
    start_marker, end_marker = (
        ("// AST", "// Parser") if language == "rust" else ("-- AST", "-- Parser utilities")
    )
    ast = text[text.index(start_marker):text.index(end_marker)]
    declarations = list(
        re.finditer(
            r"^(?:pub (struct|enum)|(structure|inductive)) ([A-Za-z0-9_]+)(?:\s*\{| where\n)",
            ast,
            re.MULTILINE,
        )
    )
    result: dict[str, Any] = {}
    for position, match in enumerate(declarations):
        kind = match.group(1) or match.group(2)
        name = match.group(3)
        stop = declarations[position + 1].start() if position + 1 < len(declarations) else len(ast)
        block = ast[match.end():stop]
        if kind in {"struct", "structure"}:
            if language == "rust":
                fields = re.findall(
                    r"^\s{4}pub ([A-Za-z][A-Za-z0-9_]*)\s*:\s*(.+),$",
                    block,
                    re.MULTILINE,
                )
            else:
                fields = re.findall(
                    r"^  ([A-Za-z][A-Za-z0-9_]*)\s*:\s*(.+)$", block, re.MULTILINE
                )
            result[name] = {
                "kind": kind,
                "fields": [field for field, _ in fields],
                "payload_types": [
                    normalize_source_type(language, source_type, declaration_names)
                    for _, source_type in fields
                ],
            }
            continue

        constructors: dict[str, list[str]] = {}
        if language == "rust":
            variants = list(re.finditer(r"^\s{4}([A-Z][A-Za-z0-9_]*)\b", block, re.MULTILINE))
            for variant_index, variant in enumerate(variants):
                variant_name = variant.group(1)
                variant_stop = (
                    variants[variant_index + 1].start()
                    if variant_index + 1 < len(variants)
                    else len(block)
                )
                variant_text = block[variant.end():variant_stop]
                opening = variant_text.find("{")
                if opening < 0:
                    payload_fields: list[tuple[str, str]] = []
                else:
                    body = variant_text[opening + 1:matching_brace(variant_text, opening)]
                    clean = "\n".join(
                        line for line in body.splitlines()
                        if not line.strip().startswith(("///", "#["))
                    )
                    payload_fields = []
                    for field in split_top_level(clean, ","):
                        field = field.strip()
                        if not field:
                            continue
                        field_name, separator, source_type = field.partition(":")
                        if not separator:
                            fail(f"Rust enum payload field has no type: {field!r}")
                        payload_fields.append((field_name.strip(), source_type.strip()))
                constructors[variant_name] = [
                    normalize_source_type(language, source_type, declaration_names)
                    for _, source_type in payload_fields
                ]
        else:
            for constructor in re.finditer(
                r"^  \| ([a-zA-Z][A-Za-z0-9_]*)(.*)$", block, re.MULTILINE
            ):
                payload_types: list[str] = []
                for group in lean_binder_groups(constructor.group(2)):
                    if ":" not in group:
                        fail(f"Lean constructor binder must declare a type: {group!r}")
                    names, source_type = group.split(":", 1)
                    normalized = normalize_source_type(language, source_type, declaration_names)
                    payload_types.extend(normalized for _ in names.split())
                constructors[constructor.group(1)] = payload_types
        result[name] = {"kind": kind, "constructors": constructors}
    return result


def payload_types_match(language: str, expected: str, actual: str) -> bool:
    return expected == actual or (
        language == "lean" and expected.startswith("boxed<") and expected[6:-1] == actual
    )


def check_payload_contract(contract: dict[str, Any], root: Path) -> None:
    inventory = contract["ast_inventory"]
    payloads = contract.get("ast_payload_types")
    if not isinstance(payloads, list) or not payloads:
        fail("contract ast_payload_types must be a non-empty array")
    by_name = {entry.get("canonical"): entry for entry in payloads}
    inventory_names = {entry["canonical"] for entry in inventory}
    if len(by_name) != len(payloads) or set(by_name) != inventory_names:
        fail("ast_payload_types must cover every AST declaration exactly once")

    u32_uses: set[str] = set()
    type_pattern = re.compile(r"^[a-z0-9_]+(?:<.+>)?$")
    for declaration in inventory:
        canonical = declaration["canonical"]
        payload = by_name[canonical]
        if declaration["rust"]["kind"] == "struct":
            fields = payload.get("fields")
            if not isinstance(fields, dict) or list(fields) != declaration["rust"]["fields"]:
                fail(f"{canonical}: payload fields must match the declared Rust field order")
            members = fields
        else:
            constructors = payload.get("constructors")
            tags = declaration.get("wire_tags")
            if not isinstance(constructors, dict) or tags is None or list(constructors) != tags:
                fail(f"{canonical}: payload constructors must match every wire tag in order")
            if not all(isinstance(fields, dict) for fields in constructors.values()):
                fail(f"{canonical}: every constructor payload must be an object")
            members = {
                f"{constructor}.{field}": ty
                for constructor, fields in constructors.items()
                for field, ty in fields.items()
            }
        for member, payload_type in members.items():
            if not isinstance(payload_type, str) or not type_pattern.fullmatch(payload_type):
                fail(f"{canonical}.{member}: invalid payload type {payload_type!r}")
            if payload_type == "u32":
                u32_uses.add(f"{canonical}.{member}")

    numeric = contract.get("numeric_domains", {}).get("u32")
    if numeric != {
        "minimum": 0,
        "maximum": 4294967295,
        "rust_type": "u32",
        "lean_type": "UInt32",
        "surface": "nonempty ASCII decimal digits without sign or whitespace",
    }:
        fail("the canonical u32 numeric domain must be explicit and exact")
    declared_uses = set(contract["numeric_domains"].get("uses", []))
    expected_uses = {
        "RefinementPredicateV1.cardinality.min",
        "RefinementPredicateV1.cardinality.max",
        "ConstraintV1.at_most.max",
    }
    if declared_uses != expected_uses or u32_uses != {
        "refinement_predicate.cardinality.min",
        "refinement_predicate.cardinality.max",
        "constraint.at_most.max",
    }:
        fail("canonical u32 payload uses are incomplete")

    source_payloads = {
        language: source_payload_inventory(contract, root, language)
        for language in ("rust", "lean")
    }
    for declaration in inventory:
        canonical = declaration["canonical"]
        payload = by_name[canonical]
        for language in ("rust", "lean"):
            source_name = declaration[language]["declaration"]
            actual = source_payloads[language].get(source_name)
            if actual is None:
                fail(f"{language}: missing payload declaration {source_name}")
            if declaration[language]["kind"] in {"struct", "structure"}:
                expected_types = list(payload["fields"].values())
                actual_types = actual["payload_types"]
                if len(expected_types) != len(actual_types) or not all(
                    payload_types_match(language, expected, observed)
                    for expected, observed in zip(expected_types, actual_types, strict=True)
                ):
                    fail(
                        f"{language} payload type drift for {source_name}: "
                        f"contract={expected_types} source={actual_types}"
                    )
            else:
                source_constructors = declaration[language]["constructors"]
                wire_tags = declaration["wire_tags"]
                for source_constructor, wire_tag in zip(
                    source_constructors, wire_tags, strict=True
                ):
                    expected_types = list(payload["constructors"][wire_tag].values())
                    actual_types = actual["constructors"].get(source_constructor)
                    if actual_types is None or len(expected_types) != len(actual_types) or not all(
                        payload_types_match(language, expected, observed)
                        for expected, observed in zip(expected_types, actual_types, strict=True)
                    ):
                        fail(
                            f"{language} payload type drift for {source_name}.{source_constructor}: "
                            f"contract={expected_types} source={actual_types}"
                        )

    rust = (root / RUST_AST_PATH).read_text(encoding="utf-8")
    lean = (root / LEAN_AST_PATH).read_text(encoding="utf-8")
    if "def maxCanonicalUInt32 : Nat := 4294967295" not in lean:
        fail("Lean canonical UInt32 parser bound drift")
    for forbidden in (
        '\"bidirectional\" | \"both\"',
        'is_call(\"id\")',
        'tag(\"id\")',
        'tag(\"<:\")',
    ):
        if forbidden in rust:
            fail(f"Rust retains noncanonical parser alias {forbidden!r}")
    for forbidden in (
        '| \"bidirectional\" | \"both\"',
        'skipString \"id\"',
        'skipString \"<:\"',
    ):
        if forbidden in lean:
            fail(f"Lean retains noncanonical parser alias {forbidden!r}")


def check_inventory(contract: dict[str, Any], root: Path) -> None:
    inventory = contract.get("ast_inventory")
    if not isinstance(inventory, list) or not inventory:
        fail("contract ast_inventory must be a non-empty array")
    actual = {
        "rust": rust_inventory(root / RUST_AST_PATH),
        "lean": lean_inventory(root / LEAN_AST_PATH),
    }
    for language in ("rust", "lean"):
        declared = {entry[language]["declaration"] for entry in inventory}
        if declared != set(actual[language]):
            fail(
                f"{language} AST declaration drift: contract={sorted(declared)} "
                f"source={sorted(actual[language])}"
            )
        for entry in inventory:
            expected = dict(entry[language])
            name = expected.pop("declaration")
            if actual[language][name] != expected:
                fail(
                    f"{language} AST inventory drift for {name}: "
                    f"contract={expected} source={actual[language][name]}"
                )
    check_payload_contract(contract, root)


def run(binary: Path, args: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [str(binary), *args],
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=30,
        check=False,
    )


def source_has_forbidden_syntax_whitespace(source: str) -> bool:
    return any(
        character in {"\v", "\f"}
        or (character.isspace() and ord(character) > 127)
        for character in source
    )


def source_has_object_declaration_separator_violation(source: str) -> bool:
    return bool(re.search(r"^[ \t]*object [ \t]", source, re.MULTILINE))


def source_has_noncanonical_alias(source: str) -> bool:
    return bool(
        "<:" in source
        or re.search(r"^\s*orientation:\s*both\s*$", source, re.MULTILINE)
        or re.search(r"^\s*(?:lhs|rhs):\s*id\(", source, re.MULTILINE)
        or re.search(r"constraint\s+symmetric\s+\w+\s+where\s+\w+\s+in\s+", source)
        or re.search(r"constraint\s+\w+.*\sparam\s*\([^)]*\)\s+on\s*\(", source)
        or re.search(r":\s*[A-Za-z_][A-Za-z0-9_]*@(data|context|world|temporal|parameter|evidence)\b", source)
    )


def source_has_invalid_u32_token(source: str) -> bool:
    def valid(token: str) -> bool:
        return bool(token) and token.isascii() and token.isdigit() and int(token) <= 4294967295

    at_most = re.search(
        r"constraint\s+at_most[ \t\r\n]+(.*?)[ \t\r\n]+[A-Za-z_][A-Za-z0-9_]*[ \t\r\n]*\.",
        source,
        re.DOTALL,
    )
    if at_most and not valid(at_most.group(1).strip(" \t\r\n")):
        return True
    for body in re.findall(r"cardinality\(([^)]*)\)", source, re.DOTALL):
        parts = re.split(r"\|", body)
        if len(parts) != 2 or any(not valid(part.strip(" \t\r\n")) for part in parts):
            return True
    return False


def source_has_empty_list_element(source: str) -> bool:
    compact = re.sub(r"[ \t\r\n]", "", source)
    return any(
        token in compact
        for token in (
            "(,",
            ",)",
            ",,",
            "(|",
            "|)",
            "||",
            "{,",
            ",}",
        )
    ) or bool(re.search(r"^\s*vars:.*?,\s*$", source, re.MULTILINE))


def source_prioritized_parse_reason(rejection_class: str, source: str) -> str:
    """Return the source-specific diagnostic reason required for a broad class."""
    if rejection_class == "parse.lexical_domain":
        offset = next(
            (
                index
                for index, character in enumerate(source)
                if character in {"\v", "\f"}
                or (character.isspace() and ord(character) > 127)
            ),
            None,
        )
        if offset is None:
            fail("lexical-domain source has no forbidden syntax-whitespace character")
        start = source.rfind("\n", 0, offset) + 1
        end = source.find("\n", offset)
        line = source[start : len(source) if end < 0 else end]
        family = re.search(
            r"\bconstraint\s+(functional|at_most|typing|symmetric|transitive|key)\b",
            line,
        )
        if family:
            return f"lexical.constraint.{family.group(1)}"
        if "vars:" in line:
            return "lexical.rewrite_vars"
        if "@reversible" in line:
            return "lexical.generator_reversible"
        if re.search(r"^\s*relation\b", line):
            return "lexical.relation_type"
        if re.search(r"^\s*module\b", line):
            return "lexical.module_header"
        fail(f"lexical-domain source locus is outside the diagnostic reasons: {line!r}")

    if rejection_class == "parse.noncanonical_alias":
        if "<:" in source:
            return "alias.subtype_colon"
        if re.search(r"^\s*orientation:\s*both\s*$", source, re.MULTILINE):
            return "alias.rewrite_orientation"
        if re.search(r"^\s*(?:lhs|rhs):\s*id\(", source, re.MULTILINE):
            return "alias.rewrite_identity"
        if re.search(r"constraint\s+symmetric\s+\w+\s+where\s+\w+\s+in\s+", source):
            return "alias.symmetric_unqualified_guard"
        if re.search(r"constraint\s+\w+.*\sparam\s*\([^)]*\)\s+on\s*\(", source):
            return "alias.closure_clause_order"
        if re.search(
            r":\s*[A-Za-z_][A-Za-z0-9_]*@(data|context|world|temporal|parameter|evidence)\b",
            source,
        ):
            return "alias.attached_role_kind"
        fail("noncanonical-alias source is outside the diagnostic reasons")

    if rejection_class == "parse.numeric_domain":
        if re.search(r"\bconstraint\s+at_most\b", source):
            return "numeric.at_most_bound"
        match = re.search(r"cardinality\(([^)]*)\)", source, re.DOTALL)
        if match:
            parts = match.group(1).split("|")
            if len(parts) == 2:
                first_valid = bool(parts[0].strip(" \t\r\n")) and not source_has_invalid_u32_token(
                    f"relation R(x: refined(A; cardinality({parts[0]}|0)))"
                )
                return "numeric.cardinality_maximum" if first_valid else "numeric.cardinality_minimum"
        fail("numeric-domain source is outside the diagnostic reasons")

    if rejection_class == "parse.list_syntax":
        if re.search(r"\bconstraint\s+key\b", source):
            return "list.theory_key_fields"
        if re.search(r"\bconstraint\s+symmetric\b", source):
            return "list.symmetric_clause"
        if re.search(r"\bconstraint\s+transitive\b", source):
            return "list.transitive_clause"
        if re.search(r"^\s*vars:", source, re.MULTILINE):
            return "list.rewrite_vars"
        if re.search(r"relation\s+[^\n]*,\s*\)", source):
            return "list.relation_roles"
        for constructor in ("indexed", "key", "in", "enum", "predicate"):
            if re.search(rf"\b{constructor}\((?:\||[^)]*(?:,,|\|\||\|\)))", source):
                return f"list.recursive_{constructor}"
        if re.search(r"=\s*\{\s*\(", source):
            return "list.tuple_fields"
        if re.search(r"=\s*\{", source):
            return "list.set_items"
        fail("list-syntax source is outside the diagnostic reasons")

    fail(f"class {rejection_class!r} is not source-prioritized")
    return ""


def parse_reason_diagnostic_tokens(reason: str, language: str) -> tuple[str, ...]:
    """Return accepted diagnostic tokens for one source-specific parse reason."""
    if reason.startswith("lexical.constraint."):
        family = reason.rsplit(".", 1)[1]
        common_extra = ("constraint header expects",)
        rust_extra = {
            "at_most": ("parameter field", "carrier field"),
            "symmetric": ("parameter field", "carrier field", "set literal"),
            "transitive": ("parameter field", "carrier field"),
        }.get(family, ())
        shared = (
            family,
            "constraint family keyword requires canonical ASCII separating whitespace",
            *common_extra,
        )
        return (*shared, *rust_extra) if language == "rust" else shared
    fixed = {
        "lexical.rewrite_vars": {
            "rust": ("invalid rewrite vars line",),
            "lean": ("expected end of input",),
        },
        "lexical.generator_reversible": {
            "rust": ("generator target must be an identifier",),
            "lean": ("expected end of input",),
        },
        "lexical.relation_type": {
            "rust": (
                "object type must be an identifier",
                "relation-object type must be an identifier",
                "indexed role must be an identifier",
                "unsupported refinement predicate",
                "equality value must be an identifier",
                "membership value must be an identifier",
                "enum value must be an identifier",
                "key role must be an identifier",
                "cardinality",
                "predicate name or argument must be an identifier",
            ),
            "lean": ("relation expects exactly",),
        },
        "lexical.module_header": {
            "rust": ("module header must be the first",),
            "lean": ("module header must be the first",),
        },
        "alias.subtype_colon": {
            "rust": ("subtype expects canonical",),
            "lean": ("condition not satisfied", "expected: '<'"),
        },
        "alias.rewrite_orientation": {
            "rust": ("unknown rewrite orientation `both`",),
            "lean": ("unknown rewrite orientation `both`",),
        },
        "alias.rewrite_identity": {
            "rust": ("invalid path expression",),
            "lean": ("expected end of input",),
        },
        "alias.symmetric_unqualified_guard": {
            "rust": ("symmetric guard must use canonical qualified",),
            "lean": ("symmetric expects",),
        },
        "alias.closure_clause_order": {
            "rust": ("canonical `on (...) param (...)` order",),
            "lean": ("symmetric expects", "transitive expects"),
        },
        "alias.attached_role_kind": {
            "rust": ("role annotation", "canonical separating whitespace"),
            "lean": ("relation expects exactly",),
        },
        "numeric.at_most_bound": {
            "rust": ("at_most",),
            "lean": ("at_most",),
        },
        "numeric.cardinality_minimum": {
            "rust": ("cardinality minimum",),
            "lean": ("relation expects exactly",),
        },
        "numeric.cardinality_maximum": {
            "rust": ("cardinality maximum",),
            "lean": ("relation expects exactly",),
        },
        "list.theory_key_fields": {
            "rust": ("key field",),
            "lean": ("key expects",),
        },
        "list.symmetric_clause": {
            "rust": ("on fields", "set literal", "symmetric expects"),
            "lean": ("symmetric expects",),
        },
        "list.transitive_clause": {
            "rust": ("param fields", "transitive expects"),
            "lean": ("transitive expects",),
        },
        "list.rewrite_vars": {
            "rust": ("invalid rewrite vars line",),
            "lean": ("expected end of input",),
        },
        "list.relation_roles": {
            "rust": ("relation role expects",),
            "lean": ("relation expects exactly",),
        },
        "list.recursive_indexed": {
            "rust": ("indexed role list",),
            "lean": ("relation expects exactly",),
        },
        "list.recursive_key": {
            "rust": ("key role list",),
            "lean": ("relation expects exactly",),
        },
        "list.recursive_in": {
            "rust": ("membership value list",),
            "lean": ("relation expects exactly",),
        },
        "list.recursive_enum": {
            "rust": ("enum value list",),
            "lean": ("relation expects exactly",),
        },
        "list.recursive_predicate": {
            "rust": ("predicate name or argument list",),
            "lean": ("relation expects exactly",),
        },
        "list.tuple_fields": {
            "rust": ("tuple field list",),
            "lean": ("expected: '}'",),
        },
        "list.set_items": {
            "rust": ("set literal",),
            "lean": ("expected: '}'",),
        },
    }
    by_language = fixed.get(reason)
    if by_language is None or language not in by_language:
        fail(f"missing {language} diagnostic tokens for parse reason {reason!r}")
    return by_language[language]


def require_source_prioritized_diagnostic(
    rejection_class: str, language: str, source: str, message: str
) -> str:
    reason = source_prioritized_parse_reason(rejection_class, source)
    tokens = parse_reason_diagnostic_tokens(reason, language)
    if not any(token in message for token in tokens):
        fail(
            f"{language} {rejection_class} diagnostic does not match source reason "
            f"{reason!r}: {message!r}"
        )
    return reason


def classify_parse_rejection(language: str, source: str, stderr: str) -> str:
    if not stderr.startswith("parse error on line "):
        fail(f"{language} parse rejection lacks the canonical parse-stage diagnostic: {stderr!r}")
    message = stderr.split(": ", 1)[1] if ": " in stderr else stderr
    if "syntax nesting exceeds" in message:
        return "parse.resource_depth"
    if "unbalanced" in message:
        return "parse.delimiter"
    if source_has_forbidden_syntax_whitespace(source):
        rejection_class = "parse.lexical_domain"
        require_source_prioritized_diagnostic(rejection_class, language, source, message)
        return rejection_class
    if source_has_object_declaration_separator_violation(source):
        if "object name" not in message:
            fail(
                f"{language} object-declaration separator diagnostic does not match "
                f"the source reason: {message!r}"
            )
        return "parse.section_syntax"
    if source_has_noncanonical_alias(source):
        rejection_class = "parse.noncanonical_alias"
        require_source_prioritized_diagnostic(rejection_class, language, source, message)
        return rejection_class
    if "@mystery" in source or "unknown or misplaced role annotation" in message:
        return "parse.role_kind"
    if "relation-level axis shorthands are not canonical" in message and re.search(
        r"relation\s+[^\n]+\)\s+@(context|world|temporal|parameter|evidence|data)\b",
        source,
    ):
        return "parse.relation_axis_shorthand"
    if source_has_invalid_u32_token(source):
        rejection_class = "parse.numeric_domain"
        require_source_prioritized_diagnostic(rejection_class, language, source, message)
        return rejection_class
    if source_has_empty_list_element(source):
        rejection_class = "parse.list_syntax"
        require_source_prioritized_diagnostic(rejection_class, language, source, message)
        return rejection_class
    if "rewrite `" in message and ("missing" in message or "duplicate" in message):
        return "parse.rewrite_shape"
    if (
        "module header must be the first" in message
        or "imports must follow the module header" in message
        or "requires exactly one explicit `module" in message
    ):
        return "parse.header_order"
    if (
        "header expects" in message
        or "expected: '<'" in message
        or source.lstrip().startswith("module M:")
        or re.search(r"^\s*subtype\s+", source, re.MULTILINE)
    ):
        return "parse.section_syntax"
    if re.search(
        r"^\s*constraint\s+(functional|at_most|typing|symmetric|transitive|key)\b",
        source,
        re.MULTILINE,
    ):
        return "parse.constraint_shape"
    fail(f"{language} parse diagnostic is outside the closed taxonomy: {stderr!r}")
    return ""


def classify_typecheck_rejection(language: str, stderr: str) -> str:
    if not stderr.startswith("type error: "):
        fail(
            f"{language} typecheck rejection lacks the canonical typecheck-stage diagnostic: "
            f"{stderr!r}"
        )
    message = stderr.removeprefix("type error: ")
    if any(token in message for token in (" repeats ", "duplicate", "declares both", "repeats field")):
        return "typecheck.duplicate_declaration"
    if "indexes unknown or non-earlier role" in message:
        return "typecheck.dependent_role"
    if "uses unsupported predicate" in message:
        return "typecheck.refinement"
    if "instance `" in message and "references unknown schema" in message:
        return "typecheck.instance_formation"
    if any(
        token in message
        for token in (
            "unknown relation",
            "has no field",
            "equation `",
            "rewrite rule `",
            "constraint",
        )
    ):
        return "typecheck.theory_formation"
    if any(
        token in message
        for token in (
            "references unknown object",
            "references unknown value type",
            "subtype cycle",
            "references unknown schema",
        )
    ):
        return "typecheck.schema_formation"
    fail(f"{language} typecheck diagnostic is outside the closed taxonomy: {stderr!r}")
    return ""


def classify_actual_rejection(
    stage: str,
    language: str,
    source: str,
    completed: subprocess.CompletedProcess[str],
) -> str:
    expected_exit = 2 if stage == "boundary" and language == "rust" else 1
    if completed.returncode != expected_exit:
        fail(
            f"{language} {stage} rejection used exit {completed.returncode}; "
            f"expected {expected_exit}: {completed.stderr!r}"
        )
    stderr = completed.stderr.strip()
    if stage == "boundary":
        marker = "is not UTF-8" if language == "rust" else "containing non UTF-8 data"
        if marker not in stderr:
            fail(f"{language} invalid-UTF8 diagnostic drift: {stderr!r}")
        return "boundary.invalid_utf8"
    if stage == "parse":
        return classify_parse_rejection(language, source, stderr)
    if stage == "typecheck":
        return classify_typecheck_rejection(language, stderr)
    fail(f"unknown operational rejection stage: {stage!r}")
    return ""


def require_cross_language_rejection(
    label: str,
    expected_class: str,
    stage: str,
    source: str,
    rust: subprocess.CompletedProcess[str],
    lean: subprocess.CompletedProcess[str],
) -> str:
    declared_stage = CROSS_LANGUAGE_REJECTION_STAGES.get(expected_class)
    if declared_stage != stage:
        fail(
            f"{label}: class {expected_class!r} belongs to stage {declared_stage!r}, "
            f"not {stage!r}"
        )
    actual = {
        "rust": classify_actual_rejection(stage, "rust", source, rust),
        "lean": classify_actual_rejection(stage, "lean", source, lean),
    }
    if actual["rust"] != actual["lean"]:
        fail(f"{label}: Rust and Lean rejected for different classes: {actual}")
    if actual["rust"] != expected_class:
        fail(
            f"{label}: expected class {expected_class!r}, observed {actual['rust']!r} "
            "from both language diagnostics"
        )
    return expected_class


def expect_status(
    label: str,
    completed: subprocess.CompletedProcess[str],
    accepted: bool,
    error_contains: str | None,
) -> None:
    actual = completed.returncode == 0
    if actual != accepted:
        fail(
            f"{label}: expected accepted={accepted}, exit={completed.returncode}, "
            f"stdout={completed.stdout!r}, stderr={completed.stderr!r}"
        )
    if not accepted and error_contains and error_contains not in completed.stderr:
        fail(f"{label}: rejection did not contain {error_contains!r}: {completed.stderr!r}")


def check_fixture_taxonomy(contract: dict[str, Any], fixtures: list[dict[str, Any]]) -> None:
    rejection_classes = contract.get("rejection_classes")
    if rejection_classes != list(CROSS_LANGUAGE_REJECTION_STAGES):
        fail("contract rejection_classes must equal the closed operational taxonomy")
    if contract.get("diagnostic_taxonomy") != diagnostic_taxonomy_contract():
        fail("contract diagnostic_taxonomy must equal the closed stage/class mapping")
    declared_outcomes = contract.get("non_cross_language_outcome_classes")
    if declared_outcomes != list(NON_CROSS_LANGUAGE_OUTCOME_CLASSES):
        fail("contract non-cross-language outcome classes drift")
    declared_classes = set(rejection_classes) | set(declared_outcomes)
    paths = [case.get("path") for case in fixtures]
    if len(paths) != len(set(paths)):
        fail("contract fixture paths must be unique")

    required_regressions = {
        "fixtures/canonical/contract/reject_functional_shape.axi": "parse.constraint_shape",
        "fixtures/canonical/contract/reject_at_most_bound.axi": "parse.numeric_domain",
        "fixtures/canonical/contract/reject_typing_shape.axi": "parse.constraint_shape",
        "fixtures/canonical/contract/reject_symmetric_shape.axi": "parse.constraint_shape",
        "fixtures/canonical/contract/reject_transitive_shape.axi": "parse.constraint_shape",
        "fixtures/canonical/contract/reject_key_shape.axi": "parse.constraint_shape",
        "fixtures/canonical/contract/reject_rewrite_missing_vars.axi": "parse.rewrite_shape",
        "fixtures/canonical/contract/reject_rewrite_missing_orientation_value.axi":
            "parse.rewrite_shape",
        "fixtures/canonical/contract/reject_rewrite_duplicate_vars.axi": "parse.rewrite_shape",
        "fixtures/canonical/contract/reject_rewrite_duplicate_lhs.axi": "parse.rewrite_shape",
        "fixtures/canonical/contract/reject_rewrite_duplicate_rhs.axi": "parse.rewrite_shape",
        "fixtures/canonical/contract/reject_rewrite_duplicate_orientation.axi": "parse.rewrite_shape",
        "fixtures/canonical/contract/reject_bad_dependent_role.axi": "typecheck.dependent_role",
        "fixtures/canonical/contract/reject_unsupported_refinement.axi": "typecheck.refinement",
        "fixtures/canonical/contract/formation_reject_key_refinement.axi":
            "formation.unsupported_refinement_witness",
        "fixtures/canonical/contract/accept_unknown_constraint_suffix.axi":
            "unsupported.opaque_constraint",
        "fixtures/canonical/contract/reject_object_relation_collision.axi":
            "typecheck.duplicate_declaration",
        "fixtures/canonical/contract/reject_repeated_subtype.axi":
            "typecheck.duplicate_declaration",
        "fixtures/canonical/contract/reject_self_subtype_cycle.axi":
            "typecheck.schema_formation",
        "fixtures/canonical/contract/reject_at_most_u32_overflow.axi":
            "parse.numeric_domain",
        "fixtures/canonical/contract/reject_cardinality_u32_overflow.axi":
            "parse.numeric_domain",
        "fixtures/canonical/contract/reject_rewrite_orientation_both.axi":
            "parse.noncanonical_alias",
        "fixtures/canonical/contract/reject_rewrite_path_id.axi":
            "parse.noncanonical_alias",
        "fixtures/canonical/contract/reject_late_module.axi": "parse.header_order",
        "fixtures/canonical/contract/reject_unbalanced_constraint_bracket.axi":
            "parse.delimiter",
        "fixtures/canonical/contract/reject_unbalanced_constraint_quote.axi":
            "parse.delimiter",
        "fixtures/canonical/contract/reject_symmetric_bare_guard_field.axi":
            "parse.noncanonical_alias",
        "fixtures/canonical/contract/reject_noncanonical_closure_clause_order.axi":
            "parse.noncanonical_alias",
        "fixtures/canonical/contract/reject_constraint_unknown_relation.axi":
            "typecheck.theory_formation",
        "fixtures/canonical/contract/reject_constraint_unknown_field.axi":
            "typecheck.theory_formation",
        "fixtures/canonical/contract/reject_rewrite_unknown_relation.axi":
            "typecheck.theory_formation",
        "fixtures/canonical/contract/reject_schema_repeated_colon.axi":
            "parse.section_syntax",
        "fixtures/canonical/contract/reject_attached_role_kind.axi":
            "parse.noncanonical_alias",
        "fixtures/canonical/contract/reject_equation_unknown_relation.axi":
            "typecheck.theory_formation",
        "fixtures/canonical/contract/reject_equation_composition.axi":
            "typecheck.theory_formation",
        "fixtures/canonical/contract/reject_equation_endpoints.axi":
            "typecheck.theory_formation",
        "fixtures/canonical/contract/reject_duplicate_equation.axi":
            "typecheck.duplicate_declaration",
    }
    by_path = {case["path"]: case for case in fixtures}
    for path, rejection_class in required_regressions.items():
        case = by_path.get(path)
        if case is None or case.get("class") != rejection_class:
            fail(f"missing required {rejection_class} regression fixture: {path}")
    max_case = by_path.get("fixtures/canonical/contract/accept_u32_max.axi")
    if max_case is None or not all(
        max_case.get(key) is True
        for key in ("rust_parse", "lean_parse", "rust_typecheck", "lean_typecheck")
    ) or not max_case.get("compare_normalized_ast"):
        fail("missing shared accepted u32 maximum regression fixture")
    equation_carrier_case = by_path.get(
        "fixtures/canonical/contract/accept_equation_declared_order_carrier.axi"
    )
    if equation_carrier_case is None or not all(
        equation_carrier_case.get(key) is True
        for key in (
            "rust_parse",
            "lean_parse",
            "rust_typecheck",
            "lean_typecheck",
            "compare_normalized_ast",
            "compare_revision",
        )
    ):
        fail("missing shared accepted declared-order equation carrier regression fixture")
    mixed_comment_case = by_path.get(
        "fixtures/canonical/contract/exact_mixed_comment.axi"
    )
    if mixed_comment_case is None or not all(
        mixed_comment_case.get(key) is True
        for key in (
            "rust_parse",
            "lean_parse",
            "rust_typecheck",
            "lean_typecheck",
            "compare_normalized_ast",
            "compare_revision",
        )
    ):
        fail("missing shared accepted mixed-comment precedence regression fixture")
    for path in (
        "fixtures/canonical/contract/accept_rewrite_path_prefix_objects.axi",
        "fixtures/canonical/contract/accept_call_interior_whitespace.axi",
        "fixtures/canonical/contract/accept_reversible_ht_separator.axi",
        "fixtures/canonical/contract/accept_reversible_cr_separator.axi",
    ):
        case = by_path.get(path)
        if case is None or not all(
            case.get(key) is True
            for key in (
                "rust_parse",
                "lean_parse",
                "rust_typecheck",
                "lean_typecheck",
                "compare_normalized_ast",
                "compare_revision",
            )
        ):
            fail(f"missing shared accepted grammar-boundary regression fixture: {path}")

    for case in fixtures:
        relative = case.get("path")
        if not isinstance(relative, str) or not relative:
            fail("fixture path must be a non-empty string")
        if case.get("rust_parse") is not case.get("lean_parse"):
            fail(f"{relative}: Rust and Lean parse outcomes must agree")
        rejection_class = case.get("class")
        if rejection_class is not None and rejection_class not in declared_classes:
            fail(f"{relative}: undeclared outcome class {rejection_class!r}")
        if case.get("rust_parse") is False:
            if CROSS_LANGUAGE_REJECTION_STAGES.get(rejection_class) != "parse":
                fail(f"{relative}: parse rejection class/stage mismatch")
        elif case.get("rust_typecheck") is False:
            if CROSS_LANGUAGE_REJECTION_STAGES.get(rejection_class) != "typecheck":
                fail(f"{relative}: typecheck rejection class/stage mismatch")
        has_rust_typecheck = "rust_typecheck" in case
        has_lean_typecheck = "lean_typecheck" in case
        if has_rust_typecheck != has_lean_typecheck:
            fail(f"{relative}: typecheck outcomes must be declared for both languages")
        if has_rust_typecheck:
            if not case["rust_parse"]:
                fail(f"{relative}: cannot typecheck a parse rejection")
            if case["rust_typecheck"] is not case["lean_typecheck"]:
                fail(f"{relative}: Rust and Lean typecheck outcomes must agree")


def check_fixtures(
    contract: dict[str, Any], root: Path, bins: dict[str, Path]
) -> set[str]:
    fixtures = contract.get("fixtures")
    if not isinstance(fixtures, list) or not fixtures:
        fail("contract fixtures must be a non-empty array")
    check_fixture_taxonomy(contract, fixtures)
    normalized: dict[str, dict[str, Any]] = {}
    revisions: dict[str, str] = {}
    evidenced_classes: set[str] = set()
    for case in fixtures:
        relative = case["path"]
        path = root / relative
        source = path.read_bytes()
        digest = hashlib.sha256(source).hexdigest()
        if digest != case["sha256"]:
            fail(f"{relative}: fixture SHA-256 drift: expected {case['sha256']}, got {digest}")

        rust_parse = run(bins["rust_parse"], [str(path)])
        lean_parse = run(bins["lean_parse"], [str(path)])
        expect_status(
            f"{relative} Rust parse",
            rust_parse,
            case["rust_parse"],
            case.get("rust_error_contains"),
        )
        expect_status(
            f"{relative} Lean parse",
            lean_parse,
            case["lean_parse"],
            case.get("lean_error_contains"),
        )
        if not case["rust_parse"]:
            evidenced_classes.add(
                require_cross_language_rejection(
                    relative,
                    case["class"],
                    "parse",
                    source.decode("utf-8"),
                    rust_parse,
                    lean_parse,
                )
            )

        if "rust_typecheck" in case:
            rust_typecheck = run(bins["rust_typecheck"], [str(path)])
            lean_typecheck = run(bins["lean_typecheck"], [str(path)])
            expect_status(
                f"{relative} Rust typecheck",
                rust_typecheck,
                case["rust_typecheck"],
                case.get("rust_typecheck_error_contains"),
            )
            expect_status(
                f"{relative} Lean typecheck",
                lean_typecheck,
                case["lean_typecheck"],
                case.get("lean_typecheck_error_contains"),
            )
            if not case["rust_typecheck"]:
                evidenced_classes.add(
                    require_cross_language_rejection(
                        relative,
                        case["class"],
                        "typecheck",
                        source.decode("utf-8"),
                        rust_typecheck,
                        lean_typecheck,
                    )
                )

        if case.get("compare_normalized_ast"):
            rust_view = run(bins["rust_parse"], ["--contract-ast-v1", str(path)])
            lean_view = run(bins["lean_parse"], ["--contract-ast-v1", str(path)])
            expect_status(f"{relative} Rust contract AST", rust_view, True, None)
            expect_status(f"{relative} Lean contract AST", lean_view, True, None)
            try:
                rust_json = json.loads(rust_view.stdout)
                lean_json = json.loads(lean_view.stdout)
            except json.JSONDecodeError as error:
                fail(f"{relative}: invalid normalized AST JSON: {error}")
            if rust_json != lean_json:
                fail(f"{relative}: independently reconstructed normalized AST values differ")
            normalized[relative] = rust_json

        if case.get("compare_revision"):
            rust_revision = run(bins["rust_digest"], [str(path)])
            lean_revision = run(bins["lean_digest"], ["--revision-digest-v2", str(path)])
            expect_status(f"{relative} Rust revision", rust_revision, True, None)
            expect_status(f"{relative} Lean revision", lean_revision, True, None)
            rust_value = rust_revision.stdout.strip()
            lean_value = lean_revision.stdout.strip()
            if rust_value != lean_value:
                fail(f"{relative}: Rust/Lean revision digest differs")
            if not re.fullmatch(r"axi:revision:v2:sha256:[0-9a-f]{64}", rust_value):
                fail(f"{relative}: malformed revision wire value {rust_value!r}")
            revisions[relative] = rust_value

    all_constructor_path = "fixtures/canonical/contract/all_constructors.axi"
    observed_tags: set[tuple[str, str]] = set()

    def collect_tags(value: Any) -> None:
        if isinstance(value, dict):
            for key, item in value.items():
                if key in {"kind", "tag", "type", "orientation"} and isinstance(item, str):
                    observed_tags.add((key, item))
                collect_tags(item)
        elif isinstance(value, list):
            for item in value:
                collect_tags(item)

    collect_tags(normalized[all_constructor_path])
    for entry in contract["ast_inventory"]:
        discriminator = entry.get("wire_discriminator")
        if discriminator is None and entry["canonical"] in {
            "role_kind",
            "generator_kind",
            "rewrite_orientation",
        }:
            discriminator = "orientation" if entry["canonical"] == "rewrite_orientation" else "kind"
        for tag in entry.get("wire_tags", []):
            if (discriminator, tag) not in observed_tags:
                fail(
                    f"{all_constructor_path}: constructor tag {discriminator}={tag} "
                    "is not exercised"
                )

    exact_paths = [
        "fixtures/canonical/contract/exact_lf.axi",
        "fixtures/canonical/contract/exact_comment.axi",
        "fixtures/canonical/contract/exact_mixed_comment.axi",
        "fixtures/canonical/contract/exact_crlf.axi",
    ]
    if not all(normalized[path] == normalized[exact_paths[0]] for path in exact_paths[1:]):
        fail("comment/newline byte variants must reconstruct the same finite AST fixture")
    if len({revisions[path] for path in exact_paths}) != len(exact_paths):
        fail("comment/newline byte variants must have distinct exact-byte revisions")

    with tempfile.TemporaryDirectory(prefix="axiograph-axi-v1-contract-") as directory:
        invalid = Path(directory) / "invalid-utf8.axi"
        invalid.write_bytes(b"module InvalidUtf8\n# \xff\n")
        expect_status("invalid UTF-8 Rust boundary", run(bins["rust_parse"], [str(invalid)]), False, None)
        expect_status("invalid UTF-8 Lean boundary", run(bins["lean_parse"], [str(invalid)]), False, None)
        expect_status("invalid UTF-8 Rust identity", run(bins["rust_digest"], [str(invalid)]), False, None)
        expect_status(
            "invalid UTF-8 Lean identity",
            run(bins["lean_digest"], ["--revision-digest-v2", str(invalid)]),
            False,
            None,
        )
        evidenced_classes.add(
            require_cross_language_rejection(
                "invalid UTF-8 parser boundary",
                "boundary.invalid_utf8",
                "boundary",
                "",
                run(bins["rust_parse"], [str(invalid)]),
                run(bins["lean_parse"], [str(invalid)]),
            )
        )
    return evidenced_classes


def check_adversarial_matrix(
    contract: dict[str, Any], root: Path, bins: dict[str, Path]
) -> tuple[int, set[str]]:
    reference = contract.get("adversarial_matrix")
    if not isinstance(reference, dict):
        fail("contract adversarial_matrix must be an object")
    relative = reference.get("path")
    expected_digest = reference.get("sha256")
    if not isinstance(relative, str) or not isinstance(expected_digest, str):
        fail("contract adversarial_matrix path and SHA-256 are required")
    path = root / relative
    source_bytes = path.read_bytes()
    actual_digest = hashlib.sha256(source_bytes).hexdigest()
    if actual_digest != expected_digest:
        fail(
            f"{relative}: adversarial matrix SHA-256 drift: "
            f"expected {expected_digest}, got {actual_digest}"
        )
    matrix = json.loads(source_bytes)
    if (
        not isinstance(matrix, dict)
        or matrix.get("schema") != "axiograph.canonical_axi_v1_adversarial_matrix"
        or matrix.get("version") != 1
    ):
        fail("unsupported axi_v1 adversarial matrix schema/version")
    cases = validate_boundary_coverage(matrix)
    identifiers = [case.get("id") for case in cases if isinstance(case, dict)]
    if len(identifiers) != len(cases) or any(
        not isinstance(identifier, str) or not identifier for identifier in identifiers
    ):
        fail("every adversarial matrix case must have a non-empty id")
    if len(identifiers) != len(set(identifiers)):
        fail("adversarial matrix case ids must be unique")
    classes = (
        set(contract["rejection_classes"])
        | set(contract["non_cross_language_outcome_classes"])
        | {"accepted"}
    )
    evidenced_classes: set[str] = set()

    with tempfile.TemporaryDirectory(prefix="axiograph-axi-v1-matrix-") as directory:
        directory_path = Path(directory)
        for index, case in enumerate(cases):
            label = f"matrix[{index}] {case['id']}"
            dimensions = case.get("dimensions")
            if not isinstance(dimensions, list) or not dimensions or not all(
                isinstance(item, str) and item for item in dimensions
            ):
                fail(f"{label}: dimensions must be a non-empty string array")
            source = case.get("source")
            parse_expected = case.get("parse")
            outcome_class = case.get("class")
            if not isinstance(source, str) or not isinstance(parse_expected, bool):
                fail(f"{label}: source and Boolean parse outcome are required")
            if outcome_class not in classes:
                fail(f"{label}: undeclared outcome class {outcome_class!r}")
            if parse_expected and not isinstance(case.get("typecheck"), bool):
                fail(f"{label}: accepted parse requires a Boolean typecheck outcome")
            if not parse_expected and "typecheck" in case:
                fail(f"{label}: parse rejection cannot declare a typecheck outcome")
            expected_stage = CROSS_LANGUAGE_REJECTION_STAGES.get(outcome_class)
            if not parse_expected and expected_stage != "parse":
                fail(f"{label}: parse rejection class/stage mismatch")
            if parse_expected and case.get("typecheck") is False and expected_stage != "typecheck":
                fail(f"{label}: typecheck rejection class/stage mismatch")
            if parse_expected and case.get("typecheck") is True and expected_stage is not None:
                fail(f"{label}: accepted case cannot declare a rejection class")

            case_path = directory_path / f"{index:03d}-{case['id']}.axi"
            case_path.write_bytes(source.encode("utf-8"))
            rust_parse = run(bins["rust_parse"], [str(case_path)])
            lean_parse = run(bins["lean_parse"], [str(case_path)])
            expect_status(
                f"{label} Rust parse",
                rust_parse,
                parse_expected,
                case.get("rust_error_contains"),
            )
            expect_status(
                f"{label} Lean parse",
                lean_parse,
                parse_expected,
                case.get("lean_error_contains"),
            )
            if not parse_expected:
                evidenced_classes.add(
                    require_cross_language_rejection(
                        label,
                        outcome_class,
                        "parse",
                        source,
                        rust_parse,
                        lean_parse,
                    )
                )
                if case.get("normalized_ast") == "reject":
                    rust_view = run(
                        bins["rust_parse"], ["--contract-ast-v1", str(case_path)]
                    )
                    lean_view = run(
                        bins["lean_parse"], ["--contract-ast-v1", str(case_path)]
                    )
                    expect_status(
                        f"{label} Rust rejected contract AST", rust_view, False, None
                    )
                    expect_status(
                        f"{label} Lean rejected contract AST", lean_view, False, None
                    )
                    evidenced_classes.add(
                        require_cross_language_rejection(
                            f"{label} normalized AST",
                            outcome_class,
                            "parse",
                            source,
                            rust_view,
                            lean_view,
                        )
                    )
                continue

            if case.get("normalized_ast") not in (None, "compare"):
                fail(f"{label}: unsupported normalized AST expectation")
            rust_view = run(bins["rust_parse"], ["--contract-ast-v1", str(case_path)])
            lean_view = run(bins["lean_parse"], ["--contract-ast-v1", str(case_path)])
            expect_status(f"{label} Rust contract AST", rust_view, True, None)
            expect_status(f"{label} Lean contract AST", lean_view, True, None)
            try:
                rust_json = json.loads(rust_view.stdout)
                lean_json = json.loads(lean_view.stdout)
            except json.JSONDecodeError as error:
                fail(f"{label}: invalid normalized AST JSON: {error}")
            if rust_json != lean_json:
                fail(f"{label}: independently reconstructed normalized AST values differ")

            typecheck_expected = case["typecheck"]
            rust_typecheck = run(bins["rust_typecheck"], [str(case_path)])
            lean_typecheck = run(bins["lean_typecheck"], [str(case_path)])
            expect_status(
                f"{label} Rust typecheck",
                rust_typecheck,
                typecheck_expected,
                case.get("rust_typecheck_error_contains"),
            )
            expect_status(
                f"{label} Lean typecheck",
                lean_typecheck,
                typecheck_expected,
                case.get("lean_typecheck_error_contains"),
            )
            if not typecheck_expected:
                evidenced_classes.add(
                    require_cross_language_rejection(
                        label,
                        outcome_class,
                        "typecheck",
                        source,
                        rust_typecheck,
                        lean_typecheck,
                    )
                )
    return len(cases), evidenced_classes


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--rust-parse", type=Path, required=True)
    parser.add_argument("--lean-parse", type=Path, required=True)
    parser.add_argument("--rust-typecheck", type=Path, required=True)
    parser.add_argument("--lean-typecheck", type=Path, required=True)
    parser.add_argument("--rust-digest", type=Path, required=True)
    parser.add_argument("--lean-digest", type=Path, required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    root = args.root.resolve(strict=True)
    contract = read_json(root / CONTRACT_PATH)
    if contract.get("schema") != "axiograph.canonical_axi_v1_contract" or contract.get("version") != 1:
        fail("unsupported axi_v1 contract schema/version")
    if contract.get("authority", {}).get("accepted_meaning") != "exact_utf8_axi_bytes":
        fail("contract must keep exact UTF-8 source bytes authoritative")
    if contract.get("authority", {}).get("rust_ast_or_ir_authoritative_for_lean") is not False:
        fail("contract must prohibit Rust AST/IR authority for Lean source meaning")
    if contract.get("differential_envelope") != differential_envelope_contract():
        fail("contract differential envelope drift")
    check_inventory(contract, root)
    bins = {
        "rust_parse": args.rust_parse.resolve(strict=True),
        "lean_parse": args.lean_parse.resolve(strict=True),
        "rust_typecheck": args.rust_typecheck.resolve(strict=True),
        "lean_typecheck": args.lean_typecheck.resolve(strict=True),
        "rust_digest": args.rust_digest.resolve(strict=True),
        "lean_digest": args.lean_digest.resolve(strict=True),
    }
    fixture_classes = check_fixtures(contract, root, bins)
    matrix_cases, matrix_classes = check_adversarial_matrix(contract, root, bins)
    evidenced_classes = fixture_classes | matrix_classes
    required_classes = set(CROSS_LANGUAGE_REJECTION_STAGES)
    if evidenced_classes != required_classes:
        fail(
            "operational rejection taxonomy coverage differs: "
            f"missing={sorted(required_classes - evidenced_classes)} "
            f"extra={sorted(evidenced_classes - required_classes)}"
        )
    print(
        json.dumps(
            {
                "schema": "axiograph.axi_v1_contract_check",
                "version": 1,
                "status": "pass",
                "ast_declarations": len(contract["ast_inventory"]),
                "fixtures": len(contract["fixtures"]),
                "adversarial_matrix_cases": matrix_cases,
                "operational_rejection_classes": len(evidenced_classes),
                "authority": "exact_utf8_axi_bytes",
                "accepted": False,
            },
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print(f"axi_v1 contract check failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
