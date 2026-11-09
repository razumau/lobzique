# frozen_string_literal: true

require_relative "facade_helper"
require "minitest/mock"

# rails-dom-testing's suites as they are, on the facade. Its default is
# :html4, which parses as HTML5 here (plan.md decision 3).
$LOAD_PATH.unshift(File.join(FacadeSuite::FIXTURES, "rails-dom-testing"))
Dir[File.join(FacadeSuite::FIXTURES, "rails-dom-testing/*_test.rb")].sort.each { require _1 }

FacadeSuite.leave_out(
  /AssertSelectTest#test_(assert_select_root_xml|feed_item_encoded)/ =>
    "XML parsing is out of scope (plan.md decision 4)",
  /AssertSelectTest#test_(equality_of_html|strip_textarea)\z/ =>
    "libxml2 keeps the newline after <pre> and <textarea>, which HTML5 drops (decision 3)",
  /DomAssertionsHtmlParserSelectionTest#test_(default_html4|default_html5,_html4_specified|raise_NotImplementedError)/ =>
    "they tell the parsers apart by libxml2's tree for :html4 (decision 3)",
  /AssertSelectTest#test_(multiple_substitution_values|nested_css_select|substitution_values)\z/ =>
    "fail on Nokogiri too: Regexp sources go unescaped into :match (rails/rails-dom-testing#136)"
)
