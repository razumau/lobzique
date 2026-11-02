# frozen_string_literal: true

require_relative "sanitizer_helper"
require "loofah/helpers"

# Loofah's scrubbing suites, loaded as they are. Each test class gets a
# lobzique copy in which Loofah::HTML5::Document.parse and
# DocumentFragment.parse return lobzique's classes, so the same tests run on
# Loofah and Nokogiri (the upstream class) and on lobzique (the copy).
# html5/test_scrub.rb isn't loaded: it tests Loofah::HTML5::Scrub's string
# functions, which the vendor uses as they are.
module LoofahSuite
  # Loofah's helper.rb, which needs Minitest 5.
  class Loofah::TestCase < Minitest::Spec
    include UpstreamSuite::NoGCStress

    class << self
      alias_method :context, :describe
    end

    LOOFAH_HTML_DOCUMENT_CLASSES = [Loofah::HTML4::Document, Loofah::HTML5::Document]
    LOOFAH_HTML_DOCUMENT_FRAGMENT_CLASSES = [Loofah::HTML4::DocumentFragment, Loofah::HTML5::DocumentFragment]
    LOOFAH_HTML_VERSIONS = [:html4, :html5]
  end
  $LOAD_PATH.unshift(File.join(UpstreamSuite::FIXTURES, "loofah"))
  $LOADED_FEATURES << File.join(UpstreamSuite::FIXTURES, "loofah/helper.rb")

  ROUTES = {
    Loofah::HTML5::Document => Lobzique::Sanitizer::Document,
    Loofah::HTML5::DocumentFragment => Lobzique::Sanitizer::DocumentFragment
  }.freeze

  module Parse
    def parse(...)
      Thread.current[:lobzique_loofah_suite] ? ROUTES.fetch(self).parse(...) : super
    end
  end
  ROUTES.each_key { |klass| klass.singleton_class.prepend(Parse) }

  module OnLobzique
    def before_setup
      Thread.current[:lobzique_loofah_suite] = true
      super
    end

    def after_teardown
      super
    ensure
      Thread.current[:lobzique_loofah_suite] = nil
    end
  end

  # Tests left out, by "class#test" name, with the reason.
  LEFT_OUT = {
    # Upstream and copy: these fail on Loofah too.
    upstream: {
      /scrubbing shortcuts/ => "Loofah's shortcuts, tested with Minitest 5's MiniTest::Mock",
      /double_breakpoint#test_0002_/ => "the fixture is ahead of Loofah 2.25.2, which has the bug it tests"
    },
    # The copy only: Loofah's document API rather than scrubbing (sanitizer.md).
    lobzique: {
      /#text/ => "TextBehavior (text, to_text) isn't ported"
    }
  }.freeze

  def self.leave_out(klass, side)
    klass.methods_matching(/^test_/).each do |test|
      next unless LEFT_OUT.fetch(side).keys.any? { |pattern| pattern.match?("#{klass.name}##{test}") }

      klass.send(:undef_method, test) if klass.method_defined?(test)
    end
  end

  def self.load(path)
    before = Minitest::Runnable.runnables.dup
    UpstreamSuite.load(path)
    (Minitest::Runnable.runnables - before).to_h do |upstream|
      leave_out(upstream, :upstream)
      next [upstream, nil] if upstream.name.include?("HTML4")

      copy = Class.new(upstream) { include OnLobzique }
      copy.instance_variable_set(:@name, "#{upstream.name} on lobzique")
      leave_out(copy, :lobzique)
      [upstream, copy]
    end
  end

  # html5/test_sanitizer.rb's testdata loop, whose tests delete keys from
  # the case they share with every run, so they can run only once.
  def self.redefine_testdata(copy)
    copy.methods_matching(/testdata sanitizer/).each { |test| copy.send(:undef_method, test) }
    dat = File.join(UpstreamSuite::FIXTURES, "loofah/assets/testdata_sanitizer_tests1.dat")
    JSON.parse(File.read(dat), allow_comments: true).each do |test|
      copy.define_method(:"test_testdata sanitizer #{test["name"]}") do
        outputs = test.except("name", "input").sort.map(&:last)
        check_sanitization(test["input"], *outputs)
      end
    end
  end
end

LoofahSuite.load("loofah/html5/test_sanitizer").each_value do |copy|
  LoofahSuite.redefine_testdata(copy) if copy
end
LoofahSuite.load("loofah/unit/test_scrubber")
LoofahSuite.load("loofah/integration/test_scrubbers")
