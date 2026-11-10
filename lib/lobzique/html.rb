# frozen_string_literal: true

# Adapted from Nokogiri 1.19.7, lib/nokogiri/html.rb (MIT, see LICENSE-NOKOGIRI.md).

require_relative "html4"

module Lobzique
  # Alias for Nokogiri::HTML4
  HTML = HTML4

  # :singleton-method: HTML
  # Parse HTML. Convenience method for Nokogiri::HTML4::Document.parse
  define_singleton_method(:HTML, Lobzique.method(:HTML4))
end
