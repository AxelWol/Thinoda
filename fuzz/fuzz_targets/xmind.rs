#![no_main]

mod support;
mod xmind_support;

use libfuzzer_sys::fuzz_target;
use thinoda_io::Format;

fuzz_target!(|data: &[u8]| {
    if let Some(input) = xmind_support::package_input(data) {
        support::roundtrip(Format::XMind, &input);
    }
});
