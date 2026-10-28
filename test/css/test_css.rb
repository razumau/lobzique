# frozen_string_literal: true

require "test_helper"
require "nokogiri"

# CSS search as Nokogiri does it, run against both. The first tests port
# test/fixtures/nokogiri/css/test_css_integration.rb, for the HTML document
# classes (XML parsing is out of scope); the rest cover what the contract's
# callers rely on.
module CSSContract
  INTEGRATION = <<~HTML
    <html><body>
      <table>
        #{(1..14).map { |i| "<tr><td>row#{i} </td></tr>" }.join("\n    ")}
      </table>
      <div>
        <b>bold1 </b>
        <i>italic1 </i>
        <b class="a">bold2 </b>
        <em class="a">emphasis1 </em>
        <i>italic2 </i>
        <p>para1 </p>
        <b class="a">bold3 </b>
      </div>
      <div>
        <i class="b">italic3 </i>
        <em>emphasis2 </em>
        <i class="b">italic4 </i>
        <em>emphasis3 </em>
        <i class="c">italic5 </i>
        <span><i class="b">italic6 </i></span>
        <i>italic7 </i>
      </div>
      <div>
        <p>para2 </p>
        <p>para3 </p>
      </div>
      <div>
        <p>para4 </p>
      </div>

      <div>
        <h2></h2>
        <h1 class='c'>header1 </h1>
        <h2></h2>
      </div>
      <div>
        <h1 class='c'>header2 </h1>
        <h1 class='c'>header3 </h1>
      </div>
      <div>
        <h1 class='c'>header4</h1>
      </div>

      <p class='empty'></p>
      <p class='not-empty'><b></b></p>
    </body></html>
  HTML

  NESTED = <<~HTML
    <html><body>
      <div class='unnested direct'>
        <b>bold</b>
        <p>para</p>
      </div>

      <div class='unnested indirect'>
        <b>bold</b>
        <i>...</i>
        <i>...</i>
        <p>para</p>
      </div>

      <div class='nested-parent'>
        <div class='nested-child direct'>
          <b>bold</b>
          <p>para</p>
        </div>

        <div class='nested-child indirect'>
          <b>bold</b>
          <i>...</i>
          <i>...</i>
          <p>para</p>
        </div>
      </div>

      <div class="has-bold">
        <b>bold</b>
      </div>

      <div class="has-para">
        <p>para</p>
      </div>
  HTML

  def doctypes = [root::HTML4::Document, root::HTML5::Document]

  def assert_rows(numbers, result, word = "row")
    assert_equal numbers.map { |j| "#{word}#{j}" }.join(" "), result.inner_text.strip
  end

  def test_positional_pseudo_classes
    doctypes.each do |doctype|
      doc = doctype.parse(INTEGRATION)
      assert_rows [2, 4, 6, 8, 10, 12, 14], doc.css("table//tr:nth(even)")
      assert_rows [1, 3, 5, 7, 9, 11, 13], doc.css("table//tr:nth(odd)")
      assert_rows (1..14).to_a, doc.css("table//tr:nth(n)")
      assert_rows [3, 7, 11], doc.css("table//tr:nth(4n+3)")
      assert_rows [4, 7, 10, 13], doc.css("table//tr:nth(3n+4)")
      assert_rows [1, 2, 3], doc.css("table//tr:nth(-n+3)")
      assert_rows [3, 7, 11], doc.css("table//tr:nth(4n-1)")
      assert_rows (3..14).to_a, doc.css("table//tr:nth(n+3)")
      assert_rows [1], doc.css("table//tr:first")
      assert_rows [1], doc.css("table//tr:first()")
      assert_rows [14], doc.css("table//tr:last")
      assert_rows [14], doc.css("table//tr:last()")
    end
  end

  def test_child_pseudo_classes
    doctypes.each do |doctype|
      doc = doctype.parse(INTEGRATION)
      assert_rows [1], doc.css("div/b:first-child"), "bold"
      assert_rows [2, 4], doc.css("div/h1.c:first-child"), "header"
      assert_rows [3], doc.css("div/b:last-child"), "bold"
      assert_rows [3, 4], doc.css("div/h1.c:last-child"), "header"
      assert_rows [2], doc.css("div/b:nth-child(3)"), "bold"
      assert_rows [5], doc.css("table//tr:nth-child(5)")
      assert_rows [1, 3], doc.css("div/h1.c:nth-child(2)"), "header"
      assert_rows [3, 4], doc.css("div/i.b:nth-child(2n+1)"), "italic"
      assert_rows [3, 4], doc.css("div/i.b:nth-child(2n + 1)"), "italic"
      assert_rows [4], doc.css("div/p:only-child"), "para"
      assert_rows [4], doc.css("div/h1.c:only-child"), "header"
    end
  end

  def test_of_type_counts_what_passes_the_conditions_before_it
    doctypes.each do |doctype|
      doc = doctype.parse(INTEGRATION)
      assert_rows [1], doc.css("table//tr:first-of-type")
      assert_rows [2], doc.css("div/b.a:first-of-type"), "bold"
      assert_rows [3], doc.css("div/i.b:first-of-type"), "italic"
      assert_rows [2, 6, 7], doc.css("div i:last-of-type"), "italic"
      assert_rows [4], doc.css("div/i.b:last-of-type"), "italic"
      assert_rows [2], doc.css("div/.a:nth-of-type(1)"), "bold"
      assert_rows [2, 4, 7], doc.css("div i:nth-of-type(2n)"), "italic"
      assert_rows [1], doc.css("div .a:nth-of-type(2n)"), "emphasis"
      assert_rows [2, 3], doc.css("div .a:nth-of-type(2n+1)"), "bold"
      assert_rows [12], doc.css("table//tr:nth-last-of-type(3)")
      assert_rows [1, 5], doc.css("div i:nth-last-of-type(2)"), "italic"
      assert_rows [3], doc.css("div/i.b:nth-last-of-type(2)"), "italic"
      assert_rows [1, 4], doc.css("div/p:only-of-type"), "para"
      assert_rows [5], doc.css("div/i.c:only-of-type"), "italic"
    end
  end

  def test_empty_parent_and_contains
    doctypes.each do |doctype|
      doc = doctype.parse(INTEGRATION)
      assert_equal ["empty"], doc.css("p:empty").map { |p| p["class"] }
      assert_equal 5, doc.css("p:parent").size
      assert_equal 14, doc.css("td:contains('row')").length
      assert_equal 6, doc.css("td:contains('row1')").length
      assert_equal 4, doc.css("div :contains('header')").length
      assert_equal 9, doc.css(":contains('header')").length
    end
  end

  def test_has
    doctypes.each do |doctype|
      doc = doctype.parse(NESTED)
      classes = ->(selector) { doc.css(selector).map { |node| node["class"] || node.parent["class"] } }
      assert_equal ["unnested direct", "unnested indirect", "nested-parent", "nested-child direct",
        "nested-child indirect", "has-bold"], classes["div:has(b)"]
      assert_equal ["nested-parent"], classes["body *:has(div > b)"]
      assert_equal ["unnested direct", "nested-parent", "nested-child direct"], classes["div:has(b + p)"]
      assert_equal ["unnested direct", "nested-child direct"], classes["b:has(+ p)"]
      assert_equal 4, doc.css("b:has(~ p)").size
    end
  end

  def test_siblings_and_attribute_and_text_results
    doctypes.each do |doctype|
      doc = doctype.parse("<div><p id=1>p1 </p><p id=2 class='q\tr'>p2 </p><p id=3>p3 </p>t<p id=4>p4 </p></div>")
      assert_equal "p2 p3 p4 ", doc.css("#1 ~ p").inner_text
      assert_equal "p2 ", doc.css("#1 + p").inner_text
      assert_empty doc.css("#4 + p")
      assert_equal ["q\tr"], doc.css("p > @class").map(&:value)
      assert_equal ["q\tr"], doc.css("p[class~='r']").map { |p| p["class"] }
      assert_equal ["t"], doc.css("div > text()").map(&:content)
    end
  end

  # Action Text (inventory/contract.yml): attachments, galleries and matches?
  def test_action_text_queries
    html = <<~HTML
      <div><action-text-attachment presentation="gallery" sgid="1"></action-text-attachment><action-text-attachment presentation="gallery" sgid="2"></action-text-attachment></div>
      <div><action-text-attachment sgid="3"></action-text-attachment></div>
      <p><a href="/x">x</a></p><figure data-trix-attachment="{}"></figure>
    HTML
    fragment = root::HTML5::DocumentFragment.parse(html)
    galleries = fragment.css("div:has(action-text-attachment[presentation=gallery] + action-text-attachment[presentation=gallery])")
    assert_equal 1, galleries.size
    attachments = fragment.css("action-text-attachment")
    assert_equal [true, true, false], attachments.map { |node| node.matches?("action-text-attachment[presentation=gallery]") }
    assert_equal ["/x"], fragment.css("a[href]").map { |a| a["href"] }
    assert_equal 1, fragment.css("[data-trix-attachment]").size
    assert_equal 2, fragment.css("p, a").size
  end

  # rails-dom-testing's substitution handler and its other selectors
  def test_rails_dom_testing_queries
    handler = Class.new do
      def match(matches, attribute, matcher)
        matches.find_all { |node| node.attribute(attribute)&.value&.match?(Regexp.new(matcher)) }
      end
    end.new
    doc = root::HTML5::Document.parse(<<~HTML)
      <div id="1"><code><a class="edit-icon">e</a><a>f</a></code></div>
      <div id="wups" class="source hidden"></div><div class="source"></div>
      <input placeholder='placeholder with "quotes"' type="checkbox" value="1">
    HTML
    assert_equal ["1"], doc.css("div:match('id', \"\\d+\")", handler).map { |d| d["id"] }
    assert_equal ["wups"], doc.css(":match('id', \"wups\")", handler).map { |d| d["id"] }
    assert_equal ["edit-icon"], doc.css("code a:first").map { |a| a["class"] }
    assert_equal ["source"], doc.css("div.source:not(.hidden)").map { |d| d["class"] }
    assert_equal 1, doc.css('input[placeholder="placeholder with \"quotes\""]').size
    assert_equal 1, doc.css("input[type=checkbox][value='1']").size
    assert_equal ["html"], doc.css(":root").map(&:name)
    # an element whose parent isn't an element
    assert_empty doc.at_css("body").css("div:root")
    assert_equal 1, root::HTML5::DocumentFragment.parse("<div><div></div></div>").css("div:root").size
  end

  # premailer and mechanize: the [@attr] dialect, search and at
  def test_xpath_attribute_dialect
    doc = root::HTML5::Document.parse(<<~HTML)
      <head><link rel="stylesheet" href="a.css"><link rel="stylesheet" data-premailer="ignore"><style>p{}</style></head>
      <body><p id="x" style="color: red"><a href="#top">t</a><a href="/y">y</a></p><link rel="canonical" href="/c"></body>
    HTML
    assert_equal 2, doc.search("link[@rel='stylesheet']:not([@data-premailer='ignore']), style:not([@data-premailer='ignore'])").size
    assert_equal ["p"], doc.search("*[@style]").map(&:name)
    assert_equal ["#top"], doc.search("a[@href^='#']").map { |a| a["href"] }
    assert_equal "/c", doc.at('link[@rel="canonical"][@href]')["href"]
    assert_equal "p", doc.at_css("#x").name
    assert_equal 2, (doc / "a").size
    assert_equal "a", (doc % "a").name
  end

  def test_results_are_the_document_objects_in_document_order
    doc = root::HTML5::Document.parse("<p><b>1</b></p><i>2</i><p><b>3</b></p>")
    doc.decorators(root::XML::NodeSet) << (marker = Module.new)
    found = doc.css("i, p, b")
    assert_kind_of marker, found
    assert_equal %w[p b i p b], found.map(&:name)
    assert_same doc.at_css("i"), doc.root.children.last.children[1]
    # rules are searched in turn, each match once
    assert_equal %w[i p p b b], doc.css("i", "p", "b", "p").map(&:name)
  end

  def test_searches_from_a_node_stay_inside_it
    doc = root::HTML5::Document.parse("<div id=outer><p><b></b></p></div>")
    paragraph = doc.at_css("p")
    assert_empty paragraph.css("div b")
    assert_empty paragraph.css("p")
    assert_equal ["b"], paragraph.css("b").map(&:name)
    assert_equal ["b"], (paragraph > "b").map(&:name)
    assert_equal ["b"], (paragraph > "b, a + text()").map(&:name)
    assert_raises(root::CSS::SyntaxError) { paragraph > "b, :-x" }
    assert_equal ["div"], doc.at_css("b").ancestors("div").map(&:name)
  end

  def test_node_sets_and_fragments_also_match_their_own_nodes
    doc = root::HTML5::Document.parse("<p class=a><b>1</b></p><p><b>2</b></p>")
    paragraphs = doc.css("p")
    assert_equal 2, paragraphs.css("p").size
    assert_equal 2, paragraphs.css("b").size
    assert_equal 1, paragraphs.filter(".a").size
    assert_same paragraphs[1], paragraphs.at(1)
    fragment = root::HTML5::DocumentFragment.parse("<p>a</p><div><p>b</p><p>c</p></div><svg><rect/></svg>")
    assert_equal 3, fragment.css("p").size
    assert_equal 2, fragment.css("p:first").size
    assert_equal 3, fragment.search("p").size
    # a fragment's children define no prefixes, unlike a document's root
    assert_raises(root::XML::XPath::SyntaxError) { fragment.css("svg|rect") }
    assert_empty root::HTML5::DocumentFragment.parse("").css("p")
  end

  def test_names_match_any_namespace_unless_prefixed
    doc = root::HTML5::Document.parse("<svg><rect/></svg><math><mi/></math>")
    assert_equal ["rect"], doc.css("rect").map(&:name)
    assert_equal ["rect"], doc.css("svg|rect").map(&:name)
    assert_equal ["rect"], doc.css("x|rect", {"x" => "http://www.w3.org/2000/svg"}).map(&:name)
    assert_empty doc.css("|rect")
    assert_empty doc.css("RECT")
  end

  def test_xml_documents_match_names_exactly
    doc = root::XML::Document.new
    doc.root = root::XML::Node.new("Envelope", doc)
    doc.root.add_child(root::XML::Node.new("Body", doc))
    assert_equal ["Body"], doc.css("Envelope Body").map(&:name)
    assert_empty doc.css("envelope")
  end

  def test_custom_pseudo_classes_are_called_with_each_candidate
    doc = root::HTML5::Document.parse("<p id=a></p><p id=b></p><p id=c></p>")
    calls = []
    handler = Class.new do
      define_method(:seen) do |set, *args|
        calls << [set.class, set.map { |n| n["id"] }, args]
        set
      end

      def second(set) = 2
      def yes(set) = true
      def nothing(set) = nil
      def weird(set) = Object.new
    end.new
    assert_equal 3, doc.css("p:seen(1, 'x', @id)", handler).size
    assert_equal [[root::XML::NodeSet, ["a"], [1.0, "x"]], [root::XML::NodeSet, ["b"], [1.0, "x"]],
      [root::XML::NodeSet, ["c"], [1.0, "x"]]], calls.map { |klass, ids, args| [klass, ids, args.first(2)] }
    assert_equal ["a"], calls.first.last.last.map(&:value)
    # a number alone in a predicate is a position, as in XPath
    assert_equal ["b"], doc.css("p:second", handler).map { |p| p["id"] }
    assert_equal 3, doc.css("p:yes", handler).size
    error = assert_raises(root::XML::XPath::SyntaxError) { doc.css("p:nothing", handler) }
    assert_match(/\AERROR: Stack usage error: /, error.message)
    error = assert_raises(RuntimeError) { doc.css("p:weird", handler) }
    assert_equal "Invalid return type", error.message
  end

  def test_basic_object_handlers
    doc = root::HTML5::Document.parse("<input disabled><input>")
    handler = Class.new(BasicObject) do
      include ::Kernel

      def disabled(list) = list.find_all { |node| node.has_attribute?("disabled") }
    end.new
    assert_equal 1, doc.css("input:disabled", handler).size
  end

  def test_syntax_errors
    doc = root::HTML5::Document.parse("<p></p>")
    ["p:", "p::x", "[", "div,", "p ++ q", "#"].each do |selector|
      assert_raises(root::CSS::SyntaxError, selector) { doc.css(selector) }
    end
    assert_equal "empty CSS selector", assert_raises(root::CSS::SyntaxError) { doc.css("") }.message
  end

  def test_xpath_errors_wait_for_evaluation
    doc = root::HTML5::Document.parse("<p></p>")
    {"a + text()" => [1207, "Invalid expression"], "p:checked" => [1209, "Unregistered function"],
     "x|y" => [1219, "Undefined namespace prefix"]}.each do |selector, (code, message)|
      error = assert_raises(root::XML::XPath::SyntaxError, selector) { doc.css(selector) }
      assert_equal [12, code, 2], [error.domain, error.code, error.level]
      assert_match(/\AERROR: #{message}: /, error.message)
    end
    # nothing evaluated, nothing raised
    assert_empty root::HTML5::DocumentFragment.parse("").css("a + text()")
  end
end

class TestCSSNokogiri < Minitest::Test
  include CSSContract

  def root = Nokogiri
end

class TestCSSLobzique < Minitest::Test
  include CSSContract

  def root = Lobzique

  def test_xpath_strings_wait_for_the_xpath_subset
    doc = Lobzique::HTML5::Document.parse("<p></p>")
    assert_raises(NotImplementedError) { doc.xpath(".//p") }
    assert_raises(NotImplementedError) { doc.search("./p") }
  end

  # HTML4 parsing comes with plan.md, phase 2 step 7
  def doctypes = [Lobzique::HTML5::Document]

  def test_contexts_survive_compaction
    skip "needs GC.verify_compaction_references" unless GC.respond_to?(:verify_compaction_references)

    doc = Lobzique::HTML5::Document.parse("<p class=a></p>")
    contexts = Array.new(50) { Lobzique::XML::XPathContext.new(doc.at_css("p")) }
    selector = Lobzique::CSS::Selector.new(".a", prefixes: ["self::"], doctype: :html5)
    doc = nil # rubocop:disable Lint/UselessAssignment
    begin
      GC.verify_compaction_references(expand_heap: true, toward: :empty)
    rescue NotImplementedError
      skip "this platform can't compact"
    end
    contexts.each { |context| assert_equal ["p"], context.evaluate(selector).map(&:name) }
  end

  def test_parentheses_nest_32_deep
    doc = Lobzique::HTML5::Document.parse("<p></p>")
    assert_equal 1, doc.css(":not(" * 32 + "p" + ")" * 32).size
    assert_raises(Lobzique::CSS::SyntaxError) { doc.css(":not(" * 33 + "p" + ")" * 33) }
  end
end
