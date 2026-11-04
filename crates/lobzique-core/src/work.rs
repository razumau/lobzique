//! A count of the work done, for fuzz targets that check how cost grows
//! with input: a time limit catches only catastrophic slowness, and timing
//! is too noisy to tell quadratic from linear. A unit is a node read or
//! created, a byte of text inserted or escaped, or a call from html5ever's
//! tokenizer or tree builder, whose own scans of the stack of open elements
//! go through `elem_name`. Counted only when built with `--cfg fuzzing`, as
//! cargo-fuzz builds; otherwise `add` compiles to nothing.

#[cfg(fuzzing)]
thread_local! {
    static UNITS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[inline(always)]
pub(crate) fn add(units: usize) {
    #[cfg(fuzzing)]
    UNITS.with(|count| count.set(count.get().saturating_add(units as u64)));
    #[cfg(not(fuzzing))]
    let _ = units;
}

/// The units counted on this thread since the last call.
#[cfg(fuzzing)]
pub fn take() -> u64 {
    UNITS.with(|count| count.replace(0))
}
