# frozen_string_literal: true

require "test_helper"
require "nokogiri"

# How node objects are made, cached and decorated (plan.md decision 5). The
# same tests run against Nokogiri, so each assertion is checked against the
# behavior being copied.
module ObjectModelContract
  def test_wrapping_a_node_twice_returns_the_same_object
    doc = root::HTML5::Document.new
    assert_same doc.internal_subset, doc.internal_subset
    assert_same doc, root::XML::Node.new("p", doc).document
  end

  def test_node_classes_follow_node_types
    doc = root::HTML5::Document.new
    assert_instance_of root::XML::Element, root::XML::Node.new("p", doc)
    assert_instance_of root::XML::DTD, doc.internal_subset
    assert_instance_of root::XML::Text, doc.create_text_node("t")
    assert_instance_of root::XML::Comment, doc.create_comment("c")
    assert_instance_of root::XML::CDATA, doc.create_cdata("c")
    assert_instance_of root::HTML5::DocumentFragment, doc.fragment
  end

  def test_names_and_types
    doc = root::HTML5::Document.new
    element = root::XML::Node.new("Div", doc)
    assert_equal ["Div", root::XML::Node::ELEMENT_NODE], [element.name, element.type]
    assert_equal ["text", root::XML::Node::TEXT_NODE], [doc.create_text_node("t").name, doc.create_text_node("t").type]
    assert_equal "#cdata-section", doc.create_cdata("c").name
    assert_equal "#document-fragment", doc.fragment.name
    assert_equal ["document", root::XML::Node::HTML_DOCUMENT_NODE], [doc.name, doc.type]
    assert_equal root::XML::Node::DOCUMENT_NODE, root::XML::Document.new.type
  end

  def test_html_documents_start_with_a_doctype
    dtd = root::HTML5::Document.new.internal_subset
    assert_equal "html", dtd.name
    assert_predicate dtd, :html_dtd?
    assert_nil root::XML::Document.new.internal_subset
  end

  def test_new_wrappers_are_decorated
    doc = root::HTML5::Document.new
    doc.decorators(root::XML::Node) << marker
    doc.decorators(root::XML::NodeSet) << marker

    assert_kind_of marker, root::XML::Node.new("p", doc)
    assert_kind_of marker, doc.create_text_node("t")
    assert_kind_of marker, doc.internal_subset
    assert_kind_of marker, doc.fragment
    assert_kind_of marker, root::XML::NodeSet.new(doc)
    assert_instance_of root::XML::NodeSet, root::XML::NodeSet.new(doc)
  end

  def test_fragments_are_decorated_before_initialize
    doc = root::HTML5::Document.new
    doc.decorators(root::XML::Node) << (decorator = marker)
    seen = nil
    fragment_class = Class.new(root::HTML5::DocumentFragment) do
      define_method(:initialize) do |*args, **kwargs|
        seen = singleton_class.include?(decorator)
        super(*args, **kwargs)
      end
    end
    fragment_class.new(doc)
    assert seen
  end

  def test_document_subclasses_extend_initialize
    decorator = marker
    node_class = root::XML::Node
    # What Loofah's DocumentDecorator does
    document_class = Class.new(root::HTML5::Document) do
      define_method(:initialize) do |*args, &block|
        super(*args, &block)
        decorators(node_class) << decorator
      end
    end
    doc = document_class.new
    assert_instance_of document_class, doc
    assert_kind_of decorator, root::XML::Node.new("p", doc)
  end

  def test_node_new_runs_initialize_with_the_block_then_yields
    calls = []
    element_class = Class.new(root::XML::Element) do
      define_method(:initialize) { |*args, &block| calls << [:initialize, args.size, !block.nil?] }
    end
    node = element_class.new("p", root::HTML5::Document.new, :extra) { |yielded| calls << yielded }
    assert_instance_of element_class, node
    assert_equal [[:initialize, 3, true], node], calls
  end

  def test_passing_a_node_as_the_document_is_deprecated
    doc = root::HTML5::Document.new
    parent = root::XML::Node.new("div", doc)
    deprecated, Warning[:deprecated] = Warning[:deprecated], true
    node = nil
    assert_output(nil, /deprecated/) { node = root::XML::Node.new("p", parent) }
    assert_same doc, node.document
  ensure
    Warning[:deprecated] = deprecated
  end

  def test_constructor_argument_errors
    doc = root::HTML5::Document.new
    error = assert_raises(ArgumentError) { root::XML::Node.new("p", 1) }
    assert_equal "document must be a #{root}::XML::Node", error.message
    assert_raises(ArgumentError) { root::XML::Node.new("p") }
    error = assert_raises(TypeError) { root::XML::Text.new(1, doc) }
    assert_equal "wrong argument type Integer (expected String)", error.message
  end

  def test_an_allocated_node_has_no_data
    node = root::XML::Node.allocate
    error = assert_raises(RuntimeError) { node.name }
    assert_match(/\AUninitialized #{root}::XML::Node struct/, error.message)
    assert_match(/\(no data\)>\z/, node.inspect)
  end

  def test_wrappers_survive_compaction
    skip "needs GC.verify_compaction_references" unless GC.respond_to?(:verify_compaction_references)

    doc = root::HTML5::Document.new
    dtd = doc.internal_subset
    nodes = Array.new(100) { |i| root::XML::Node.new("p#{i}", doc) }
    begin
      GC.verify_compaction_references(expand_heap: true, toward: :empty)
    rescue NotImplementedError
      skip "this platform can't compact"
    end
    assert_same dtd, doc.internal_subset
    nodes.each_with_index do |node, i|
      assert_same doc, node.document
      assert_equal "p#{i}", node.name
    end
  end

  private

  def marker
    @marker ||= Module.new
  end
end

class TestObjectModelNokogiri < Minitest::Test
  include ObjectModelContract

  def root = Nokogiri
end

class TestObjectModelLobzique < Minitest::Test
  include ObjectModelContract

  def root = Lobzique
end
