# frozen_string_literal: true

# Adapted from Nokogiri 1.19.4, lib/nokogiri/xml/document_fragment.rb (MIT, see LICENSE-NOKOGIRI.md).

module Lobzique
  module XML
    class DocumentFragment < Node
      attr_reader :parse_options

      class << self
        # The native node is made, wrapped and decorated first, then filled
        # in by `initialize`, which is what subclasses override.
        def new(document, ...) # :nodoc:
          instance = native_new(document)
          instance.send(:initialize, document, ...)
          instance
        end
      end

      def initialize(
        document, tags = nil,
        context_ = nil, options_ = ParseOptions::DEFAULT_XML,
        context: context_, options: options_
      ) # rubocop:disable Lint/MissingSuper
        return unless tags

        options = ParseOptions.new(options) if Integer === options
        @parse_options = options
        yield options if block_given?

        children = if context
          context.parse(tags, options)
        else
          wrapper_doc = XML::Document.parse("<root>#{tags}</root>", nil, nil, options)
          self.errors = wrapper_doc.errors
          wrapper_doc.xpath("/root/node()")
        end
        children.each { |child| child.parent = self }
      end

      def name
        "#document-fragment"
      end

      def errors
        document.errors
      end

      def errors=(things) # :nodoc:
        document.errors = things
      end
    end
  end
end
