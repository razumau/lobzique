# frozen_string_literal: true

require "test_helper"
require "nokogiri"

# Changing a tree: reparenting, libxml2's text merging, copying and editing
# attributes and content. Adapted from
# test/fixtures/nokogiri/xml/test_node_reparenting.rb and test_node.rb onto
# HTML5 documents, finding nodes by walking until CSS and XPath land.
module NodeReparentingContract
  HTML = "<div id=d>a<b x=1>B<i>I</i></b>c<!--m--></div><svg><g><rect/></g></svg>"

  def document(html = HTML) = root::HTML5(html)

  def at(node, name)
    node.traverse { |found| return found if found.name == name }
    nil
  end

  def markup(nodes) = nodes.map(&:to_html)

  def test_add_child_moves_a_node_and_keeps_its_object
    doc = document
    div, b = at(doc, "div"), at(doc, "b")
    i = at(doc, "i")
    assert_same i, div.add_child(i)
    assert_same div, i.parent
    assert_same i, div.children.last
    assert_equal "<b x=\"1\">B</b>", b.to_html
    assert_same div, (div << root::XML::Node.new("u", doc))
  end

  def test_add_child_merges_text_into_trailing_text
    doc = root::HTML5("<div>a<b>B</b>c</div>")
    div = at(doc, "div")
    c = div.children.last
    text = doc.create_text_node("x")

    assert_same text, div.add_child(text)
    assert_equal ["a", "<b>B</b>", "cx"], markup(div.children)
    assert_same c, div.children.last
    # The object now stands for the node it merged into
    assert_equal "cx", text.content
    assert_equal c, text
    refute_same c, text
  end

  def test_sibling_inserts_copy_text_without_merging
    doc = document
    div, b = at(doc, "div"), at(doc, "b")
    a, c = div.children.first, div.children[2]
    before, after = doc.create_text_node("1"), doc.create_text_node("2")
    b.add_previous_sibling(before)
    b.add_next_sibling(after)

    assert_equal ["a", "1", "<b x=\"1\">B<i>I</i></b>", "2", "c", "<!--m-->"], markup(div.children)
    assert_same a, div.children.first
    assert_same c, div.children[4]
    assert_equal div.children[1], before
    refute_same div.children[1], before
    assert_same doc, before.document
  end

  def test_replace_merges_text_on_both_sides
    doc = document
    div, b = at(doc, "div"), at(doc, "b")
    a, c = div.children.first, div.children[2]
    text = doc.create_text_node("x")
    assert_same text, b.replace(text)

    assert_equal ["axc", "<!--m-->"], markup(div.children)
    assert_same a, div.children.first
    assert_equal "axc", text.content
    assert_nil b.parent
    # The text after was swapped for a copy first, so its object survives
    assert_equal "c", c.content
    assert_nil c.parent
  end

  def test_replacing_text_keeps_neighbors
    doc = document
    div = at(doc, "div")
    a = div.children.first
    replacement = doc.create_text_node("x")
    a.replace(replacement)
    assert_equal ["x", "<b x=\"1\">B<i>I</i></b>", "c", "<!--m-->"], markup(div.children)
    assert_nil a.parent
    assert_equal "x", replacement.content
  end

  def test_markup_is_parsed_in_context
    doc = document
    div, b = at(doc, "div"), at(doc, "b")
    added = div.add_child("<u>1</u>tail")
    assert_instance_of root::XML::NodeSet, added
    assert_equal ["<u>1</u>", "tail"], markup(added)
    assert_equal "<!--m--><u>1</u>tail", markup(div.children.to_a.last(3)).join

    replaced = b.replace("<s>2</s><s>3</s>")
    assert_equal ["<s>2</s>", "<s>3</s>"], markup(replaced)
    assert_nil b.parent
    assert_equal "a<s>2</s><s>3</s>c", markup(div.children.to_a.first(4)).join

    table = root::HTML5("<table><tr><td>x</td></tr></table>")
    td = at(table, "td")
    td.add_next_sibling("<td>y</td>")
    assert_equal "<tr><td>x</td><td>y</td></tr>", at(table, "tr").to_html
  end

  def test_sibling_methods
    doc = document
    div, b = at(doc, "div"), at(doc, "b")
    assert_same b, b.before("<hr>")
    assert_same b, b.after("<br>")
    returned = b.add_previous_sibling("<em>e</em>")
    assert_equal ["<em>e</em>"], markup(returned)
    assert_equal "a<hr><em>e</em><b x=\"1\">B<i>I</i></b><br>c<!--m-->", div.inner_html

    b.previous = root::XML::Node.new("p", doc)
    b.next = root::XML::Node.new("q", doc)
    assert_equal ["p", "b", "q"], [b.previous.name, b.name, b.next.name]
  end

  def test_node_sets_keep_their_order_next_to_text
    doc = document
    a = at(doc, "div").children.first
    a.add_next_sibling("<i>1</i><i>2</i>")
    a.add_previous_sibling("<i>3</i><i>4</i>")
    assert_equal "<i>3</i><i>4</i>a<i>1</i><i>2</i><b x=\"1\">", at(doc, "div").inner_html[0, 42]
  end

  def test_children_and_inner_html
    doc = document
    div = at(doc, "div")
    b = at(doc, "b")
    div.inner_html = "<p>1</p>2"
    assert_equal "<p>1</p>2", div.inner_html
    assert_nil b.parent

    div.children = doc.create_text_node("x")
    assert_equal "x", div.inner_html
    div.children = root::XML::NodeSet.new(doc, [root::XML::Node.new("p", doc), root::XML::Node.new("q", doc)])
    assert_equal "<p></p><q></q>", div.inner_html
  end

  def test_replacing_a_child_leaves_sibling_text_alone
    doc = root::HTML5("<div><p>asdf</p>qwer</div>")
    paragraph = at(doc, "p")
    sibling = paragraph.next
    paragraph.inner_html = "foo"
    assert_same sibling, paragraph.next
  end

  def test_prepend_child_swap_and_wrap
    doc = document
    div, b = at(doc, "div"), at(doc, "b")
    div.prepend_child("<hr>")
    assert_equal "hr", div.children.first.name
    empty = root::XML::Node.new("p", doc)
    empty.prepend_child(doc.create_text_node("t"))
    assert_equal "<p>t</p>", empty.to_html

    assert_same b, b.swap("<u>1</u>")
    assert_nil b.parent

    i = root::XML::Node.new("i", doc)
    div.add_child(i)
    assert_same i, i.wrap("<p class=w></p>")
    assert_equal "<p class=\"w\"><i></i></p>", div.children.last.to_html
    span = root::XML::Node.new("span", doc)
    i.wrap(span)
    assert_equal "<p class=\"w\"><span><i></i></span></p>", div.children.last.to_html
    assert_nil span.parent, "wrapping in a node wraps in its copy"
  end

  def test_unlink_and_remove
    doc = document
    div, b = at(doc, "div"), at(doc, "b")
    assert_same b, b.unlink
    assert_nil b.parent
    assert_equal "ac<!--m-->", div.inner_html
    assert_equal "<b x=\"1\">B<i>I</i></b>", b.to_html
    div.add_child(b)
    assert_same div, b.parent
    at(doc, "i").remove
    assert_equal "<b x=\"1\">B</b>", b.to_html
  end

  def test_parent_assignment
    doc = document
    i = at(doc, "i")
    i.parent = at(doc, "div")
    assert_equal "div", i.parent.name
  end

  def test_reparenting_rejects_other_arguments
    doc = document
    div = at(doc, "div")
    [doc, div.attribute("id"), 1].each do |argument|
      error = assert_raises(ArgumentError) { div.add_child(argument) }
      assert_match(/\ARequires a Node, NodeSet or String argument, and cannot accept a #{argument.class}\./, error.message)
    end
    error = assert_raises(ArgumentError) { div.send(:add_child_node, doc) }
    assert_equal "node must be a #{root}::XML::Node", error.message
    error = assert_raises(ArgumentError) { div.children.first.add_child(doc.create_text_node("x")) }
    assert_equal "cannot reparent #{root}::XML::Text there", error.message
    error = assert_raises(ArgumentError) { at(doc, "b").send(:add_child_node, doc.fragment) }
    assert_equal "cannot reparent #{root}::HTML5::DocumentFragment there", error.message
  end

  def test_a_node_cannot_go_into_or_next_to_itself
    doc = document
    b, i = at(doc, "b"), at(doc, "i")
    error = assert_raises(RuntimeError) { b.add_next_sibling(b) }
    assert_equal "Could not reparent node", error.message
    assert_nil b.parent, "Nokogiri unlinks the node before libxml2 refuses"

    error = assert_raises(RuntimeError) { i.add_child(i) }
    assert_equal "Could not reparent node", error.message
  end

  def test_cycles_are_refused
    doc = document
    div, i = at(doc, "div"), at(doc, "i")
    error = assert_raises(RuntimeError) { div.parent = i }
    assert_equal "cycle detected: node 'div' is an ancestor of itself", error.message
  end

  def test_a_document_has_one_root
    doc = document
    error = assert_raises(RuntimeError) { doc.add_child(root::XML::Node.new("p", doc)) }
    assert_equal "A document may not have multiple root nodes.", error.message
    error = assert_raises(ArgumentError) { doc.root.add_next_sibling(root::XML::Node.new("p", doc)) }
    assert_equal "A document may not have multiple root nodes.", error.message
    doc.add_child(doc.create_comment("end"))
    assert_equal "<!--end-->", doc.children.last.to_html
  end

  def test_root_assignment
    doc = document
    old_root = doc.root
    new_root = root::XML::Node.new("html", doc)
    assert_same new_root, (doc.root = new_root)
    assert_same new_root, doc.root
    assert_nil old_root.parent

    other = root::HTML5("<p>q</p>")
    foreign = other.root
    doc.root = foreign
    assert_same other, foreign.document, "a foreign root is copied"
    refute_same foreign, doc.root
    assert_equal foreign.to_html, doc.root.to_html
    assert_same foreign, other.root

    doc.root = nil
    assert_nil doc.root
    error = assert_raises(TypeError) { doc.root = doc.create_text_node("x") }
    assert_equal "root must be a #{root}::XML::Element", error.message
    error = assert_raises(ArgumentError) { doc.root = 1 }
    assert_equal "expected #{root}::XML::Node but received Integer", error.message
  end

  def test_text_from_another_document
    doc = document
    other = root::HTML5("<p>q</p>")
    text = at(other, "p").children.first
    at(doc, "b").add_next_sibling(text)
    assert_same doc, text.document
    assert_equal "div", text.parent.name
    assert_empty at(other, "p").children
  end

  def test_fragments_add_their_children
    doc = document
    div = at(doc, "div")
    fragment = doc.fragment("<u>1</u>x")
    children = fragment.children.to_a
    added = div.add_child(fragment)
    assert_equal ["<u>1</u>", "x"], markup(added)
    assert_empty fragment.children
    assert_same div, children.first.parent

    foreign = root::HTML5("<p></p>").fragment("<s>2</s>")
    div.add_child(foreign)
    assert_equal "<s>2</s>", div.children.last.to_html
    assert_empty foreign.children
  end

  def test_reparented_nodes_are_decorated_again
    doc = document
    decorator = Module.new
    i = at(doc, "i")
    doc.decorators(root::XML::Node) << decorator
    at(doc, "div").add_child(i)
    assert_kind_of decorator, i
    copy = doc.create_text_node("t")
    at(doc, "b").add_next_sibling(copy)
    assert_kind_of decorator, at(doc, "b").next
  end

  def test_dup
    doc = document
    b = at(doc, "b")
    assert_equal "<b x=\"1\">B<i>I</i></b>", b.dup.to_html
    assert_equal "<b></b>", b.dup(0).to_html
    assert_equal "<b x=\"1\"></b>", b.dup(2).to_html
    copy = b.clone
    assert_nil copy.parent
    assert_same doc, copy.document
    refute_equal b, copy

    other = root::HTML5("<p></p>")
    moved = b.dup(1, other)
    assert_same other, moved.document
    assert_equal b.to_html, moved.to_html

    text = at(doc, "div").children.first
    assert_instance_of root::XML::Text, text.dup
    assert_equal "a", text.dup(0).content
    attribute = b.attribute("x").dup
    assert_instance_of root::XML::Attr, attribute
    assert_equal ["x", "1"], [attribute.name, attribute.value]
    assert_nil attribute.parent
    assert_nil root::HTML5("<!doctype html>").internal_subset.dup
  end

  def test_copies_are_decorated_and_keep_singletons_on_clone
    doc = document
    decorator, extra = Module.new, Module.new
    doc.decorators(root::XML::Node) << decorator
    b = at(doc, "b")
    b.extend(extra)
    assert_kind_of decorator, b.dup
    refute_kind_of extra, b.dup
    assert_kind_of extra, b.clone
  end

  def test_document_dup
    doc = document
    doc.errors = [:error]
    copy = doc.dup
    assert_instance_of root::HTML5::Document, copy
    assert_equal doc.to_html, copy.to_html
    refute_same doc.root, copy.root
    assert_same doc.errors, copy.errors
    assert_equal "", doc.dup(0).to_html
    assert_empty doc.dup(0).children
    assert_equal root::XML::Document, root::XML::Document.new.dup.class

    at(copy, "b").unlink
    assert at(doc, "b").parent
  end

  def test_fragment_dup_and_clone
    doc = document
    fragment = doc.fragment("<p>1</p>2")
    copy = fragment.dup
    assert_instance_of root::HTML5::DocumentFragment, copy
    assert_equal "<p>1</p>2", copy.to_html
    refute_same doc, copy.document

    clone = fragment.clone
    assert_equal "<p>1</p>2", clone.to_html
    assert_same doc, clone.document
  end

  def test_setting_attributes
    doc = document
    div = at(doc, "div")
    id = div.attribute("id")
    div["data-x"] = 5
    div[:id] = "<&\">"
    div.set_attribute("a:b", "v")
    div["xml:lang"] = "de"
    div["ID"] = "upper"
    assert_equal %(<div id="<&amp;&quot;>" data-x="5" a:b="v" xml:lang="de" ID="upper">), div.to_html[/\A<div.*?ID="upper">/]
    assert_same id, div.attribute("id"), "an existing attribute keeps its node"
    assert_equal "<&\">", id.value
    assert_equal "de", div.lang
    assert_nil div.children.first.send(:set, "a", "b")
  end

  def test_removing_attributes
    doc = document
    div = at(doc, "div")
    id = div.attribute("id")
    assert_same id, div.remove_attribute("id")
    assert_nil id.parent
    assert_nil div.remove_attribute("missing")
    div["class"] = "a"
    div.delete("class")
    assert_equal "<div>", div.to_html[/\A<div[^>]*>/]
  end

  def test_class_keywords
    div = at(document, "div")
    assert_same div, div.add_class("x y")
    div.add_class(["y", "z"])
    div.append_class("x")
    assert_equal "x y z x", div["class"]
    div.remove_class("x")
    assert_equal "y z", div["class"]
    div.kwattr_add("rel", "a")
    div.kwattr_remove("rel", "a")
    refute div.key?("rel")
    div.remove_class
    refute div.key?("class")
    assert_raises(ArgumentError) { div.add_class(1) }
  end

  def test_attribute_values
    div = at(document, "div")
    id = div.attribute("id")
    id.value = %(<&"'>)
    assert_equal %(<&"'>), div["id"]
    id.content = ""
    assert_equal %(<div id="">), div.to_html[/\A<div[^>]*>/]
    id.value = nil
    assert_equal "", id.value
  end

  def test_names_and_lang
    doc = document
    div = at(doc, "div")
    div.name = "section"
    assert_equal "<section id=\"d\">", div.to_html[/\A<section[^>]*>/]
    text = div.children.first
    text.name = "other"
    assert_equal "text", text.name
    div.lang = "fr"
    assert_equal "fr", div.lang
    assert_equal "fr", div["xml:lang"]
  end

  def test_content_assignment
    doc = document
    div = at(doc, "div")
    div.content = "<x>&amp;\r"
    assert_equal "<x>&amp;\r", div.content
    assert_equal 1, div.children.size
    div.content = ""
    assert_empty div.children

    comment = doc.create_comment("c")
    comment.content = "a<b&"
    assert_equal "a&lt;b&amp;", comment.content, "Node#content= escapes for character data too"

    text = doc.create_text_node("t")
    text.content = "<y>"
    assert_equal "<y>", text.content

    doc.content = "x"
    assert_empty doc.children
  end

  def test_native_content_decodes_references_in_elements
    div = at(document, "div")
    div.native_content = "&lt;&#65;&#x42;&amp;amp;&gt;"
    assert_equal "<AB&amp;>", div.content
  end

  def test_prefixed_names_find_their_namespace_when_reparented
    doc = document
    g = at(doc, "g")
    circle = root::XML::Node.new("svg:circle", doc)
    g.add_child(circle)
    assert_equal "circle", circle.name
    assert_equal ["svg", "http://www.w3.org/2000/svg"], [circle.namespace.prefix, circle.namespace.href]
    assert_equal "<g><rect></rect><circle></circle></g>", g.to_html
  end

  def test_names_with_an_empty_side_of_the_colon_are_not_prefixed
    doc = root::HTML5(%(<svg><a xlink:href=x></a></svg><svg><g xlink:=y></g></svg>))
    first, second = at(doc, "body").children.to_a
    a = at(first, "a")
    a["xlink:"] = "v"
    first.add_previous_sibling(second)
    names = ->(node) { node.attribute_nodes.map { |attribute| [attribute.name, attribute.namespace&.prefix] } }
    assert_equal [["href", "xlink"], ["xlink:", nil]], names.call(a)
    assert_equal [["xlink:", nil]], names.call(at(second, "g"))
  end

  def test_copies_define_the_namespaces_they_use
    doc = root::HTML5(%(<svg xmlns:xlink="http://www.w3.org/1999/xlink"><a xlink:href="h">x</a></svg>))
    svg = at(doc, "svg")
    definitions = ->(node) { node.namespace_definitions.map { |ns| [ns.prefix, ns.href] } }
    assert_equal [["svg", "http://www.w3.org/2000/svg"], ["xlink", "http://www.w3.org/1999/xlink"]],
      definitions[svg.children.first.dup]
    assert_equal definitions[doc.root], definitions[svg.dup]
    assert_nil svg.dup(0).namespace

    other = root::HTML5("<p></p>")
    g = at(document, "g")
    at(other, "p").add_child(g)
    assert_equal [["svg", "http://www.w3.org/2000/svg"]], definitions[g]
  end

  def test_definitions_already_in_scope_are_dropped
    doc = root::HTML5("<svg></svg><div></div>")
    div = at(doc, "div")
    div.add_child("<svg><g/></svg>")
    assert_empty div.children.first.namespace_definitions

    plain = root::HTML5("<div></div>")
    at(plain, "div").add_child("<svg><g/></svg>")
    assert_equal ["svg"], at(plain, "div").children.first.namespace_definitions.map(&:prefix)
  end

  def test_create_internal_subset
    doc = root::HTML5("<p>")
    dtd = doc.create_internal_subset("html", nil, nil)
    assert_instance_of root::XML::DTD, dtd
    assert_same dtd, doc.children.first
    assert_equal "<!DOCTYPE html><html>", doc.to_html[0, 21]
    error = assert_raises(RuntimeError) { doc.create_internal_subset("x", nil, nil) }
    assert_equal "Document already has an internal subset", error.message

    xml = root::XML::Document.new
    xml.root = root::XML::Node.new("r", xml)
    xml.add_child(xml.create_comment("c"))
    dtd = xml.create_internal_subset("r", "pub", "sys")
    assert_equal [root::XML::DTD, root::XML::Element, root::XML::Comment], xml.children.map(&:class)
    assert_equal ["pub", "sys"], [dtd.external_id, dtd.system_id]
  end

  def test_create_element
    doc = document
    element = doc.create_element("span", "<content>", {class: "x"}) { |e| e["y"] = "z" }
    assert_equal %(<span y="z" class="x">&lt;content&gt;</span>), element.to_html
    assert_same doc, element.document
  end

  def test_node_set_mutation
    doc = document
    div = at(doc, "div")
    set = div.children
    assert_same set, set.attr("data-x", "1")
    assert_equal "1", at(doc, "b")["data-x"]
    assert_equal "1", set.attr("data-x").value if set.first.element?
    set.add_class("k").append_class("k").remove_class("k")
    refute at(doc, "b").key?("class")
    set.attr("data-y") { |node| node.name }
    assert_equal "b", at(doc, "b")["data-y"]
    set.remove_attr("data-y")
    refute at(doc, "b").key?("data-y")

    elements = div.element_children
    elements.wrap("<p></p>")
    assert_equal "p", at(doc, "b").parent.name
    elements.before("<hr>")
    elements.after("<br>")
    assert_equal "<p><hr><b", div.inner_html[1, 9], "the set still holds the wrapped nodes"

    unlinked = div.children
    assert_same unlinked, unlinked.unlink
    assert_empty div.children
    assert unlinked.all? { |node| node.parent.nil? }
    div.add_child("<u></u>")
    div.children.remove
    assert_empty div.children
  end

  def test_unlinked_nodes_survive_collection
    doc = document
    nodes = at(doc, "div").children.to_a
    at(doc, "div").children.unlink
    GC.start
    nodes.each { |node| at(doc, "body").add_child(node) }
    assert_match(%r{</svg>a<b x="1">B<i>I</i></b>c<!--m-->\z}, at(doc, "body").inner_html)
  end
end

class TestNodeReparentingNokogiri < Minitest::Test
  include NodeReparentingContract

  def root = Nokogiri
end

class TestNodeReparentingLobzique < Minitest::Test
  include NodeReparentingContract

  def root = Lobzique

  # A departure: libxml2 links the cycle before Nokogiri raises
  def test_cycles_leave_the_tree_unchanged
    doc = document
    div, i = at(doc, "div"), at(doc, "i")
    assert_raises(RuntimeError) { div.parent = i }
    assert_equal "body", div.parent.name
    assert_equal "b", i.parent.name
  end

  # A departure: attribute values are bytes, so attributes take no children
  def test_attributes_take_no_children
    doc = document
    attribute = at(doc, "div").attribute("id")
    error = assert_raises(ArgumentError) { attribute.send(:add_child_node, doc.create_text_node("x")) }
    assert_equal "cannot reparent Lobzique::XML::Text there", error.message
  end
end
