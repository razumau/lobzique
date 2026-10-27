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
        return self unless tags # rubocop:disable Lint/ReturnInVoidContext

        options = ParseOptions.new(options) if Integer === options
        @parse_options = options
        yield options if block_given?

        children = if context
          context.parse(tags, options)
        else
          # lobzique: XML parsing is out of scope (plan.md decision 4).
          # HTML4::DocumentFragment reaches this branch until its own
          # #initialize is ported (plan.md, phase 2 step 7).
          raise NotImplementedError, "#{XML::Document}.parse isn't supported"
        end
        children.each { |child| child.parent = self }
      end

      # A copy in a copy of the document.
      def dup
        new_document = document.dup
        new_fragment = self.class.new(new_document)
        children.each do |child|
          child.dup(1, new_document).parent = new_fragment
        end
        new_fragment
      end

      def name
        "#document-fragment"
      end

      def to_s
        children.to_s
      end

      def to_html(*args)
        children.to_html(*args)
      end

      # lobzique: to_xhtml and to_xml wait for native_write_to (plan.md,
      # phase 2 step 8)

      def errors
        document.errors
      end

      def errors=(things) # :nodoc:
        document.errors = things
      end

      def fragment(data)
        document.fragment(data)
      end

      def deconstruct
        children.to_a
      end
    end
  end
end
