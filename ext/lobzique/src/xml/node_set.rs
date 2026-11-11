//! `XML::NodeSet`, which is Ruby over an Array here (ARCHITECTURE.md,
//! departures); natively only the wrapping of a result.

use super::node::{wrap_all, Node};
use crate::XML;
use lobzique_core::dom::NodeId;
use magnus::{prelude::*, typed_data::Obj, value::Lazy, Error, RClass, Ruby, Value};

pub static NODE_SET: Lazy<RClass> = Lazy::new(|ruby| {
    ruby.get_inner(&XML)
        .const_get("NodeSet")
        .expect("lib/lobzique/xml/node_set.rb defines XML::NodeSet")
});

/// A NodeSet of the node objects for `ids`: `noko_xml_node_set_wrap`, which
/// is Ruby's `NodeSet.wrap` here.
pub fn wrap(ruby: &Ruby, document: Obj<Node>, ids: Vec<NodeId>) -> Result<Value, Error> {
    let nodes = wrap_all(ruby, document, ids)?;
    ruby.get_inner(&NODE_SET).funcall("wrap", (document, nodes))
}
