//! Ruby bindings: the native classes and methods Nokogiri's C extension
//! defines, with the same names, each a thin call into lobzique-core. Files
//! mirror `lib/lobzique/`; see ARCHITECTURE.md.
//!
//! No code borrowing an arena can reach Ruby (ARCHITECTURE.md, "No borrow
//! across Ruby"). The borrow closures must be `Send`, which keeps out Ruby
//! handles and values, and the other ways in are closed here and in
//! `clippy.toml`: `unsafe` (raw rb-sys calls) only in `support`, no
//! deprecated magnus API (it fetches a handle itself), and none of the
//! calls that fetch a handle, which `clippy.toml` lists.
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
    // Parents before children, as in Nokogiri's Init_nokogiri
    xml::init(ruby)?;
    html4::init(ruby)?;
    html5::init(ruby)?;
    html5ever::init(ruby)?;
    css::init(ruby)?;
    Ok(())
}
