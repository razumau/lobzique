# frozen_string_literal: true

# Adapted from Nokogiri 1.19.4, lib/nokogiri/xml/cdata.rb (MIT, see LICENSE-NOKOGIRI.md).

module Lobzique
  module XML
    class CDATA < Text
      def name
        "#cdata-section"
      end
    end
  end
end
