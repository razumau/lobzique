# frozen_string_literal: true

require "test_helper"
require "nokogiri"

# Reading a tree: traversal, paths, document order, namespaces and the
# content aliases. Adapted from test/fixtures/nokogiri/xml/test_node.rb onto
# HTML5 documents, walking children until CSS and XPath land.
module NodeContract
  HTML = <<~HTML.delete("\n")
    <!DOCTYPE html><html><body>
    <p id=a class="x  y">one<b>two</b><!--three--><i>four</i>five</p>
    <p xml:lang=fr lang=en>six</p>
    <svg xmlns:xlink="http://www.w3.org/1999/xlink" xml:lang=de><a xlink:href=h xml:lang=it></a><g/><svg:g/></svg>
    <math><mi/></math>
    </body></html>
  HTML

  def test_children_and_siblings
    first, second = paragraphs
    one, bold, comment, italic, five = first.children.to_a

    assert_equal "one", first.child.content
    assert_equal bold, one.next_sibling
    assert_equal bold, one.next
    assert_equal one, bold.previous_sibling
    assert_equal one, bold.previous
    assert_nil five.next_sibling
    assert_nil one.previous_sibling
    assert_equal italic, bold.next_element
    assert_equal bold, comment.previous_element
    assert_nil italic.next_element
    assert_nil one.previous_element
    assert_equal second, first.next_element
  end

  def test_element_children
    first, = paragraphs
    assert_equal ["b", "i"], first.element_children.map(&:name)
    assert_equal ["b", "i"], first.elements.map(&:name)
    assert_equal "b", first.first_element_child.name
    assert_equal "i", first.last_element_child.name
    assert_instance_of root::XML::NodeSet, first.element_children

    text = first.child
    assert_empty text.element_children
    assert_nil text.first_element_child
    assert_nil text.last_element_child
  end

  def test_document_children
    doc = document
    assert_instance_of root::XML::DTD, doc.child
    assert_equal "html", doc.child.next_element.name
    assert_equal "html", doc.root.name
    assert_equal doc.root, doc.first_element_child
    assert_nil doc.next_sibling
    assert_nil doc.next_element
    assert_nil root::HTML5::Document.new.root
    assert_nil root::XML::Document.new.root
  end

  # Attributes are siblings of each other in libxml2
  def test_attribute_siblings
    id, klass = paragraphs.first.attribute_nodes
    assert_equal klass, id.next_sibling
    assert_equal id, klass.previous_sibling
    assert_nil id.previous_sibling
    assert_nil klass.next_sibling
    assert_nil id.next_element
    assert_nil klass.previous_element
  end

  def test_text_aliases
    first, second = paragraphs
    assert_equal "onetwofourfive", first.text
    assert_equal "onetwofourfive", first.inner_text
    assert_equal "onetwofourfive", first.to_str
    assert_equal "onetwofourfivesix", root::XML::NodeSet.new(document, [first, second]).text
    assert_equal "onetwofourfivesix", root::XML::NodeSet.new(document, [first, second]).inner_text
  end

  def test_node_set_children
    first, second = paragraphs
    set = root::XML::NodeSet.new(document, [first, second]).children
    assert_instance_of root::XML::NodeSet, set
    assert_equal ["one", "b", "comment", "i", "five", "six"], set.map { |n| n.text? ? n.content : n.name }
  end

  def test_blank
    doc = document
    assert_predicate root::XML::Text.new("\t \n\r", doc), :blank?
    assert_predicate root::XML::Text.new("", doc), :blank?
    assert_predicate root::XML::CDATA.new(doc, "\t \n"), :blank?
    refute_predicate root::XML::Text.new(" \f", doc), :blank?
    refute_predicate root::XML::Text.new("not-blank", doc), :blank?
    refute_predicate root::XML::Comment.new(doc, " "), :blank?
    refute_predicate root::XML::Node.new("p", doc), :blank?
    refute_predicate doc, :blank?
  end

  def test_encode_special_chars
    node = paragraphs.first
    assert_equal "&amp;", node.encode_special_chars("&")
    assert_equal "&lt;&gt;&amp;&quot;'&#13;\n\té", node.encode_special_chars(%(<>&"'\r\n\té))
    assert_equal Encoding::UTF_8, node.encode_special_chars("a\xffb".b).encoding
    assert_raises(TypeError) { node.encode_special_chars(nil) }
  end

  def test_path
    doc = document
    first, = paragraphs(doc)
    one, _, comment, _, five = first.children.to_a
    svg = doc.root.children.last.children[2]
    assert_equal "/", doc.path
    assert_equal "?", doc.internal_subset.path
    assert_equal "/html", doc.root.path
    assert_equal "/html/body/p[1]", first.path
    assert_equal "/html/body/p[1]/text()[1]", one.path
    assert_equal "/html/body/p[1]/comment()", comment.path
    assert_equal "/html/body/p[1]/text()[2]", five.path
    assert_equal "/html/body/p[1]/@class", first.attribute_nodes[1].path
    assert_equal "/html/body/svg:svg", svg.path
    assert_equal ["/html/body/svg:svg/svg:a", "/html/body/svg:svg/svg:g", "/html/body/svg:svg/svg:svg:g"],
      svg.children.map(&:path)
    assert_equal "/html/body/svg:svg/svg:a/@xlink:href", svg.child.attribute_nodes.first.path
    assert_equal "/html/body/math:math/math:mi", svg.next_sibling.child.path
    assert_equal "html > body > svg:svg > svg:a", svg.child.css_path
    assert_equal "html > body > p:nth-of-type(1) > text():nth-of-type(2)", five.css_path
  end

  def test_path_outside_the_document
    doc = document
    assert_equal "/div", root::XML::Node.new("div", doc).path
    assert_equal "/text()", doc.create_text_node("t").path
    fragment = root::HTML5::DocumentFragment.parse("<div><span>x</span></div>")
    assert_equal ["?", "?", "?"], [fragment, fragment.child, fragment.child.child].map(&:path)
  end

  def test_ancestors
    doc = document
    first, = paragraphs(doc)
    bold = first.children[1]
    assert_equal ["p", "body", "html", "document"], bold.ancestors.map(&:name)
    assert_instance_of root::XML::NodeSet, bold.ancestors
    assert_equal ["b", "p", "body", "html", "document"], bold.child.ancestors.map(&:name)
    assert_equal ["p", "body", "html", "document"], first.attribute_nodes.first.ancestors.map(&:name)
    assert_empty doc.ancestors
    assert_empty root::XML::Node.new("div", doc).ancestors
  end

  def test_traverse
    first, = paragraphs
    visited = []
    first.traverse { |node| visited << (node.text? ? node.content : node.name) }
    assert_equal ["one", "two", "b", "comment", "four", "i", "five", "p"], visited
  end

  def test_accept
    visitor = Class.new do
      attr_reader :visited

      def visit(node)
        (@visited ||= []) << node.name
        node.children.each { |child| child.accept(self) }
      end
    end.new
    paragraphs.first.children[1].accept(visitor)
    assert_equal ["b", "text"], visitor.visited
  end

  def test_equality
    doc = document
    first, second = paragraphs(doc)
    other_first, = paragraphs(document)
    assert_equal first, doc.root.children.last.children.first
    assert_equal first, second.previous_element
    refute_equal first, second
    refute_equal first, other_first
    refute_equal first, nil
    refute_equal first, "one"
    assert_equal doc, first.document
    assert_kind_of Integer, first.pointer_id
    assert_equal first.pointer_id, second.previous_element.pointer_id
  end

  def test_document_order
    doc = document
    first, second = paragraphs(doc)
    one, bold = first.children.to_a
    id, klass = first.attribute_nodes

    assert_equal(-1, first <=> second)
    assert_equal 1, second <=> first
    assert_equal 0, first <=> first # rubocop:disable Lint/BinaryOperatorWithIdenticalOperands
    assert_equal [first, second], [second, first].sort
    assert_equal(-1, first <=> one)
    assert_equal 1, bold.child <=> first
    assert_equal 1, first <=> doc
    assert_equal(-1, doc <=> first)
    # An attribute comes after its element and before the element's children
    assert_equal 1, id <=> first
    assert_equal(-1, id <=> one)
    assert_equal(-1, id <=> klass)
    assert_equal 1, klass <=> id
    assert_equal(-1, id <=> second)
  end

  def test_document_order_across_trees
    doc = document
    first, = paragraphs(doc)
    unlinked = root::XML::Node.new("div", doc)
    assert_equal(-2, unlinked <=> first)
    assert_equal(-2, first <=> unlinked)
    assert_nil first <=> paragraphs(document).first
    assert_nil first <=> "p"
  end

  # gumbo defines the foreign namespaces on the html element, wherever
  # they are used, and libxml2 never defines xml
  def test_namespace_definitions
    doc = document
    definitions = doc.root.namespace_definitions
    assert_equal [
      ["svg", "http://www.w3.org/2000/svg"],
      ["xmlns", "http://www.w3.org/2000/xmlns/"],
      ["xlink", "http://www.w3.org/1999/xlink"],
      ["math", "http://www.w3.org/1998/Math/MathML"]
    ], definitions.map { |ns| [ns.prefix, ns.href] }
    assert_instance_of root::XML::Namespace, definitions.first
    assert_empty paragraphs(doc).first.namespace_definitions
    assert_empty root::HTML5("<p>").root.namespace_definitions
  end

  def test_namespaces_in_scope
    doc = document
    first, = paragraphs(doc)
    expected = {
      "xmlns:svg" => "http://www.w3.org/2000/svg",
      "xmlns:xmlns" => "http://www.w3.org/2000/xmlns/",
      "xmlns:xlink" => "http://www.w3.org/1999/xlink",
      "xmlns:math" => "http://www.w3.org/1998/Math/MathML"
    }
    assert_equal expected, first.namespaces
    assert_equal expected, doc.namespaces
    assert_equal expected.keys, first.child.namespace_scopes.map { |ns| "xmlns:#{ns.prefix}" }
    assert_equal({}, root::HTML5("<p>").root.namespaces)
    assert_equal({}, root::HTML5::Document.new.namespaces)
    assert_empty root::XML::Node.new("div", doc).namespace_scopes
  end

  # A fragment's top-level elements each hold their own definitions
  def test_fragment_namespace_definitions
    fragment = root::HTML5::DocumentFragment.parse(%(<div><svg/></div><math/><svg><a xlink:href="h"/></svg>))
    assert_equal [["svg"], ["math"], ["svg", "xlink"]],
      fragment.children.map { |child| child.namespace_definitions.map(&:prefix) }
    assert_equal "svg", fragment.child.child.namespace.prefix
  end

  def test_lang
    doc = document
    first, second = paragraphs(doc)
    svg = doc.root.children.last.children[2]
    # xml:lang on an HTML element is an attribute named "xml:lang" in no namespace
    assert_nil second.lang
    assert_nil first.lang
    assert_equal "de", svg.lang
    assert_equal "it", svg.child.lang
    assert_equal "it", svg.child.attribute_nodes.first.lang
    assert_equal "de", svg.children[1].lang
    assert_nil doc.lang
  end

  def test_document_and_doctype_leave_out_node_methods
    doc = document
    [:parent, :namespace, :attributes, :namespace_definitions].each do |name|
      refute_respond_to doc, name
    end
    [:values, :namespace_definitions, :attribute_nodes].each do |name|
      refute_respond_to doc.internal_subset, name
    end
  end

  def test_pattern_matching
    doc = document
    first, = paragraphs(doc)
    assert_equal(
      {name: "p", content: "onetwofourfive", namespace: nil},
      first.deconstruct_keys([:name, :content, :namespace, :unknown])
    )
    assert_equal first.attribute_nodes, first.deconstruct_keys([:attributes])[:attributes]
    assert_equal ["b", "i"], first.deconstruct_keys([:elements])[:elements].map(&:name)
    assert_equal({root: doc.root}, doc.deconstruct_keys([]))
    id = first.attribute_nodes.first
    assert_equal({name: "id", value: "a", namespace: nil}, id.deconstruct_keys([]))
    fragment = root::HTML5::DocumentFragment.parse("<b>x</b>y")
    assert_equal fragment.children.to_a, fragment.deconstruct
  end

  private

  def document = root::HTML5(HTML)

  def paragraphs(doc = document)
    doc.root.children.last.element_children.select { |node| node.name == "p" }
  end
end

class TestNodeNokogiri < Minitest::Test
  include NodeContract

  def root = Nokogiri
end

class TestNodeLobzique < Minitest::Test
  include NodeContract

  def root = Lobzique

  # A departure: attribute values aren't text nodes (ARCHITECTURE.md)
  def test_attributes_have_no_children
    id = root::HTML5("<p id=a>").root.children.last.child.attribute_nodes.first
    assert_nil id.child
    assert_empty id.children
  end
end
