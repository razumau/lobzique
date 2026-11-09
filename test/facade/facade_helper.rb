# frozen_string_literal: true

# Tests of the nokogiri facade, run in its own bundle (test/facade/Gemfile,
# `rake test:facade`): `require "nokogiri"` loads lobzique, so real Nokogiri
# isn't there to compare with. Upstream suites from test/fixtures are loaded
# as they are; whatever Nokogiri::HTML5 does differently is checked in the
# main suite against real Nokogiri.

require "nokogiri"
require "minitest/autorun"

module FacadeSuite
  FIXTURES = File.expand_path("../fixtures", __dir__)

  def self.load(path)
    require File.join(FIXTURES, path)
  end

  # Upstream tests left out, as {/Class#test pattern/ => reason}.
  def self.leave_out(left_out)
    Minitest::Runnable.runnables.each do |klass|
      klass.public_instance_methods(true).grep(/^test_/).each do |test|
        next unless left_out.keys.any? { |pattern| pattern.match?("#{klass.name}##{test}") }

        klass.send(:undef_method, test)
      end
    end
  end
end
