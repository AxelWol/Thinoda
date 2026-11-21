#![no_main]

mod drawio_bundle_support;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| drawio_bundle_support::exercise(data));
