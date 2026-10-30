# frozen_string_literal: true

require "test_helper"
require "nokogiri"
require "stringio"

# Node#parse, adapted from the parse tests in
# test/fixtures/nokogiri/xml/test_node.rb, html4/test_node.rb,
# html4/test_document.rb and html5/test_api.rb onto HTML5 documents.
# Nokogiri parses in context with libxml2's HTML4 parser, so the shared
# tests use input both parsers read the same way.
module NodeParseContract
  HTML = "<div>x</div><table><tr><!--c--><td>c</td></tr></table><textarea>t</textarea><math></math>"

  def test_parse_returns_unlinked_nodes
    doc = root::HTML5(HTML)
    set = doc.at_css("div").parse("<p>a</p><b>c</b>")
    assert_instance_of root::XML::NodeSet, set
    assert_same doc, set.document
    assert_equal ["<p>a</p>", "<b>c</b>"], set.map(&:to_s)
    set.each do |node|
      assert_same doc, node.document
      assert_nil node.parent
      assert_nil node.next_sibling
    end
  end

  def test_parse_with_io_and_empty_string
    div = root::HTML5(HTML).at_css("div")
    assert_equal ["<i>x</i>"], div.parse(StringIO.new("<i>x</i>")).map(&:to_s)
    assert_empty div.parse("")
  end

  def test_parse_config_option
    options = nil
    root::HTML5(HTML).at_css("div").parse("<div></div>") { |config| options = config }
    assert_instance_of root::XML::ParseOptions, options
    assert_equal root::XML::ParseOptions::DEFAULT_HTML, options.to_i
  end

  def test_errors_are_added_to_the_document_and_recovered_from
    doc = root::HTML5(HTML)
    div = doc.at_css("div")
    assert_equal ["<b>x</b>"], div.parse("</div><b>x</b>").map(&:to_s)
    assert_operator doc.errors.length, :>, 0

    count = doc.errors.length
    div.parse("</a>" * 1000)
    assert_equal 100, doc.errors.length - count
  end

  def test_strict_raises_the_first_new_error
    div = root::HTML5(HTML).at_css("div")
    assert_raises(root::XML::SyntaxError) { div.parse("<x></y>", &:strict) }
    assert_equal 1, div.parse("<div>a</div>", root::XML::ParseOptions::STRICT).length
  end

  def test_the_context_is_the_nearest_element
    doc = root::HTML5(HTML)
    comment = doc.at_css("tr").child
    assert_predicate comment, :comment?
    assert_equal ["<td>x</td>"], comment.parse("<td>x</td>").map(&:to_s)
  end

  def test_the_document_is_the_context_of_nodes_outside_an_element
    doc = root::HTML5(HTML)
    assert_equal ["<p>a</p>"], doc.parse("<p>a</p>").map(&:to_s)
    assert_equal ["div"], root::XML::Text.new("div", doc).parse("<div></div>").map(&:name)
    assert_equal ["<i></i>"], root::HTML5.fragment("foo<b></b>").child.parse("<i></i>").map(&:to_s)
    assert_equal ["div"], root::XML::Text.new("div", root::HTML4::Document.new).parse("<div/>").map(&:name)
  end

  def test_parse_in_context_of_foreign_namespace
    nodes = root::HTML5("<html><body><math>").at_css("math").parse("mrow")
    assert_kind_of root::XML::NodeSet, nodes
    assert_equal 1, nodes.length
  end

  def test_empty_input_in_xml_documents
    doc = root::XML::Document.new
    doc.root = root::XML::Node.new("r", doc)
    options = nil
    set = doc.root.parse("") { |config| options = config }
    assert_empty set
    assert_equal root::XML::ParseOptions::DEFAULT_XML, options.to_i
    assert_empty root::XML::DocumentFragment.new(doc, "", doc.root).children
  end

  def test_node_ownership_after_dup
    dup = root::HTML5(HTML).dup
    set = dup.at_css("div").parse("<div>replaced</div>")
    assert_same dup, set.document
    assert_same dup, set.first.document
  end
end

class TestNodeParseNokogiri < Minitest::Test
  include NodeParseContract

  def root = Nokogiri
end

class TestNodeParseLobzique < Minitest::Test
  include NodeParseContract

  def root = Lobzique

  # Departures: the HTML5 fragment parser with the node as context
  # (ARCHITECTURE.md)
  def test_parsing_follows_the_html5_fragment_algorithm
    doc = Lobzique::HTML5(HTML)
    assert_equal ["<head></head>", "<body><br></body>"], doc.root.parse("<br>").map(&:to_s)
    assert_equal ["&lt;i&gt;x&lt;/i&gt;"], doc.at_css("textarea").child.parse("<i>x</i>").map(&:to_s)
    assert_equal ["<input>"], Lobzique::HTML5("<form><span>").at_css("span").parse("<form><input></form>").map(&:to_s)
  end

  def test_the_document_context_has_the_documents_quirks_mode
    input = "<p>a<table></table>"
    assert_equal ["<p>a<table></table></p>"], Lobzique::HTML5("<div></div>").parse(input).map(&:to_s)
    assert_equal ["<p>a</p>", "<table></table>"], Lobzique::HTML5("<!DOCTYPE html>").parse(input).map(&:to_s)
  end

  def test_parsed_nodes_have_no_previous_sibling
    set = Lobzique::HTML5(HTML).at_css("div").parse("<p>a</p><b>c</b>")
    assert_nil set[1].previous_sibling
  end

  def test_an_empty_result_stays_in_the_document
    doc = Lobzique::HTML5(HTML)
    set = doc.at_css("div").parse("</div>")
    assert_empty set
    assert_same doc, set.document
  end

  def test_xml_documents_are_not_supported
    doc = Lobzique::XML::Document.new
    doc.root = Lobzique::XML::Node.new("r", doc)
    assert_raises(NotImplementedError) { doc.root.parse("<a/>") }
    assert_raises(NotImplementedError) { Lobzique::XML::DocumentFragment.new(doc, "<a/>", doc.root) }
  end

  def test_limits
    div = Lobzique::HTML5(HTML).at_css("div")
    assert_raises(ArgumentError) { div.parse("<div>" * 401) }
    assert_raises(ArgumentError) { div.parse("<p #{(1..401).map { "a#{_1}" }.join(" ")}>") }
  end
end
