// SPDX-License-Identifier: MIT OR Apache-2.0

//! Any input must be accepted or rejected with issues — never panic, hang
//! or exhaust memory (TPL-011, SEC-009).

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = chalkboard_template::validate(data);
});
