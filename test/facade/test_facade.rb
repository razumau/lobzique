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

  def test_lobzique_first_is_refused
    lib = File.expand_path("../../lib", __dir__)
    facade = File.expand_path("../../facade/lib", __dir__)
    _, error, status = Open3.capture3(RbConfig.ruby, "-I#{lib}", "-I#{facade}", "-e", 'require "lobzique"; require "nokogiri"')
    refute_predicate status, :success?
    assert_match(/lobzique was loaded before the nokogiri facade/, error)
  end
end
