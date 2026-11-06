# frozen_string_literal: true

require "test_helper"
require "nokogiri"
require "active_support"
require "active_support/test_case"
require "lobzique/rails_dom_testing"

# rails-dom-testing's own suites from test/fixtures, loaded byte for byte.
# Its classes run as they are, with the seam loaded and :html4 by default,
# so Nokogiri::HTML4 keeps working beside it; subclasses run them again
# with :html5 on Nokogiri::HTML5 and on lobzique.
module RailsDomTestingHelper
  FIXTURES = File.expand_path("../fixtures/rails-dom-testing", __dir__)

  class << self
    # Whose HTML5 classes rails-dom-testing picks: lobzique's, as the seam
    # sets them, or Nokogiri's, as without it
    attr_accessor :root
  end
  self.root = Lobzique

  module ParserSelection
    def html_document(...)
      parser = super
      parser.equal?(Lobzique::HTML5::Document) ? RailsDomTestingHelper.root::HTML5::Document : parser
    end

    def html_document_fragment(...)
      parser = super
      parser.equal?(Lobzique::HTML5::DocumentFragment) ? RailsDomTestingHelper.root::HTML5::DocumentFragment : parser
    end
  end
  Rails::Dom::Testing.singleton_class.prepend(ParserSelection)

  module HTML5
    # Upstream expects libxml2's trees: the HTML5 parser drops a newline
    # right after <pre> and <textarea>, on Nokogiri::HTML5 as on lobzique.
    HTML4_ONLY = %w[test_strip_textarea test_equality_of_html].freeze

    def before_setup
      super
      skip "expects HTML4's tree" if HTML4_ONLY.include?(name)
      @saved = [Rails::Dom::Testing.default_html_version, RailsDomTestingHelper.root]
      Rails::Dom::Testing.default_html_version = :html5
      RailsDomTestingHelper.root = root
    end

    def after_teardown
      Rails::Dom::Testing.default_html_version, RailsDomTestingHelper.root = @saved if @saved
      super
    end
  end
end

require "minitest/mock"

# Nokogiri 1.19.5 reads CSS escapes in string arguments, and rails-dom-testing
# 2.3.0 substitutes a Regexp's source unescaped, so `\d+` becomes `\r+`
# (rails/rails-dom-testing#136). lobzique reads them as Nokogiri does.
module RegexpSubstitution
  BROKEN = %w[test_substitution_values test_multiple_substitution_values test_nested_css_select].freeze

  def before_setup
    super
    skip "rails/rails-dom-testing#136" if BROKEN.include?(name)
  end
end

# Upstream's helper defines DomTestingHelpers; its suites' own
# `require "test_helper"` finds ours, which is already loaded.
require File.join(RailsDomTestingHelper::FIXTURES, "test_helper")
%w[dom_assertions_test selector_assertions_test parser_selection_test].each do
  require File.join(RailsDomTestingHelper::FIXTURES, _1)
end

AssertSelectTest.prepend(RegexpSubstitution)

# It asserts Nokogiri::HTML5's classes for :html5
Minitest::Runnable.runnables.delete(DomTestingParserSelectionTest)

class NokogiriParserSelectionTest < DomTestingParserSelectionTest
  include RailsDomTestingHelper::HTML5

  def root = Nokogiri
end

class LobziqueParserSelectionTest < Minitest::Test
  include DomTestingHelpers

  def test_html5_is_lobziques
    [:html4, :html5].each do |default|
      with_default_html_version(default) do
        assert_equal Lobzique::HTML5::Document, Rails::Dom::Testing.html_document(html_version: :html5)
        assert_equal Lobzique::HTML5::DocumentFragment, Rails::Dom::Testing.html_document_fragment(html_version: :html5)
        assert_equal Nokogiri::HTML4::Document, Rails::Dom::Testing.html_document(html_version: :html4)
        assert_equal Nokogiri::HTML4::DocumentFragment, Rails::Dom::Testing.html_document_fragment(html_version: :html4)
      end
    end
    with_default_html_version(:html5) do
      assert_equal Lobzique::HTML5::Document, Rails::Dom::Testing.html_document
      assert_equal Lobzique::HTML5::DocumentFragment, Rails::Dom::Testing.html_document_fragment
    end
  end

  def test_errors_are_rails_dom_testings
    assert_raises(ArgumentError) { Rails::Dom::Testing.html_document(html_version: :html9) }
    Rails::Dom::Testing.stub(:html5_support?, false) do
      assert_raises(NotImplementedError) { Rails::Dom::Testing.html_document_fragment(html_version: :html5) }
    end
  end
end

# With :html5 as the default, as in apps on load_defaults 7.1 and later
[DomAssertionsTest, AssertSelectTest].each do |suite|
  # Upstream's run on Nokogiri::HTML4; GC.stress is for lobzique's below
  suite.define_method(:gc_stress?) { false }
  {Nokogiri => "NokogiriHTML5", Lobzique => "Lobzique"}.each do |root, prefix|
    Object.const_set(:"#{prefix}#{suite.name}", Class.new(suite) {
      include RailsDomTestingHelper::HTML5

      define_method(:root) { root }
      define_method(:gc_stress?) { root.equal?(Lobzique) }
    })
  end
end

# Upstream's, with lobzique's error class: in seam mode it isn't Nokogiri's
class LobziqueAssertSelectTest
  def test_assert_select_with_invalid_selector
    render_html '<a href="http://example.com">hello</a>'
    assert_raises Lobzique::CSS::SyntaxError do
      assert_select("[href=http://example.com]")
    end
  end

  def test_css_select_with_invalid_selector
    render_html '<a href="http://example.com">hello</a>'
    assert_raises Lobzique::CSS::SyntaxError do
      css_select("[href=http://example.com]")
    end
  end
end
