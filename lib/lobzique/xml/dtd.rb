# frozen_string_literal: true

# Adapted from Nokogiri 1.19.4, lib/nokogiri/xml/dtd.rb (MIT, see LICENSE-NOKOGIRI.md).

module Lobzique
  module XML
    class DTD < Node
      # lobzique: Nokogiri also undefines values, namespace_definitions and
      # line, which aren't defined yet
      undef_method :attribute_nodes
      undef_method :content
      undef_method :namespace

      def html_dtd?
        name.casecmp("html").zero?
      end

      def html5_dtd?
        html_dtd? &&
          external_id.nil? &&
          (system_id.nil? || system_id == "about:legacy-compat")
      end
    end
  end
end
