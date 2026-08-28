#![no_main]

use libfuzzer_sys::fuzz_target;

const MAX_AXI_BYTES: usize = 4 * 1024 * 1024;

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_AXI_BYTES {
        return;
    }
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = axiograph_dsl::axi_v1::parse_axi_v1(text);
    }
});
