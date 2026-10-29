# frozen_string_literal: true

require "test_helper"
require "nokogiri"

# XPath strings as Nokogiri evaluates them, run against both, within the
# subset lobzique evaluates (plan.md decision 2).
module XPathContract
  PAGE = <<~HTML
    <!DOCTYPE html>
    <html><head><meta http-equiv="Set-Cookie" content="a=b"><meta charset="utf-8"></head>
    <body>
      <form id="f"><input name="q"></form>
      <p id="p1">one<br>two<br>three</p>
      <!-- note -->
      <p id="p2" class="x">see <a href="#top">top</a> <img src="i.png" onerror="alert(1)"></p>
      <ul><li id="l1">a</li><li id="l2">b</li></ul>
      <svg><circle r="1"/></svg>
    </body></html>
  HTML

  def page = root::HTML5::Document.parse(PAGE)

  def paths(nodes) = nodes.map(&:path)

  # The XPath HTML gems in the inventory evaluate at runtime
  # (inventory/summary.md)
  def test_expressions_from_the_inventory
    doc = page
    body = doc.at_css("body")
    assert_equal ["/html/body"], paths(doc.xpath("/html/body"))
    assert_equal ["/html/body"], paths(doc.root.xpath("./body"))
    assert_equal ["/html/body/form"], paths(body.xpath("./form"))
    assert_equal ["/html/body/p[1]"], paths(doc.xpath("//p[br[following-sibling::br]]"))
    assert_equal ["/html/body/ul/li[1]/text()", "/html/body/ul/li[2]/text()"], paths(doc.at_css("ul").xpath(".//text()"))
    assert_equal ["/html/body/comment()"], paths(doc.xpath("//comment()"))
    assert_equal ["/html/body/ul/li[1]", "/html/body/ul/li[2]"], paths(doc.at_css("ul").xpath("li"))
    assert_equal ["/html/body/p[2]/text()[1]"], paths(doc.at_css("a").xpath("preceding::text()[1]"))
    assert_equal ["/html"], paths(doc.xpath("html"))
    assert_equal ["/html/head/meta[1]", "/html/head/meta[2]"], paths(doc.xpath("html/head/meta"))
    assert_equal ["/html/head/meta[1]"], paths(doc.xpath('//head/meta[@http-equiv="Set-Cookie"]'))
    assert_equal ["/html/body/p[2]/img/@onerror"], paths(doc.xpath("//img/@onerror"))
  end

  def test_axes
    p2 = page.at_css("#p2")
    {
      "ancestor::*" => ["/html", "/html/body"],
      "ancestor-or-self::p" => ["/html/body/p[2]"],
      "child::*" => ["/html/body/p[2]/a", "/html/body/p[2]/img"],
      "descendant::text()" => ["/html/body/p[2]/text()[1]", "/html/body/p[2]/a/text()", "/html/body/p[2]/text()[2]"],
      "descendant-or-self::a" => ["/html/body/p[2]/a"],
      "following::*" => ["/html/body/ul", "/html/body/ul/li[1]", "/html/body/ul/li[2]", "/html/body/svg:svg", "/html/body/svg:svg/svg:circle"],
      "following-sibling::*[1]" => ["/html/body/ul"],
      "parent::body" => ["/html/body"],
      "preceding::*[1]" => ["/html/body/p[1]/br[2]"],
      "preceding-sibling::*" => ["/html/body/form", "/html/body/p[1]"],
      "self::node()" => ["/html/body/p[2]"],
      "attribute::*" => ["/html/body/p[2]/@id", "/html/body/p[2]/@class"],
      "@class/following::*[1]" => ["/html/body/ul"],
      "@class/.." => ["/html/body/p[2]"]
    }.each do |xpath, found|
      assert_equal found, paths(p2.xpath(xpath)), xpath
    end
  end

  def test_predicates
    doc = page
    {
      "//p[2]" => ["/html/body/p[2]"],
      "//p[@class]" => ["/html/body/p[2]"],
      "//p[@class='x']" => ["/html/body/p[2]"],
      "//p[@class=x]" => [],
      "//li[.='b']" => ["/html/body/ul/li[2]"],
      "//p[a/@href = '#top']" => ["/html/body/p[2]"],
      "//circle[@r = 1]" => [],
      "//*:circle[@r = 1]" => ["/html/body/svg:svg/svg:circle"],
      "//li[1][@id='l2']" => [],
      "//li[@id='l2'][1]" => ["/html/body/ul/li[2]"],
      "//li[-1]" => [],
      "//li[1.5]" => [],
      "//li[2.0]" => ["/html/body/ul/li[2]"],
      "//p[/html/head]" => ["/html/body/p[1]", "/html/body/p[2]"]
    }.each do |xpath, found|
      assert_equal found, paths(doc.xpath(xpath)), xpath
    end
  end

  def test_abbreviations_and_blanks
    doc = page
    assert_equal [doc], doc.xpath("/").to_a
    assert_equal [doc], doc.xpath(".").to_a
    assert_empty doc.xpath("..")
    assert_equal ["/html/body/ul/li[2]"], paths(doc.xpath(" // ul / li [ 2 ] "))
    assert_equal ["/html/body/p[1]/@id", "/html/body/p[2]/@id"], paths(doc.xpath("//p/ @ id"))
    assert_equal ["/html/body/ul"], paths(doc.xpath("//li/.."))
    assert_equal 2, doc.xpath("//p/text ( )[1]").size
    assert_equal ["/html/body/ul"], paths(doc.xpath("/ / html/body/ul"))
    assert_equal ["/html/body/ul"], paths(doc.xpath("/ //ul"))
    # The doctype is a child of the document, but passes no node test
    assert_equal ["/html"], paths(doc.xpath("/node()"))
  end

  def test_names_and_namespaces
    doc = page
    assert_empty doc.xpath("//svg")
    assert_equal ["/html/body/svg:svg"], paths(doc.xpath("//svg:svg"))
    assert_equal ["/html/body/svg:svg/svg:circle"], paths(doc.xpath("//s:circle", "s" => "http://www.w3.org/2000/svg"))
    assert_equal ["/html/body/svg:svg/svg:circle"], paths(doc.xpath("//*:circle"))
    assert_empty doc.xpath("//P")
    assert_empty doc.xpath("//@*:id")
    error = assert_raises(root::XML::XPath::SyntaxError) { doc.xpath("//x:p") }
    assert_equal 1219, error.code
  end

  def test_results_are_the_document_nodes
    doc = page
    attribute = doc.at_xpath("//img/@onerror")
    assert_kind_of root::XML::Attr, attribute
    assert_equal "alert(1)", attribute.value
    assert_same doc.at_css("img"), attribute.parent
    assert_same doc.at_css("#p1"), doc.at_xpath("//p[1]")
    assert_kind_of root::XML::Comment, doc.at_xpath("//comment()")
  end

  def test_searching_from_fragments_and_node_sets
    doc = page
    assert_equal ["/html/body/p[1]/br[1]", "/html/body/p[1]/br[2]"], paths(doc.css("p").xpath("./br"))
    assert_equal ["/html/body/p[2]/a", "/html/body/ul"], paths(doc.search("//a", ".//ul"))
    assert_equal ["/html/body/p[1]", "/html/body/p[2]/a"], paths(doc.xpath("//p[1]", "//a"))
    fragment = root::HTML5::DocumentFragment.parse("<p>a</p><p><b>b</b></p>")
    assert_equal ["p", "p"], fragment.xpath("./p").map(&:name)
    assert_equal ["b"], fragment.search(".//b").map(&:name)
    assert_equal "a", fragment.at_xpath(".//text()").content
  end

  # libxml2's `node()` doesn't pass a fragment, and it rewrites
  # `descendant-or-self::node()/child::x` as `descendant::x` only when `x`
  # has no predicates. So `.//p[@id]` doesn't look at the fragment's
  # children.
  def test_fragments_are_not_nodes
    fragment = root::HTML5::DocumentFragment.parse("<p id=a><b id=b></b></p>")
    assert_equal [fragment], fragment.xpath(".").to_a
    assert_empty fragment.xpath("self::node()")
    assert_equal ["p", "b"], fragment.xpath(".//*").map(&:name)
    assert_equal ["b"], fragment.xpath(".//*[@id]").map(&:name)
    assert_equal ["b"], fragment.xpath("descendant-or-self::node()/*[1]").map(&:name)
    assert_empty fragment.at_css("p").xpath("..")
    assert_equal ["p"], fragment.at_css("b").xpath("ancestor::node()").map(&:name)
  end

  def test_syntax_errors
    doc = page
    {
      "..faulty_xpath" => [1207, "Invalid expression"],
      "" => [1207, "Invalid expression"],
      "//p/" => [1207, "Invalid expression"],
      "//p[@id='a'" => [1206, "Invalid predicate"],
      "//p[@id='a" => [1202, "Unfinished literal"],
      "//p[$]" => [1204, "Expected $ for variable reference"],
      "//p[@id='\u0001" => [1221, "Char out of XML range"],
      "//\uFFFE" => [1221, "Char out of XML range"],
      "//text(" => [1208, "Missing closing curly brace"]
    }.each do |xpath, (code, message)|
      error = assert_raises(root::XML::XPath::SyntaxError, xpath) { doc.xpath(xpath) }
      assert_equal [12, code, 2, xpath], [error.domain, error.code, error.level, error.str1], xpath
      assert_equal "ERROR: #{message}: #{xpath}", error.message
    end
  end

  def test_expressions_are_strings_without_nul
    context = root::XML::XPathContext.new(page)
    assert_raises(TypeError) { context.evaluate(1) }
    assert_raises(ArgumentError) { context.evaluate("//p\0") }
  end
end

class TestXPathNokogiri < Minitest::Test
  include XPathContract

  def root = Nokogiri
end

class TestXPathLobzique < Minitest::Test
  include XPathContract

  def root = Lobzique

  # What Nokogiri evaluates and lobzique leaves out
  def test_unsupported_xpath_names_the_construct
    doc = page
    {
      "string(((/html/head | /html | /head | /)/title)[1])" => "function string()",
      "//p | //a" => "unions",
      "(//p)[1]" => "parentheses",
      "//p[last()]" => "function last()",
      "//p[@id != 'p1']" => "the != operator",
      "//p[@id and @class]" => "the and operator",
      "//address[@domestic=$value]" => "variable $value",
      "//svg:*" => "the node test svg:*",
      "1 = 1" => "expressions that aren't location paths"
    }.each do |xpath, construct|
      error = assert_raises(Lobzique::XML::XPath::UnsupportedError, xpath) { doc.xpath(xpath) }
      assert_kind_of Lobzique::XML::XPath::SyntaxError, error
      assert_equal [12, 1207, 2, xpath], [error.domain, error.code, error.level, error.str1]
      assert_equal "ERROR: Unsupported XPath: #{construct}: #{xpath}", error.message
    end
  end

  def test_predicates_nest_32_deep
    doc = page
    assert_empty doc.xpath("//" + "p[" * 32 + "x" + "]" * 32)
    assert_raises(Lobzique::XML::XPath::UnsupportedError) { doc.xpath("//" + "p[" * 33 + "x" + "]" * 33) }
  end
end
