# frozen_string_literal: true

require_relative "facade_helper"
require "minitest/mock"
require "minitest/spec"
require "loofah"
require "loofah/helpers"

# Loofah's suites as they are, on the facade. Its helper.rb needs Minitest 5,
# so its test case is defined here.
MiniTest = Minitest unless defined?(MiniTest)

class Loofah::TestCase < Minitest::Spec
  class << self
    alias_method :context, :describe
  end

  LOOFAH_HTML_DOCUMENT_CLASSES = [Loofah::HTML4::Document, Loofah::HTML5::Document]
  LOOFAH_HTML_DOCUMENT_FRAGMENT_CLASSES = [Loofah::HTML4::DocumentFragment, Loofah::HTML5::DocumentFragment]
  LOOFAH_HTML_VERSIONS = [:html4, :html5]
end
$LOAD_PATH.unshift(File.join(FacadeSuite::FIXTURES, "loofah"))
$LOADED_FEATURES << File.join(FacadeSuite::FIXTURES, "loofah/helper.rb")

Dir[File.join(FacadeSuite::FIXTURES, "loofah/{html5,integration,unit}/*.rb")].sort.each { require _1 }

FacadeSuite.leave_out(
  /UnitTestApi::Loofah::(XML|xml methods)#|#scrub_xml_fragment#/ =>
    "XML parsing is out of scope (plan.md decision 4)",
  /Loofah::HTML4::Document::#(serialize|to_s)#test_0001_generate HTML/ =>
    "libxml2 adds the HTML 4.0 doctype; the HTML4 names parse as HTML5 (decision 3)",
  /_in_html4_svg_content\z/ =>
    "libxml2's HTML4 parser lowercases SVG names; the HTML5 parser keeps textPath (decision 3)"
)
