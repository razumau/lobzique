# frozen_string_literal: true

# Adapted from Nokogiri 1.19.4, lib/nokogiri/xml/xpath_context.rb (MIT, see LICENSE-NOKOGIRI.md).

module Lobzique
  module XML
    class XPathContext
      def register_namespaces(namespaces)
        namespaces.each do |key, value|
          key = key.to_s.gsub(/.*:/, "") # strip off 'xmlns:' or 'xml:'

          register_ns(key, value)
        end
      end

      def register_variables(binds)
        return if binds.nil?

        binds.each do |key, value|
          key = key.to_s

          register_variable(key, value)
        end
      end
    end
  end
end
