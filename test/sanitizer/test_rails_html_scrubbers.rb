# frozen_string_literal: true

require_relative "sanitizer_helper"

# rails-html-sanitizer's scrubbers_test.rb, whose classes parse with
# Loofah's HTML4 fragment. Each runs twice instead: on Loofah's HTML5
# fragment, and on lobzique's.
UpstreamSuite.load("rails-html-sanitizer/scrubbers_test")
ScrubberTest.include(UpstreamSuite::NoGCStress)

module ScrubbersOn
  def self.fragment_class(klass)
    Module.new do
      define_method(:scrub_fragment) { |html| klass.parse(html).scrub!(@scrubber).to_s }
      define_method(:to_node) { |text| klass.parse(text).children.first }
      protected :scrub_fragment, :to_node
    end
  end

  NOKOGIRI = fragment_class(Loofah::HTML5::DocumentFragment)
  LOBZIQUE = fragment_class(Lobzique::Sanitizer::DocumentFragment)
end

[
  PermitScrubberTest,
  TargetScrubberTest,
  TextOnlyScrubberTest,
  ReturningStopFromScrubNodeTest,
  PermitScrubberMinimalOperationsTest
].each do |upstream|
  {"Nokogiri" => ScrubbersOn::NOKOGIRI, "Lobzique" => ScrubbersOn::LOBZIQUE}.each do |root, parsing|
    Object.const_set(:"#{upstream.name}#{root}", Class.new(upstream) { include parsing })
  end
  UpstreamSuite.skip(upstream)
end
