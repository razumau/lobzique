# frozen_string_literal: true

# Adapted from Nokogiri 1.19.4, lib/nokogiri/xml/node.rb (MIT, see LICENSE-NOKOGIRI.md).

module Lobzique
  module XML
    class Node
      include PP::Node
      include Searchable
      include ClassResolver
      include Enumerable

      # libxml2's node types, which `#type` returns
      ELEMENT_NODE = 1
      ATTRIBUTE_NODE = 2
      TEXT_NODE = 3
      CDATA_SECTION_NODE = 4
      ENTITY_REF_NODE = 5
      ENTITY_NODE = 6
      PI_NODE = 7
      COMMENT_NODE = 8
      DOCUMENT_NODE = 9
      DOCUMENT_TYPE_NODE = 10
      DOCUMENT_FRAG_NODE = 11
      NOTATION_NODE = 12
      HTML_DOCUMENT_NODE = 13
      DTD_NODE = 14
      ELEMENT_DECL = 15
      ATTRIBUTE_DECL = 16
      ENTITY_DECL = 17
      NAMESPACE_DECL = 18
      XINCLUDE_START = 19
      XINCLUDE_END = 20
      DOCB_DOCUMENT_NODE = 21

      # Runs after the native `Node.new` has made the node, so subclasses can
      # override it; intentionally empty.
      def initialize(name, document)
      end

      def decorate!
        document.decorate(self)
      end

      alias_method :name, :node_name
      alias_method :type, :node_type

      def fragment(tags)
        document.related_class("DocumentFragment").new(document, tags, self)
      end

      def comment?
        type == COMMENT_NODE
      end

      def cdata?
        type == CDATA_SECTION_NODE
      end

      def xml?
        type == DOCUMENT_NODE
      end

      def html?
        type == HTML_DOCUMENT_NODE
      end

      def document?
        is_a?(XML::Document)
      end

      def processing_instruction?
        type == PI_NODE
      end

      def text?
        type == TEXT_NODE
      end

      def fragment?
        type == DOCUMENT_FRAG_NODE
      end

      def read_only?
        [NOTATION_NODE, ENTITY_NODE, ENTITY_DECL].include?(type)
      end

      def element?
        type == ELEMENT_NODE
      end

      alias_method :elem?, :element?

      private

      def inspect_attributes
        [:name, :namespace, :attribute_nodes, :children]
      end
    end
  end
end

require_relative "node/save_options"
