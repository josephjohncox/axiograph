//! Canonical `.axi` module digests (versioned).
//!
//! Certificates are snapshot-scoped: the trusted checker needs a stable way to
//! refer to the exact `.axi` inputs a result depends on.
//!
//! For the initial migration we use a **simple, deterministic, non-cryptographic**
//! digest:
//!
//! - algorithm: **FNV-1a 64-bit**
//! - input: the UTF-8 bytes of the `.axi` file as-read
//! - output: `"fnv1a64:<16 lowercase hex digits>"`
//!
//! This is intentionally easy to implement in both Rust and Lean.
//!
//! Notes:
//! - This digest is **not** a security primitive. It is a stability/identity
//!   tool for snapshots in the verified pipeline.
//! - We can upgrade to a cryptographic hash later (e.g. SHA-256) once we have a
//!   vetted Lean implementation/library dependency in-tree.

/// Prefix used in serialized digests.
pub const AXI_DIGEST_V1_PREFIX: &str = "fnv1a64:";

/// Prefix used in serialized fact ids (FNV-1a 64-bit).
pub const AXI_FACT_ID_V1_PREFIX: &str = "factfnv1a64:";

/// Compute a v1 digest (FNV-1a 64-bit) over arbitrary bytes.
///
/// This uses the same `"fnv1a64:<hex>"` encoding as `axi_digest_v1`, but is not
/// restricted to UTF-8 `.axi` text. It is useful for:
/// - `.axpd` snapshot keys (REPL query-plan cache),
/// - query IR digests, and
/// - other internal stability ids where cryptographic guarantees are not required.
pub fn fnv1a64_digest_bytes(bytes: &[u8]) -> String {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x00000100000001b3;

    let mut hash = FNV_OFFSET_BASIS;
    for b in bytes {
        hash ^= (*b) as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }

    format!("{AXI_DIGEST_V1_PREFIX}{hash:016x}")
}

/// Compute the v1 digest for a `.axi` module (FNV-1a 64-bit).
pub fn axi_digest_v1(text: &str) -> String {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x00000100000001b3;

    let mut hash = FNV_OFFSET_BASIS;
    for b in text.as_bytes() {
        hash ^= (*b) as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }

    format!("{AXI_DIGEST_V1_PREFIX}{hash:016x}")
}

/// Compute a stable id for a single *fact/tuple* extracted from an `axi_schema_v1` instance.
///
/// This is intended for certificates and for stable export/import identifiers.
///
/// Properties:
/// - deterministic
/// - stable under tuple *field order* changes (we use schema-declared order)
/// - non-cryptographic (same tradeoff as `axi_digest_v1`)
///
/// The `fields_in_decl_order` slice must be ordered according to the relation declaration.
pub fn axi_fact_id_v1(
    module_name: &str,
    schema_name: &str,
    instance_name: &str,
    relation_name: &str,
    fields_in_decl_order: &[(&str, &str)],
) -> String {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x00000100000001b3;

    fn add(hash: &mut u64, s: &str) {
        for b in s.as_bytes() {
            *hash ^= (*b) as u64;
            *hash = hash.wrapping_mul(FNV_PRIME);
        }
    }

    let mut hash = FNV_OFFSET_BASIS;

    add(&mut hash, "module=");
    add(&mut hash, module_name);
    add(&mut hash, "|schema=");
    add(&mut hash, schema_name);
    add(&mut hash, "|instance=");
    add(&mut hash, instance_name);
    add(&mut hash, "|relation=");
    add(&mut hash, relation_name);
    add(&mut hash, "|fields=");

    for (field, value) in fields_in_decl_order {
        add(&mut hash, field);
        add(&mut hash, "=");
        add(&mut hash, value);
        add(&mut hash, ";");
    }

    format!("{AXI_FACT_ID_V1_PREFIX}{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_has_expected_prefix_and_width() {
        let d = axi_digest_v1("module X\n");
        assert!(d.starts_with(AXI_DIGEST_V1_PREFIX));
        assert_eq!(d.len(), AXI_DIGEST_V1_PREFIX.len() + 16);
    }

    #[test]
    fn fact_id_has_expected_prefix_and_width() {
        let id = axi_fact_id_v1("M", "S", "I", "R", &[("a", "A"), ("b", "B")]);
        assert!(id.starts_with(AXI_FACT_ID_V1_PREFIX));
        assert_eq!(id.len(), AXI_FACT_ID_V1_PREFIX.len() + 16);
    }

    #[test]
    fn fact_id_changes_when_fields_change() {
        let id1 = axi_fact_id_v1("M", "S", "I", "R", &[("a", "A"), ("b", "B")]);
        let id2 = axi_fact_id_v1("M", "S", "I", "R", &[("a", "A"), ("b", "C")]);
        assert_ne!(id1, id2);
    }
}
