#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|subject: &str| {
    assert!(lobzique_core::hello(subject).contains(subject));
});
