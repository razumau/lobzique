//! XML and XHTML output as libxml2 writes it for Nokogiri, over the arena
//! DOM. XML parsing is out of scope (plan.md decision 4).

mod serialize;
#[cfg(test)]
mod tests;

pub use serialize::{save_options, write, HtmlUnsupported, Output};
