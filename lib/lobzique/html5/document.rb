# frozen_string_literal: true

# Adapted from Nokogiri 1.19.4, lib/nokogiri/html5/document.rb (MIT, see LICENSE-NOKOGIRI.md).

require_relative "../html4/document"

module Lobzique
  module HTML5
    module QuirksMode
      NO_QUIRKS = 0
      QUIRKS = 1
      LIMITED_QUIRKS = 2
    end

    class Document < HTML4::Document
      attr_reader :url
      attr_reader :quirks_mode

      def initialize(*args) # :nodoc:
        super
        @url = nil
        @quirks_mode = nil
      end

      # Parses markup in the context of `<body>`.
      def fragment(markup = nil)
        DocumentFragment.new(self, markup)
      end
    end
  end
end
