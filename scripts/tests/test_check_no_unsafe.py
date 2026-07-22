from __future__ import annotations

import unittest

from scripts.check_no_unsafe import unsafe_keyword_lines


class UnsafeScannerTests(unittest.TestCase):
    def test_ignores_comments_strings_characters_lifetimes_and_raw_identifiers(
        self,
    ) -> None:
        source = r"""
// unsafe fn commented() {}
/* unsafe { nested(); } /* unsafe impl X {} */ */
const NORMAL: &str = "unsafe";
const BYTES: &[u8] = b"unsafe";
const RAW: &str = r###"unsafe"###;
const RAW_BYTES: &[u8] = br##"unsafe"##;
const CHARACTER: char = 'u';
fn borrow<'unsafe_name>(value: &'unsafe_name str) -> &'unsafe_name str { value }
let r#unsafe = "identifier";
"""
        self.assertEqual(unsafe_keyword_lines(source), [])

    def test_reports_every_rust_unsafe_keyword_with_exact_lines(self) -> None:
        source = "\n".join(
            [
                "unsafe fn function() {}",
                'unsafe extern "C" { fn foreign(); }',
                "unsafe trait Marker {}",
                "unsafe impl Marker for Thing {}",
                "fn call() { unsafe { foreign(); } }",
                "",
            ]
        )
        self.assertEqual(unsafe_keyword_lines(source), [1, 2, 3, 4, 5])

    def test_does_not_let_nested_comments_hide_following_unsafe_code(self) -> None:
        source = "/* outer /* inner */ closed */\nunsafe { call(); }\n"
        self.assertEqual(unsafe_keyword_lines(source), [2])


if __name__ == "__main__":
    unittest.main()
