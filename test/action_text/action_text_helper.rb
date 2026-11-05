# frozen_string_literal: true

# Action Text without a Rails app: what its engine and a host app would load
# before Content and its attachments work. Rendering through a controller
# (Content#to_s, attachment partials) still needs an app; Rails' own suite
# covers that (script/rails).

require "active_support/all"
require "active_model"
require "active_record"
require "global_id"
require "action_view"
require "rails-html-sanitizer"
require "action_text"
require File.join(Gem.loaded_specs.fetch("actiontext").full_gem_path, "app/helpers/action_text/content_helper")
require "lobzique/sanitizer"
require "lobzique/action_text"

SignedGlobalID.verifier ||= ActiveSupport::MessageVerifier.new("lobzique")
# Railties defines it; TrixAttachment logs malformed JSON to it
Rails.singleton_class.attr_accessor(:logger) unless Rails.respond_to?(:logger)
Rails.logger ||= ActiveSupport::Logger.new(IO::NULL)

module ActionTextHelper
  # The classes Action Text parses with and the sanitizer its content helper
  # uses, per root module; requiring lobzique/action_text set the first two.
  def self.use(root)
    vendor = root.equal?(Lobzique) ? Lobzique::Sanitizer : Rails::HTML5::Sanitizer
    ActionText.instance_variable_set(:@html_document_class, root::HTML5::Document)
    ActionText.instance_variable_set(:@html_document_fragment_class, root::HTML5::DocumentFragment)
    ActionText::ContentHelper.sanitizer = vendor.safe_list_sanitizer.new
  end
end
