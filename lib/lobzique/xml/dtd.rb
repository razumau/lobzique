# frozen_string_literal: true

# Adapted from Nokogiri 1.19.4, lib/nokogiri/xml/dtd.rb (MIT, see LICENSE-NOKOGIRI.md).

module Lobzique
  module XML
    class DTD < Node
      def html_dtd?
        name.casecmp("html").zero?
      end
    end
  end
end
