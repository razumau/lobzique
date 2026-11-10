# frozen_string_literal: true

require_relative "facade_helper"
require "minitest/mock"
# Before rails-html-sanitizer, as in test/sanitizer/sanitizer_helper.rb
require "active_support"
require "active_support/test_case"
require "action_view"
require "rails-html-sanitizer"

# rails-html-sanitizer's suites as they are: Rails::HTML4 and Rails::HTML5,
# through Loofah, on the facade.
$LOADED_FEATURES << File.join(FacadeSuite::FIXTURES, "rails-html-sanitizer/test_helper.rb")
%w[sanitizer_test scrubbers_test rails_api_test].each { FacadeSuite.load("rails-html-sanitizer/#{_1}") }
