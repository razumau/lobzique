# frozen_string_literal: true

require "test_helper"
require "nokogiri"
require_relative "../differential/corpus"

# CSS.xpath_for as Nokogiri has it, run against both. The first tests port
# test/fixtures/nokogiri/css/test_css.rb and test_selector_cache.rb.
module XPathForContract
  def css = root::CSS

  def visitor(**) = css::XPathVisitor.new(**)

  def setup
    css::SelectorCache.clear_cache
  end

  def test_accepts_just_a_selector
    assert_equal ["//foo"], css.xpath_for("foo")
    assert_equal ["//h1", "//h2", "//h3"], css.xpath_for("h1, h2, h3")
  end

  def test_accepts_keyword_arguments
    assert_equal ["./foo"], css.xpath_for("foo", prefix: "./")
    assert_equal ["./foo"], css.xpath_for("foo", visitor: visitor(prefix: "./"))
    assert_equal ["//xmlns:foo"], css.xpath_for("foo", ns: {"xmlns" => "http://example.com"})
    assert_equal ["//foo"], css.xpath_for("foo", ns: {"x" => "http://example.com"})
  end

  def test_accepts_an_options_hash
    with_deprecation_warnings do
      assert_output(nil, /Passing options as an explicit hash is deprecated/) do
        assert_equal ["./foo"], css.xpath_for("foo", {prefix: "./"})
      end
      assert_output(nil, /Passing options as an explicit hash is deprecated/) do
        assert_equal ["./foo"], css.xpath_for("foo", {visitor: visitor(prefix: "./")})
      end
    end
  end

  def test_visitor_excludes_prefix_and_ns
    assert_raises(ArgumentError) { css.xpath_for("foo", prefix: "./", visitor: visitor) }
    assert_raises(ArgumentError) { css.xpath_for("foo", ns: {}, visitor: visitor) }
  end

  def test_errors
    assert_raises(css::SyntaxError) { css.xpath_for("'") }
    assert_raises(css::SyntaxError) { css.xpath_for("a[x=]") }
    assert_equal "empty CSS selector", assert_raises(css::SyntaxError) { css.xpath_for("") }.message
    [nil, 3, Object.new, ["foo", "bar"]].each do |selector|
      assert_raises(TypeError) { css.xpath_for(selector) }
    end
    assert_raises(css::SyntaxError) { css.xpath_for("div:-moz-drag-over") }
    assert_raises(css::SyntaxError) { css.xpath_for("div:-moz-drag-over()") }
  end

  def test_visitor_configuration
    assert_equal({builtins: :never, doctype: :xml, prefix: "//", namespaces: nil}, visitor.config)
    assert_raises(ArgumentError) { visitor(builtins: :sometimes) }
    assert_raises(ArgumentError) { visitor(doctype: :html) }
    assert_equal ["//a[contains(concat(' ',normalize-space(@class),' '),' b ')]"], css.xpath_for("a.b")
    assert_equal ["//a[nokogiri-builtin:css-class(@class,'b')]"],
      css.xpath_for("a.b", visitor: visitor(builtins: :always))
    assert_equal ["//*:a", "//svg:a", "//*"], css.xpath_for("a, svg|a, *", visitor: visitor(builtins: :optimal, doctype: :html5))
    assert_equal ["//*[local-name()='a']"], css.xpath_for("a", visitor: visitor(doctype: :html5))
    assert_equal ["//a"], css.xpath_for("a", visitor: visitor(builtins: :always, doctype: :html4))
  end

  def test_leading_combinators_start_at_the_context_node
    assert_equal ["./a", "./following-sibling::*[1]/self::a", "./following-sibling::a", "foo//a"],
      ["> a", "+ a", "~ a", "// a"].flat_map { css.xpath_for(_1, prefix: "foo") }
  end

  def test_string_literals
    assert_equal ["//a[@b='c']", %(//a[@b="it's"]), %(//a[@b=concat('it',"'",'s "x"')])],
      css.xpath_for(%q(a[b=c], a[b="it's"], a[b='it\'s "x"']))
    assert_equal ["//a[nokogiri:foo(.,'b c',@d,3,e)]"], css.xpath_for("a:foo('b c', @d, 3, e)")
  end

  def test_translations_are_cached_by_selector_and_configuration
    cached = css.xpath_for("a b")
    assert_same cached, css.xpath_for("a b")
    refute_same cached, css.xpath_for("a b", cache: false)
    assert_equal [".//a//b"], css.xpath_for("a b", prefix: ".//")
    assert_same cached, css.xpath_for("a b", visitor: visitor)
    css::SelectorCache.clear_cache
    refute_same cached, css.xpath_for("a b")
  end
end

class TestXPathForNokogiri < Minitest::Test
  include XPathForContract

  def root = Nokogiri
end

class TestXPathForLobzique < Minitest::Test
  include XPathForContract

  def root = Lobzique

  CONFIGURATIONS = [
    {},
    {prefix: ".//", namespaces: {"xmlns" => "http://www.w3.org/1999/xhtml"}},
    {builtins: :always, doctype: :html5},
    {builtins: :optimal, doctype: :html4, prefix: "self::"}
  ].freeze

  # Nokogiri's parser and visitor also fail with these, on selectors that
  # the core refuses with a CSS::SyntaxError
  NOKOGIRI_ERRORS = [Nokogiri::CSS::SyntaxError, Racc::ParseError, NoMethodError, ArgumentError].freeze

  def selectors
    cases = []
    Differential::Corpus.each_wpt_selector { cases << _1 }
    Differential::Corpus.each_nokogiri_selector { cases << _1 }
    cases.flat_map(&:selectors).uniq
  end

  def translate(mod, selector, configuration)
    mod::CSS.xpath_for(selector, visitor: mod::CSS::XPathVisitor.new(**configuration), cache: false)
  rescue Lobzique::CSS::SyntaxError, *NOKOGIRI_ERRORS
    :error
  end

  def test_fixture_selectors_translate_as_in_nokogiri
    selectors = self.selectors
    assert_operator selectors.size, :>, 500
    translated = 0
    CONFIGURATIONS.each do |configuration|
      selectors.each do |selector|
        expected = translate(Nokogiri, selector, configuration)
        assert_equal expected, translate(Lobzique, selector, configuration), "#{selector.inspect} with #{configuration}"
        translated += 1 unless expected == :error
      end
    end
    assert_operator translated, :>, 1500
  end

  # The strings mean what the compiled selector means
  def test_translations_find_what_css_finds
    {
      "wpt-selectors/ParentNode-querySelector-All-content.html" => :each_wpt_selector,
      "nokogiri/files/tlm.html" => :each_nokogiri_selector
    }.each do |file, corpus|
      doc = Lobzique::HTML5::Document.parse(File.binread(File.join(Differential::Corpus::FIXTURES, file)))
      visitor = Lobzique::CSS::XPathVisitor.new(builtins: :always, doctype: :html5)
      compared = 0
      Differential::Corpus.public_send(corpus) do |kase|
        selector = kase.selectors.first
        found = [
          -> { doc.css(selector) },
          -> { doc.xpath(Lobzique::CSS.xpath_for(selector, visitor: visitor).join("|")) }
        ].map do |search|
          search.call.to_a
        rescue Lobzique::CSS::SyntaxError, Lobzique::XML::XPath::SyntaxError => error
          error.class
        end
        assert_equal found[0], found[1], selector.inspect
        compared += 1 if found[0].is_a?(Array)
      end
      assert_operator compared, :>, 100, file
    end
  end

  def test_a_pasted_node_has_no_address
    assert_equal ["//a[position()=#<Nokogiri::CSS::Node>]"], Lobzique::CSS.xpath_for("a:eq(@b)")
  end

  def test_selectors_are_utf8
    assert_equal ["//é"], Lobzique::CSS.xpath_for("é")
    assert_equal Encoding::UTF_8, Lobzique::CSS.xpath_for("a").first.encoding
    assert_raises(ArgumentError) { Lobzique::CSS.xpath_for("\xff") }
  end
end
