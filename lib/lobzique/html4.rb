# frozen_string_literal: true

# Adapted from Nokogiri 1.19.7, lib/nokogiri/html4.rb (MIT, see LICENSE-NOKOGIRI.md).
# The HTML4 names keep Nokogiri's hierarchy but parse and serialize with the
# HTML5 parser (plan.md decision 3).

module Lobzique
  class << self
    # Convenience method for Nokogiri::HTML4::Document.parse
    def HTML4(...)
      HTML4::Document.parse(...)
    end
  end

  module HTML4
    class << self
      # Convenience method for Nokogiri::HTML4::Document.parse
      def parse(...)
        Document.parse(...)
      end

      # Convenience method for Nokogiri::HTML4::DocumentFragment.parse
      def fragment(...)
        HTML4::DocumentFragment.parse(...)
      end
    end

    # lobzique: NamedCharacters (EntityLookup), SAX and ElementDescription
    # are left out: they read libxml2's HTML4 tables, or are SAX
  end
end

require_relative "html4/document"
require_relative "html4/document_fragment"
require_relative "html4/encoding_reader"
