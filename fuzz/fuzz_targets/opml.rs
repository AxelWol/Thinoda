#![no_main]

mod support;

use libfuzzer_sys::fuzz_target;
use thinoda_io::Format;

fuzz_target!(|data: &[u8]| support::roundtrip(Format::Opml, data));
