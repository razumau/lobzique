# frozen_string_literal: true

# Adapted from Nokogiri 1.19.7, lib/nokogiri/html4/document_fragment.rb (MIT, see LICENSE-NOKOGIRI.md).

module Lobzique
  module HTML4
    class DocumentFragment < XML::DocumentFragment
      # Parse a fragment in a new HTML4::Document whose encoding is the
      # input's, or UTF-8 for binary input.
      def self.parse(
        input,
        encoding_ = nil, options_ = XML::ParseOptions::DEFAULT_HTML,
        encoding: encoding_, options: options_,
        &block
      )
        doc = HTML4::Document.new

        if input.respond_to?(:read)
          input.set_encoding(encoding) if encoding && input.respond_to?(:set_encoding)
          input = input.read
        end

        encoding ||= if input.respond_to?(:encoding)
          encoding = input.encoding
          if encoding == ::Encoding::ASCII_8BIT
            "UTF-8"
          else
            encoding.name
          end
        else
          "UTF-8"
        end

        doc.encoding = encoding

        new(doc, input, options: options, &block)
      end

      def initialize(
        document, input = nil,
        context_ = nil, options_ = XML::ParseOptions::DEFAULT_HTML,
        context: context_, options: options_
      ) # rubocop:disable Lint/MissingSuper
        return self unless input # rubocop:disable Lint/ReturnInVoidContext

        options = XML::ParseOptions.new(options) if Integer === options
        @parse_options = options
        yield options if block_given?

        # lobzique: the HTML5 fragment parser, in <body> or in the context
        # given (plan.md decision 3). Nokogiri parses the input in a document
        # of its own, or as a <div>'s content in the context. The input is
        # decoded as that document's parse would decode it.
        input = input.to_str
        encoding = document.encoding
        encoding ||= input.encoding.name unless input.encoding == Encoding::ASCII_8BIT
        encoding ||= EncodingReader.detect_encoding(input)
        html, _, errors = EncodingReader.decode(input, encoding)

        Html5ever.fragment(
          self, html, context,
          max_attributes: Html5ever::DEFAULT_MAX_ATTRIBUTES,
          max_errors: 100,
          max_tree_depth: Html5ever::DEFAULT_MAX_TREE_DEPTH
        )
        # Html5ever.fragment keeps errors in @errors, as HTML5's fragments do
        parse_errors = instance_variable_defined?(:@errors) ? remove_instance_variable(:@errors) : []
        self.errors = errors + parse_errors
        children
      end
    end
  end
end
