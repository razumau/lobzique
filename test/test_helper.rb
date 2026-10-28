# frozen_string_literal: true

$LOAD_PATH.unshift File.expand_path("../lib", __dir__)
require "lobzique"

require "minitest/autorun"

if ENV["LOBZIQUE_GC_STRESS"]
  # Collect garbage on every allocation while a test runs, so a Ruby object
  # that Rust holds without marking gets freed while it is still in use.
  module GCStress
    def before_setup
      super
      # Nokogiri's C code isn't GC.stress-safe (XPath handler results lose their node set)
      GC.stress = true unless defined?(root) && root.equal?(Nokogiri)
    end

    def after_teardown
      GC.stress = false
      super
    end
  end

  Minitest::Test.prepend(GCStress)
end
