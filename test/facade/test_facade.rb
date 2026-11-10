# frozen_string_literal: true

require_relative "facade_helper"
require "open3"
require "rbconfig"

# What the facade adds to lobzique: the root's name, and the version
# answers that Loofah and others read (plan.md decision 1).
class TestFacade < Minitest::Test
  def test_lobzique_is_nokogiri
    assert_same Nokogiri, Lobzique
    assert_equal "Nokogiri::HTML5::Document", Lobzique::HTML5::Document.name
    assert_equal "Nokogiri::XML::Element", Nokogiri::HTML5("<p>").at_css("p").class.name
  end

  def test_messages_name_nokogiri
    error = assert_raises(ArgumentError) { Nokogiri::XML::Node.new("p", Object.new) }
    assert_equal "document must be a Nokogiri::XML::Node", error.message
    assert_raises(Nokogiri::CSS::SyntaxError) { Nokogiri::HTML5("<p>").css("p[") }
  end

  def test_versions
    assert_equal "1.19.7", Nokogiri::VERSION
    assert_equal Nokogiri::VERSION, Lobzique::VERSION
    assert_equal Nokogiri::VERSION, Nokogiri::VERSION_INFO.dig("nokogiri", "version")
    assert_equal Gem.loaded_specs.fetch("lobzique").version.to_s, Nokogiri::VERSION_INFO.dig("lobzique", "version")
    assert Gem::Requirement.new(Gem.loaded_specs.fetch("loofah").dependencies.find { _1.name == "nokogiri" }.requirement)
      .satisfied_by?(Gem::Version.new(Nokogiri::VERSION))
  end

  # Loofah defines its HTML5 classes, and rails-html-sanitizer
  # Rails::HTML5::Sanitizer, only with uses_gumbo?, and escapes attributes
  # as lobzique's vendor does only with libxml2?.
  def test_introspection
    assert_predicate Nokogiri, :uses_gumbo?
    assert_predicate Nokogiri, :uses_libxml?
    assert Nokogiri.uses_libxml?("~> 2.13")
    assert Nokogiri::VersionInfo.instance.libxml2?
    refute Nokogiri.jruby?
    assert_equal [], Nokogiri.libxml2_patches
    assert_empty Nokogiri::VERSION_INFO["warnings"]
    assert_match(/\A# Nokogiri \(1\.19\.7\)\n/, Nokogiri::VersionInfo.instance.to_markdown)
  end

  def test_loofah_and_rails_html5
    require "rails-html-sanitizer"
    assert_predicate Loofah, :html5_support?
    assert_predicate Rails::HTML::Sanitizer, :html5_support?
    assert_operator Loofah::HTML5::Document, :<, Nokogiri::HTML5::Document
    assert_equal %(<a href="x%20y">x</a>y), Rails::HTML5::SafeListSanitizer.new.sanitize(%(<a href="x y" onclick="1">x</a><script>y</script>))
  end

  def test_parse_guesses_html
    html = "<!DOCTYPE html><html><body><p>x</p></body></html>"
    assert_kind_of Nokogiri::HTML4::Document, Nokogiri.parse(html)
    assert_equal "x", Nokogiri(html).at_css("p").text
    assert_equal "p", Nokogiri.make("<p>x</p>").name
    Nokogiri.parse(html) { |doc| assert_kind_of Nokogiri::HTML4::Document, doc }
  end

  # What lobzique leaves out raises with a reason, not NoMethodError.
  def test_unsupported_methods
    [
      ["Nokogiri.XML", -> { Nokogiri::XML("<a/>") }],
      ["Nokogiri.XML", -> { Nokogiri.parse("<a/>") }],
      ["Nokogiri::XML.parse", -> { Nokogiri::XML.parse("<a/>") }],
      ["Nokogiri::XML.Reader", -> { Nokogiri::XML::Reader("<a/>") }],
      ["Nokogiri::XML::Document.parse", -> { Nokogiri::XML::Document.parse("<a/>", nil, nil, 0) }],
      ["Nokogiri::XML::DocumentFragment.parse", -> { Nokogiri::XML::DocumentFragment.parse("<a/>") }],
      ["Nokogiri.XSLT", -> { Nokogiri::XSLT("<a/>") }],
      ["Nokogiri::CSS.xpath_for", -> { Nokogiri::CSS.xpath_for("p") }],
      ["Nokogiri.Slop", -> { Nokogiri::Slop("<a/>") }],
      ["Nokogiri() with a block", -> { Nokogiri { p_ } }]
    ].each do |name, call|
      error = assert_raises(NotImplementedError, name, &call)
      assert_match(/\A#{Regexp.escape(name)} isn't supported: the nokogiri facade is lobzique/, error.message)
      assert_match(/lobzique's seams/, error.message)
    end
  end

  def test_unsupported_methods_name_subclasses
    subclass = Class.new(Nokogiri::XML::Document)
    error = assert_raises(NotImplementedError) { subclass.parse("<a/>") }
    assert_match(/\A#{Regexp.escape(subclass.inspect)}\.parse isn't supported/, error.message)
  end

  def test_html_parsing_is_not_stubbed
    assert_kind_of Nokogiri::HTML4::Document, Nokogiri::HTML4::Document.parse("<p>")
    assert_kind_of Nokogiri::HTML5::Document, Nokogiri::HTML5::Document.read_memory("<p>")
    assert_kind_of Nokogiri::HTML4::DocumentFragment, Nokogiri::HTML4::DocumentFragment.parse("<p>")
    assert_kind_of Nokogiri::HTML5::DocumentFragment, Nokogiri::HTML5::DocumentFragment.parse("<p>")
  end

  def test_unsupported_constants
    %w[
      Nokogiri::XML::Builder Nokogiri::HTML::Builder Nokogiri::HTML5::Builder
      Nokogiri::XML::SAX Nokogiri::HTML4::SAX Nokogiri::XML::Reader Nokogiri::XSLT
      Nokogiri::CSS::Parser Nokogiri::HTML::NamedCharacters
    ].each do |name|
      error = assert_raises(NameError, name) { Object.const_get(name) }
      assert_match(/\A#{Regexp.escape(name.sub("::HTML::", "::HTML4::"))} isn't supported: the nokogiri facade is lobzique/, error.message)
    end
    assert_nil defined?(Nokogiri::XML::Builder)
    assert_raises(NameError) { Class.new(Nokogiri::XML::SAX::Document) }
    error = assert_raises(NameError) { Nokogiri::XML::NoSuchThing }
    assert_match(/uninitialized constant Nokogiri::XML::NoSuchThing/, error.message)
  end

  def test_lobzique_first_is_refused
    lib = File.expand_path("../../lib", __dir__)
    facade = File.expand_path("../../facade/lib", __dir__)
    _, error, status = Open3.capture3(RbConfig.ruby, "-I#{lib}", "-I#{facade}", "-e", 'require "lobzique"; require "nokogiri"')
    refute_predicate status, :success?
    assert_match(/lobzique was loaded before the nokogiri facade/, error)
  end
end
