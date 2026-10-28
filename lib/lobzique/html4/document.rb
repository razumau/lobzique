# frozen_string_literal: true

# Adapted from Nokogiri 1.19.4, lib/nokogiri/html4/document.rb (MIT, see LICENSE-NOKOGIRI.md).

module Lobzique
  module HTML4
    class Document < XML::Document
      def serialize(options = {})
        options[:save_with] ||= XML::Node::SaveOptions::DEFAULT_HTML
        super
      end

      # lobzique: CSS::XPathVisitor::DoctypeConfig::HTML4 in Nokogiri
      def xpath_doctype
        :html4
      end
    end
  end
end
