# frozen_string_literal: true

# Adapted from Nokogiri 1.19.4, lib/nokogiri/xml/attr.rb (MIT, see LICENSE-NOKOGIRI.md).

module Lobzique
  module XML
    class Attr < Node
      private

      def inspect_attributes
        [:name, :namespace, :value]
      end
    end
  end
end
