//! Application-level limits at Axiograph's untrusted CLI/network seams.
//!
//! The generic no-follow file reader, JSON depth guard, and process-group
//! runner live in `axiograph-security` so non-CLI crates use the same hardened
//! implementation. These constants define the CLI's narrower payload policy.

use anyhow::Result;
use std::io::Read;

pub(crate) use axiograph_security::{
    parse_json_bounded, read_file_bounded, read_utf8_file_bounded, read_utf8_stream_bounded,
    run_command_bounded, ProcessLimits, MAX_CHILD_RUNTIME,
};

pub(crate) const MAX_AXI_MODULE_BYTES: usize = 4 * 1024 * 1024;
pub(crate) const MAX_JSON_INPUT_BYTES: usize = 8 * 1024 * 1024;
pub(crate) const MAX_TEXT_INPUT_BYTES: usize = 8 * 1024 * 1024;
pub(crate) const MAX_BINARY_INPUT_BYTES: usize = 64 * 1024 * 1024;
pub(crate) const MAX_NETWORK_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
pub(crate) const MAX_NETWORK_ERROR_BYTES: usize = 64 * 1024;
pub(crate) const MAX_OUTPUT_BYTES: usize = 64 * 1024 * 1024;

pub(crate) fn write_output_bounded(
    path: impl AsRef<std::path::Path>,
    bytes: impl AsRef<[u8]>,
    label: &str,
) -> Result<()> {
    axiograph_security::write_file_atomic_bounded(path.as_ref(), bytes, MAX_OUTPUT_BYTES, label)
}

pub(crate) fn read_blocking_response_bounded(
    response: reqwest::blocking::Response,
    limit: usize,
    label: &str,
) -> Result<Vec<u8>> {
    if let Some(actual) = response.content_length() {
        if actual > u64::try_from(limit).unwrap_or(u64::MAX) {
            anyhow::bail!("{label} Content-Length {actual} exceeds {limit} bytes");
        }
    }
    axiograph_security::read_stream_bounded(response.take(u64::MAX), limit, label)
}
