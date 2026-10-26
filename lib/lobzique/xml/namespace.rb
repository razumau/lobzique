# frozen_string_literal: true

# Adapted from Nokogiri 1.19.4, lib/nokogiri/xml/namespace.rb (MIT, see LICENSE-NOKOGIRI.md).

module Lobzique
  module XML
    class Namespace
      include PP::Node

      attr_reader :document

      private

      def inspect_attributes
        [:prefix, :href]
      end
    end
  end
end
