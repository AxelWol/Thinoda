#![no_main]

mod drawio_support;
mod support;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| drawio_support::exercise(data));
