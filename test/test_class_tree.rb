# frozen_string_literal: true

require "test_helper"
require "nokogiri"
require "yaml"

# Every class, module and constant lobzique defines mirrors Nokogiri's: the
# same ancestors (so `is_a?`, `case` and `super` behave the same) and the
# same constant values. The contract's classes, modules and constants must
# all exist.
class TestClassTree < Minitest::Test
  CONTRACT = File.expand_path("../inventory/contract.yml", __dir__)
  ROOT = /\A(?:Nokogiri|Lobzique)(?=::|\z)/
  # Ours, renamed after the engine it binds: Nokogiri's
  RENAMED = {"Lobzique::Html5ever" => "Nokogiri::Gumbo"}
  # Constants whose values differ by design
  OWN_VALUE = ["Lobzique::VERSION"]
  # Ours, with no counterpart: compiled CSS, where Nokogiri has XPath strings
  OWN = ["Lobzique::CSS::Selector"]

  def self.counterpart(path)
    RENAMED.reduce(path) { |p, (ours, theirs)| p.sub(/\A#{ours}\b/, theirs) }.sub(ROOT, "Nokogiri")
  end

  def self.each_constant(mod, seen = {mod => true}, &block)
    mod.constants(false).sort.each do |name|
      value = mod.const_get(name, false)
      yield "#{mod.name}::#{name}", value
      next unless value.is_a?(Module) && value.name&.match?(ROOT) && !seen[value]

      seen[value] = true
      each_constant(value, seen, &block)
    end
  end

  def self.test_name(path) = "test_#{path.gsub("::", "__")}"

  each_constant(Lobzique) do |path, value|
    next if OWN_VALUE.include?(path) || OWN.include?(path)

    define_method(test_name(path)) do
      theirs = Object.const_get(self.class.counterpart(path))
      if value.is_a?(Module)
        assert_equal normalize(theirs.ancestors), normalize(value.ancestors)
      else
        assert_equal theirs, value
      end
    end
  end

  YAML.safe_load_file(CONTRACT).fetch("entries").each do |entry|
    next unless entry["scope"] == "in" && %w[class module constant].include?(entry["kind"])

    ours = entry["api"].sub(ROOT, "Lobzique")
    define_method("#{test_name(ours)}__in_contract") do
      assert Object.const_defined?(ours), "#{ours} is in inventory/contract.yml but not defined"
    end
  end

  private

  def normalize(ancestors)
    ancestors.map { |mod| self.class.counterpart(mod.name.to_s) }
  end
end
