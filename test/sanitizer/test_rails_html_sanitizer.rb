# frozen_string_literal: true

require_relative "sanitizer_helper"

# rails-html-sanitizer's sanitizer_test.rb with Lobzique::Sanitizer as the
# module under test. Its HTML5…Test classes run as they are, so the same
# assertions are checked on Loofah and Nokogiri.
UpstreamSuite.load("rails-html-sanitizer/sanitizer_test")

UpstreamSuite.skip(
  SanitizerTests::BaseSanitizerTest,
  SanitizerTests::HTML4FullSanitizerTest,
  SanitizerTests::HTML4LinkSanitizerTest,
  SanitizerTests::HTML4SafeListSanitizerTest
)
[SanitizerTests::HTML5FullSanitizerTest, SanitizerTests::HTML5LinkSanitizerTest, SanitizerTests::HTML5SafeListSanitizerTest]
  .each { |klass| klass.include(UpstreamSuite::NoGCStress) }

# Upstream accepts several results where libxml2 versions or JRuby differ;
# a vendor has to give exactly what Rails::HTML5 gives.
module SameAsRailsHTML5
  # Left to upstream's own assertions.
  DIFFERENT = {
    # html5ever parses <select> as customizable select (ARCHITECTURE.md,
    # departures); gumbo, Nokogiri's parser, doesn't. The vendor gives the
    # result upstream accepts from libxml2 (EscapeRawTextInSelect).
    "test_combination_of_select_and_style_with_css_payload" => :select,
    "test_combination_of_select_and_style_with_script_payload" => :select,
    "test_raw_text_in_select_is_inert_in_either_browser" => :select
  }.freeze

  protected

  def full_sanitize(input, options = {})
    assert_same_as_html5(:FullSanitizer, input, options) { super }
  end

  def link_sanitize(input, options = {})
    assert_same_as_html5(:LinkSanitizer, input, options) { super }
  end

  def safe_list_sanitize(input, options = {})
    assert_same_as_html5(:SafeListSanitizer, input, options) { super }
  end

  private

  def assert_same_as_html5(sanitizer, input, options)
    result = yield
    return result if DIFFERENT.key?(name)

    expected = with_allowed_lists_of(module_under_test::SafeListSanitizer) do
      Rails::HTML5.const_get(sanitizer).new.sanitize(input, options)
    end
    message = "#{sanitizer} differs from Rails::HTML5 on #{input.inspect}"
    expected.nil? ? assert_nil(result, message) : assert_equal(expected, result, message)
    result
  end

  # scope_allowed_tags and scope_allowed_attributes set the lists on the
  # vendor's class.
  def with_allowed_lists_of(klass)
    html5 = Rails::HTML5::SafeListSanitizer
    saved = [html5.allowed_tags, html5.allowed_attributes]
    html5.allowed_tags = klass.allowed_tags
    html5.allowed_attributes = klass.allowed_attributes
    yield
  ensure
    html5.allowed_tags, html5.allowed_attributes = saved
  end
end

class LobziqueFullSanitizerTest < Minitest::Test
  @module_under_test = Lobzique::Sanitizer
  include UpstreamSuite::NoGCStress
  include SanitizerTests::FullSanitizerTest
  include SameAsRailsHTML5
end

class LobziqueLinkSanitizerTest < Minitest::Test
  @module_under_test = Lobzique::Sanitizer
  include UpstreamSuite::NoGCStress
  include SanitizerTests::LinkSanitizerTest
  include SameAsRailsHTML5
end

class LobziqueSafeListSanitizerTest < Minitest::Test
  @module_under_test = Lobzique::Sanitizer
  include UpstreamSuite::NoGCStress
  include SanitizerTests::SafeListSanitizerTest
  include SameAsRailsHTML5

  # Nokogiri::HTML5 parses <select> as browsers without customizable select
  # do, lobzique as browsers with it.
  def test_raw_text_in_select_is_inert_in_either_browser
    %w[iframe noembed noframes plaintext style xmp].each do |raw_text|
      ["<script>alert(1)</script>", "<input><img src=x onerror=alert(1)>"].each do |payload|
        input = "<select><div><#{raw_text}>#{payload}</#{raw_text}></div></select>"
        result = safe_list_sanitize(input, tags: ["select", raw_text, "input", "img"])
        [Nokogiri::HTML5, Lobzique::HTML5].each do |browser|
          tree = browser.fragment(result)
          assert_nil tree.at_xpath(".//script | .//@onerror"), "#{browser} parsing #{result.inspect}"
        end
      end
    end
  end

  # Loofah's API, which skips the sanitizers' serialize.
  def test_raw_text_in_select_is_escaped_by_scrubbing_a_fragment
    scrubber = Rails::HTML::PermitScrubber.new
    scrubber.tags = %w[select style img]
    fragment = Lobzique::Sanitizer::DocumentFragment.parse("<select><style><img src=x onerror=alert(1)></style></select>")
    assert_equal "<select><style>&lt;img src=x onerror=alert(1)&gt;</style></select>", fragment.scrub!(scrubber).to_s
    assert_equal "<select><style>&lt;img src=x onerror=alert(1)&gt;</style></select>", fragment.scrub!(scrubber).to_s
  end

  # The cases of HTML5SafeListSanitizerTest that name Rails::HTML5 directly.
  # Nokogiri::HTML5 stays the browser that parses the result.

  def test_should_not_be_vulnerable_to_nokogiri_foreign_style_serialization_bug
    input = "<svg><style>&lt;img src onerror=alert(1)>"
    result = Lobzique::Sanitizer::SafeListSanitizer.new.sanitize(input, tags: ["svg", "style"])
    browser = Nokogiri::HTML5::Document.parse(result)
    assert_nil browser.at_xpath("//img/@onerror")
  end

  def test_should_not_be_vulnerable_to_ns_confusion_2519936
    input = "<math><style><style class='</style><script>alert(1)</script>'>"
    result = Lobzique::Sanitizer::SafeListSanitizer.new.sanitize(input, tags: ["style"])
    browser = Nokogiri::HTML5::Document.parse(result)
    assert_nil browser.at_xpath("//script")
  end

  def test_should_not_be_vulnerable_to_ns_confusion_2519941
    input = "<math><mtext><table><mglyph><style><img src=: onerror=alert(1)>"
    result = Lobzique::Sanitizer::SafeListSanitizer.new.sanitize(input, tags: %w[math style])
    browser = Nokogiri::HTML5::Document.parse(result)
    assert_nil browser.at_xpath("//img/@onerror")
  end

  def test_should_not_be_vulnerable_to_mglyph_namespace_confusion
    input = "<math><mtext><table><mglyph><style><img src=: onerror=alert(1)>"
    result = nil
    assert_output(nil, /WARNING/) do
      result = safe_list_sanitize(input, tags: %w[math mtext table mglyph style])
    end
    browser = Nokogiri::HTML5::Document.parse(result)
    assert_nil browser.at_xpath("//img/@onerror")
  end

  def test_should_not_be_vulnerable_to_malignmark_namespace_confusion
    input = "<math><mtext><table><malignmark><style><img src=: onerror=alert(1)>"
    result = nil
    assert_output(nil, /WARNING/) do
      result = safe_list_sanitize(input, tags: %w[math mtext table malignmark style])
    end
    browser = Nokogiri::HTML5::Document.parse(result)
    assert_nil browser.at_xpath("//img/@onerror")
  end

  def test_should_not_be_vulnerable_to_noscript_attacks
    input = '<noscript><p id="</noscript><script>alert(1)</script>"></noscript>'
    result = nil
    assert_output(nil, /WARNING/) do
      result = Lobzique::Sanitizer::SafeListSanitizer.new.sanitize(input, tags: %w[p div noscript], attributes: %w[id class style])
    end
    browser = Nokogiri::HTML5::Document.parse(result, parse_noscript_content_as_text: true)
    assert_nil browser.at_xpath("//script")
  end
end
