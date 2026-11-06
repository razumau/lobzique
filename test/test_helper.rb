# frozen_string_literal: true

$LOAD_PATH.unshift File.expand_path("../lib", __dir__)
require "lobzique"

require "minitest/autorun"

if ENV["LOBZIQUE_GC_STRESS"]
  # Collect garbage on every allocation while a test runs, so a Ruby object
  # that Rust holds without marking gets freed while it is still in use.
  module GCStress
    attr_writer :warm_up

    # Unstressed first: what loads at first use (Action Text's autoloads,
    # I18n's backend) takes minutes under GC.stress. Not on the gc-stress
    # build, which collects around every C API call anyway, so a warm-up
    # costs about half as much as the stressed run
    WARM_UP = !ENV["LOBZIQUE_GC_STRESS_BUILD"]

    def run
      self.class.new(name).tap { _1.warm_up = true }.run if WARM_UP && stressed?
      super
    end

    def before_setup
      super
      GC.stress = true if stressed?
    end

    def stressed? = gc_stress? && !@warm_up

    # Nokogiri's C code isn't GC.stress-safe (XPath handler results lose their node set)
    def gc_stress?
      !(defined?(root) && root.equal?(Nokogiri))
    end

    def after_teardown
      GC.stress = false
      super
    end
  end

  Minitest::Test.prepend(GCStress)
end
