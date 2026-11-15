# frozen_string_literal: true

# A rails-html-sanitizer vendor that parses, scrubs and serializes with
# lobzique (plan.md phase 3):
#
#   config.action_view.sanitizer_vendor = Lobzique::Sanitizer
#   config.action_text.sanitizer_vendor = Lobzique::Sanitizer
#
# The scrubbers of Loofah and Rails run unchanged on lobzique nodes. Only
# the documents and scrub! of Loofah, and the scrubbers that build Nokogiri
# nodes, are ported.

require "lobzique"
require "loofah"
require "rails-html-sanitizer"

module Lobzique
  module Sanitizer
    # The vendor classes include the concerns of rails-html-sanitizer,
    # which are internals. Only the versions that the tests ran on are accepted.
    RAILS_HTML_SANITIZER_REQUIREMENT = Gem::Requirement.new(">= 1.7.1", "< 1.8")

    unless RAILS_HTML_SANITIZER_REQUIREMENT.satisfied_by?(Gem::Version.new(Rails::HTML::Sanitizer::VERSION))
      raise "#{self} needs rails-html-sanitizer #{RAILS_HTML_SANITIZER_REQUIREMENT}, " \
        "not #{Rails::HTML::Sanitizer::VERSION}"
    end
  end
end

require_relative "sanitizer/escape_raw_text_in_select"
require_relative "sanitizer/concerns"
require_relative "sanitizer/scrubbers"
require_relative "sanitizer/document"
require_relative "sanitizer/document_fragment"

module Lobzique
  module Sanitizer
    # Rails::HTML::Concern::Parser::HTML5, with the fragment of lobzique
    # in place of the fragment of Loofah.
    module Parser
      def parse_fragment(html)
        DocumentFragment.parse(html)
      end
    end

    # The vendor stands in for Rails::HTML5, so its safe list sanitizer reads
    # and writes the lists of Rails::HTML5. Apps and gems edit them on the class
    # of Rails by name.
    module RailsAllowedLists
      %i[allowed_tags allowed_tags= allowed_attributes allowed_attributes=].each do |name|
        define_method(name) { |*args| Rails::HTML5::SafeListSanitizer.public_send(name, *args) }
      end
    end

    class FullSanitizer < Rails::HTML::Sanitizer
      include Rails::HTML::Concern::ComposedSanitize
      include Parser
      include Rails::HTML::Concern::Scrubber::Full
      include Rails::HTML::Concern::Serializer::UTF8Encode
    end

    class LinkSanitizer < Rails::HTML::Sanitizer
      include Rails::HTML::Concern::ComposedSanitize
      include Parser
      include Rails::HTML::Concern::Scrubber::Link
      include Rails::HTML::Concern::Serializer::UTF8Encode
    end

    class SafeListSanitizer < Rails::HTML::Sanitizer
      include Rails::HTML::Concern::ComposedSanitize
      include Parser
      include Rails::HTML::Concern::Scrubber::SafeList
      include Rails::HTML::Concern::Serializer::UTF8Encode
      singleton_class.prepend(RailsAllowedLists)
    end

    class << self
      def full_sanitizer
        FullSanitizer
      end

      def link_sanitizer
        LinkSanitizer
      end

      def safe_list_sanitizer
        SafeListSanitizer
      end

      def white_list_sanitizer # :nodoc:
        safe_list_sanitizer
      end
    end
  end
end
