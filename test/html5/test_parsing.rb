# frozen_string_literal: true

require "test_helper"
require "nokogiri"
require "stringio"

# Parsing and serializing through Nokogiri::HTML5's entry points, adapted
# from test/fixtures/nokogiri/html5/test_api.rb, test_quirks_mode.rb and
# test_serialize.rb to walk children until CSS and XPath land. The tree and
# output themselves are compared on whole corpora by script/differential.
module ParsingContract
  def test_convenience_methods
    html = "<!DOCTYPE html><p>hi"
    expected = root::HTML5::Document.parse(html).to_html
    assert_equal expected, root::HTML5.parse(html).to_html
    assert_equal expected, root.HTML5(html).to_html
    assert_equal root::HTML5::DocumentFragment.parse("<div><p>hi</div>").to_html,
      root::HTML5.fragment("<div><p>hi</div>").to_html
  end

  def test_parsed_documents
    doc = root::HTML5("<!DOCTYPE html><p>hi")
    assert_instance_of root::HTML5::Document, doc
    assert_equal "UTF-8", doc.encoding
    assert_nil root::HTML5::Document.new.encoding
    assert_predicate doc, :html?
    assert_equal root::HTML5::QuirksMode::NO_QUIRKS, doc.quirks_mode
    assert_equal root::HTML5::QuirksMode::QUIRKS, root::HTML5("<p>").quirks_mode
    assert_nil root::HTML5::Document.new.quirks_mode
    assert_predicate doc.internal_subset, :html5_dtd?
  end

  def test_url_and_errors
    url = "http://example.com"
    assert_nil root::HTML5("<p>hi").url
    assert_equal url, root::HTML5("<p>hi", url).url
    assert_equal url, root::HTML5("<p>hi", url: url).url
    assert_empty root::HTML5("<p>hi").errors

    errors = root::HTML5("<p>hi", url, max_errors: 1).errors
    assert_equal 1, errors.length
    assert_instance_of root::XML::SyntaxError, errors[0]
    assert_equal [url, 1, 2], [errors[0].file, errors[0].line, errors[0].level]
    assert_operator root::HTML5("<p>\n</b></i>", max_errors: -1).errors.length, :>, 2
    assert_equal "#fragment", root::HTML5.fragment("</b>", max_errors: 1).errors[0].file
  end

  def test_parse_encoding
    utf8 = "<!DOCTYPE html><body><p>おはようございます"
    shift_jis = utf8.encode(Encoding::SHIFT_JIS)
    raw = shift_jis.dup.force_encoding(Encoding::ASCII_8BIT)

    assert_match(/おはようございます/, root::HTML5(utf8).to_s)
    assert_match(/おはようございます/, root::HTML5(shift_jis).to_s)
    refute_match(/おはようございます/, root::HTML5(raw).to_s)
    assert_match(/おはようございます/, root::HTML5(raw, nil, Encoding::SHIFT_JIS).to_s)
    assert_match(/おはようございます/, root::HTML5(raw, encoding: Encoding::SHIFT_JIS).to_s)
    assert_match(/おはようございます/, root::HTML5.fragment(raw, encoding: Encoding::SHIFT_JIS).to_s)
  end

  def test_serialization_encoding
    frag = root::HTML5.fragment("<span>아는 길도 물어가라</span>")
    html = frag.serialize(encoding: "US-ASCII")
    assert_equal "<span>&#xc544;&#xb294; &#xae38;&#xb3c4; &#xbb3c;&#xc5b4;&#xac00;&#xb77c;</span>", html
    assert_equal Encoding::US_ASCII, html.encoding
    assert_equal "<span>아는 길도 물어가라</span>", root::HTML5.fragment(html).serialize
    assert_equal Encoding::UTF_8, root::HTML5("<p>").to_html.encoding
    assert_equal "", root::HTML5.fragment("").to_html
  end

  def test_io_and_memory
    assert_equal "<html><head></head><body><p>x</p></body></html>",
      root::HTML5::Document.read_io(StringIO.new("<p>x")).to_html
    assert_equal "<html><head></head><body><p>x</p></body></html>",
      root::HTML5::Document.read_memory("<p>x").to_html
    assert_raises(ArgumentError) { root::HTML5::Document.read_io("<p>") }
    assert_raises(ArgumentError) { root::HTML5::Document.read_memory(StringIO.new) }
    assert_raises(ArgumentError) { root::HTML5::Document.parse(Object.new) }
  end

  def test_noscript
    html = "<!DOCTYPE html><body><noscript><p>x</p></noscript>"
    assert_match "<noscript><p>x</p></noscript>", root::HTML5(html).to_html
    assert_match "<noscript><p>x</p></noscript>",
      root::HTML5(html, parse_noscript_content_as_text: true).to_html
    noscript = body(root::HTML5(html, parse_noscript_content_as_text: true)).children[0]
    assert_predicate noscript.children[0], :text?
  end

  def test_limits
    assert_equal "Document tree depth limit exceeded",
      assert_raises(ArgumentError) { root::HTML5("<div>" * 399) }.message
    root::HTML5("<div>" * 398)
    root::HTML5("<div>" * 1000, max_tree_depth: -1)
    assert_raises(ArgumentError) { root::HTML5.fragment("<div>" * 401) }
    root::HTML5.fragment("<div>" * 400)

    many = (1..401).map { "a#{_1}" }.join(" ")
    assert_equal "Attributes per element limit exceeded",
      assert_raises(ArgumentError) { root::HTML5("<p #{many}>") }.message
    root::HTML5("<p #{many}>", max_attributes: 401)
    assert_raises(ArgumentError) { root::HTML5("<p a b>", max_attributes: 1) }
  end

  def test_fragment_contexts
    assert_equal "<tbody><tr><td>x</td></tr></tbody>",
      root::HTML5.fragment("<td>x</td>", context: "table").to_html
    path = root::HTML5.fragment("<path/>", context: "svg").children[0]
    assert_equal ["path", "svg"], [path.name, path.namespace.prefix]
    assert_equal "Invalid context namespace 'foo:bar'",
      assert_raises(ArgumentError) { root::HTML5.fragment("x", context: "foo:bar") }.message
  end

  def test_fragment_from_node_drops_nested_forms
    form = body(root::HTML5("<!DOCTYPE html><form><span></span></form>")).children[0]
    frag = form.children[0].fragment("<form>Nested forms should be ignored</form>")
    assert_instance_of root::HTML5::DocumentFragment, frag
    assert_equal 1, frag.children.length
    assert_predicate frag.children[0], :text?

    span = body(root::HTML5("<!DOCTYPE html><span></span>")).children[0]
    assert_equal "form", span.fragment("<form></form>").children[0].name
  end

  def test_fragment_in_annotation_xml
    ["MathML-Presentation", "text/html"].zip(["math", nil]).each do |encoding, prefix|
      math = body(root::HTML5("<!DOCTYPE html><math><annotation-xml encoding='#{encoding}' /></math>")).children[0]
      mi = math.children[0].fragment("<mi>x</mi>").children[0]
      assert_equal ["mi", prefix], [mi.name, mi.namespace&.prefix]
    end
  end

  def test_fragment_quirks_mode
    input = "<p><table>"
    no_quirks = "<p></p><table></table>"
    quirks = "<p><table></table></p>"

    assert_nil root::HTML5::DocumentFragment.new(root::HTML5::Document.new).quirks_mode
    div = body(root::HTML5("<div>")).children[0]
    assert_equal [root::HTML5::QuirksMode::QUIRKS, quirks],
      div.fragment(input).then { [_1.quirks_mode, _1.to_html] }
    assert_equal [root::HTML5::QuirksMode::NO_QUIRKS, no_quirks],
      root::HTML5::DocumentFragment.new(div.document, input, "body").then { [_1.quirks_mode, _1.to_html] }
    div = body(root::HTML5("<!DOCTYPE html><div>")).children[0]
    assert_equal no_quirks, div.fragment(input).to_html
    assert_equal no_quirks, root::HTML5("<div>").fragment(input).to_html
  end

  def test_foreign_content
    svg = body(root::HTML5("<svg xlink:href=x><![CDATA[a<b]]></svg>")).children[0]
    assert_equal ["svg", "http://www.w3.org/2000/svg"], [svg.namespace.prefix, svg.namespace.href]
    attr = svg.attribute_nodes[0]
    assert_equal ["href", "xlink", "x"], [attr.name, attr.namespace.prefix, attr.value]
    assert_predicate svg.children[0], :cdata?
    assert_equal "a<b", svg.children[0].content
    assert_equal '<svg xlink:href="x"><![CDATA[a<b]]></svg>', svg.to_html
    assert_nil body(svg.document).namespace
  end

  def test_template_contents_are_children
    template = root::HTML5("<template><p>x</p></template>").children[0].children[0].children[0]
    assert_equal ["template", "p"], [template.name, template.children[0].name]
  end

  def test_initial_linefeed
    html = "<!DOCTYPE html><div><pre>\n\nx</pre><textarea>\nx</textarea><listing>x</listing></div>"
    div = body(root::HTML5(html)).children[0]
    assert_equal "<pre>\nx</pre><textarea>x</textarea><listing>x</listing>", div.inner_html
    assert_equal "<pre>\n\nx</pre><textarea>x</textarea><listing>x</listing>",
      div.inner_html(preserve_newline: true)
    assert_equal "\n\nx", div.children[0].inner_html(preserve_newline: true)
  end

  def test_content
    doc = root::HTML5("<!DOCTYPE html><!--c--><p>a<b>b</b><!--d-->c</p>")
    assert_equal "abc", doc.content
    assert_equal "abc", body(doc).children[0].content
    assert_equal "c", doc.children[1].content
  end

  def test_subclasses_are_initialized_once_and_decorated
    decorator = Module.new
    node_class = root::XML::Node
    klass = Class.new(root::HTML5::Document) do
      attr_reader :initialized_count

      define_method(:initialize) do |*args|
        super(*args)
        @initialized_count = (@initialized_count || 0) + 1
        decorators(node_class) << decorator
      end
    end
    doc = klass.parse("<p>x")
    assert_instance_of klass, doc
    assert_equal 1, doc.initialized_count
    assert_kind_of decorator, body(doc).children[0]
    assert_kind_of decorator, body(doc).children[0].children[0]
  end

  private

  def body(document)
    document.children.find(&:element?).children.find { _1.name == "body" }
  end
end

class TestParsingNokogiri < Minitest::Test
  include ParsingContract

  def root = Nokogiri
end

class TestParsingLobzique < Minitest::Test
  include ParsingContract

  def root = Lobzique

  # A departure: html5ever's messages, not gumbo's (ARCHITECTURE.md)
  def test_error_messages_are_html5evers
    error = Lobzique::HTML5("<p>", max_errors: 1).errors[0]
    assert_equal "1:0: ERROR: Unexpected token", error.to_s
  end

  # A departure: one Namespace object per call, not per libxml2 xmlNs
  def test_namespaces_are_not_shared
    svg = Lobzique::HTML5.fragment("<svg></svg>").children[0]
    assert_equal svg.namespace.href, svg.namespace.href
    refute_same svg.namespace, svg.namespace
  end
end
