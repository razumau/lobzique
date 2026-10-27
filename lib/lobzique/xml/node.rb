# frozen_string_literal: true

# Adapted from Nokogiri 1.19.4, lib/nokogiri/xml/node.rb (MIT, see LICENSE-NOKOGIRI.md).

require "stringio"

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

      def to_s
        document.xml? ? to_xml : to_html
      end

      def inner_html(*args)
        children.map { |x| x.to_html(*args) }.join
      end

      # :section: Serialization and Generating Output

      # Save options can also be set with a block, which gets a SaveOptions.
      def serialize(*args, &block)
        options = if args.first.is_a?(Hash)
          args.shift
        else
          {
            encoding: args[0],
            save_with: args[1]
          }
        end

        options[:encoding] ||= document.encoding
        encoding = Encoding.find(options[:encoding] || "UTF-8")

        io = StringIO.new(String.new(encoding: encoding))

        write_to(io, options, &block)
        io.string
      end

      def to_html(options = {})
        to_format(SaveOptions::DEFAULT_HTML, options)
      end

      # lobzique: to_xml, to_xhtml, write_to and the write_*_to methods
      # that use them wait for native_write_to (plan.md, phase 2 step 8)

      def write_html_to(io, options = {})
        write_format_to(SaveOptions::DEFAULT_HTML, io, options)
      end

      private

      def to_format(save_option, options)
        options[:save_with] = save_option unless options[:save_with]
        serialize(options)
      end

      def write_format_to(save_option, io, options)
        options[:save_with] ||= save_option
        write_to(io, options)
      end

      def inspect_attributes
        [:name, :namespace, :attribute_nodes, :children]
      end
    end
  end
end

require_relative "node/save_options"
