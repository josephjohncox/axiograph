#![no_main]

use libfuzzer_sys::fuzz_target;

const MAX_ADAPTER_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

fuzz_target!(|data: &[u8]| {
    let _ = axiograph_cli::proposal_adapter_boundary::parse_predictive_proposal_response_bounded(
        data,
        MAX_ADAPTER_RESPONSE_BYTES,
        "fuzzed predictive proposal adapter response",
    );
});
