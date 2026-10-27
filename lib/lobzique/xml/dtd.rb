# frozen_string_literal: true

# Adapted from Nokogiri 1.19.4, lib/nokogiri/xml/dtd.rb (MIT, see LICENSE-NOKOGIRI.md).

module Lobzique
  module XML
    class DTD < Node
      undef_method :attribute_nodes
      undef_method :values
      undef_method :content
      undef_method :namespace
      undef_method :namespace_definitions
      # lobzique: Nokogiri also undefines line, which isn't defined yet, and
      # defines keys and each over the DTD's attribute declarations, which
      # are out of scope

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
