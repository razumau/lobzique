//! Ruby bindings: the native classes and methods that the C extension
//! of Nokogiri defines, with the same names. Each is a thin call
//! into lobzique-core. Files mirror `lib/lobzique/`. See ARCHITECTURE.md.
//!
//! Code that borrows an arena must not reach Ruby (ARCHITECTURE.md, "Memory
//! model"). The borrow closures must be `Send`. The lints here
//! and in `clippy.toml` close the known ways around that: `unsafe` only
//! in `support`, and no magnus API that fetches a Ruby handle itself.
#![deny(unsafe_code, deprecated)]

mod css;
mod html4;
mod html5;
mod html5ever;
mod support;
mod xml;

use magnus::{value::Lazy, Error, Module, RModule, Ruby};

// The root is named only through the `Lobzique` constant, which a facade can
// point at its own module (plan.md decision 1).
pub static LOBZIQUE: Lazy<RModule> = Lazy::new(|ruby| ruby.define_module("Lobzique").unwrap());
pub static XML: Lazy<RModule> =
    Lazy::new(|ruby| ruby.get_inner(&LOBZIQUE).define_module("XML").unwrap());
pub static HTML4: Lazy<RModule> =
    Lazy::new(|ruby| ruby.get_inner(&LOBZIQUE).define_module("HTML4").unwrap());
pub static HTML5: Lazy<RModule> =
    Lazy::new(|ruby| ruby.get_inner(&LOBZIQUE).define_module("HTML5").unwrap());
pub static CSS: Lazy<RModule> =
    Lazy::new(|ruby| ruby.get_inner(&LOBZIQUE).define_module("CSS").unwrap());
pub static HTML5EVER: Lazy<RModule> = Lazy::new(|ruby| {
    ruby.get_inner(&LOBZIQUE)
        .define_module("Html5ever")
        .unwrap()
});

#[magnus::init]
fn init(ruby: &Ruby) -> Result<(), Error> {
    xml::init(ruby)?;
    html4::init(ruby)?;
    html5::init(ruby)?;
    html5ever::init(ruby)?;
    css::init(ruby)?;
    Ok(())
}
