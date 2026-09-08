//! Query/checker boundary policy, using the shared hardened I/O implementation.
#[cfg(test)]
pub(crate) use axiograph_security::read_utf8_file_bounded;
pub(crate) use axiograph_security::{parse_json_bounded, run_command_bounded, ProcessLimits};
pub(crate) const MAX_AXI_MODULE_BYTES: usize = 4 * 1024 * 1024;
pub(crate) const MAX_JSON_INPUT_BYTES: usize = 8 * 1024 * 1024;
#[cfg(test)]
pub(crate) const MAX_TEXT_INPUT_BYTES: usize = 8 * 1024 * 1024;
#[cfg(test)]
pub(crate) fn write_output_bounded(
    path: impl AsRef<std::path::Path>,
    bytes: impl AsRef<[u8]>,
    label: &str,
) -> anyhow::Result<()> {
    axiograph_security::write_file_atomic_bounded(path.as_ref(), bytes, 64 * 1024 * 1024, label)
}
