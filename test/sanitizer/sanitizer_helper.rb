# frozen_string_literal: true

require "test_helper"
require "nokogiri"
# Before rails-html-sanitizer, which opens ActionView::Helpers::SanitizeHelper
# and so keeps it from being autoloaded; Rails loads them in this order.
require "active_support"
require "active_support/test_case"
require "action_view"
require "lobzique/sanitizer"

# Upstream suites from test/fixtures are loaded as they are, byte for byte,
# and their test modules and classes reused with lobzique's classes in place
# of Loofah's.
module UpstreamSuite
  FIXTURES = File.expand_path("../fixtures", __dir__)

  # rails-html-sanitizer's test_helper only prints Nokogiri's version, and
  # its suites already have what it requires.
  $LOADED_FEATURES << File.join(FIXTURES, "rails-html-sanitizer/test_helper.rb")

  def self.load(path)
    require File.join(FIXTURES, path)
  end

  # Under GC.stress these suites, on Nokogiri and on lobzique, would take
  # hours; they run Ruby over native methods the other suites stress.
  # The gc-stress build collects around every C API call whatever
  # GC.stress is, so there they don't run at all.
  module NoGCStress
    def gc_stress? = false

    module Skipped
      def runnable_methods = []
    end

    def self.included(test_class)
      test_class.extend(Skipped) if ENV["LOBZIQUE_GC_STRESS_BUILD"]
    end
  end

  # Upstream classes left over after the reuse, testing HTML4 or
  # rails-html-sanitizer's own Loofah parsing rather than a vendor.
  def self.skip(*classes)
    classes.each { |klass| Minitest::Runnable.runnables.delete(klass) }
  end
end
