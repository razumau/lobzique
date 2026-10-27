//! The Ruby-free part of lobzique: the engine that libxml2 and gumbo are for
//! Nokogiri. Code here must not depend on magnus or rb-sys, so Miri,
//! sanitizers and fuzzers can run it without a Ruby VM. See ARCHITECTURE.md.

#![forbid(unsafe_code)]

pub mod dom;
pub mod html5;
