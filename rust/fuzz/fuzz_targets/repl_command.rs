#![no_main]

use libfuzzer_sys::fuzz_target;

const MAX_REPL_LINE_BYTES: usize = 1024 * 1024;

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_REPL_LINE_BYTES {
        return;
    }
    if let Ok(line) = std::str::from_utf8(data) {
        let _ = axiograph_cli::repl_command::tokenize_repl_line(line);
    }
});
