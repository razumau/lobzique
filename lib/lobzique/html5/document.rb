# frozen_string_literal: true

# Adapted from Nokogiri 1.19.7, lib/nokogiri/html5/document.rb (MIT, see LICENSE-NOKOGIRI.md).

require_relative "../html4/document"

module Lobzique
  module HTML5
    module QuirksMode
      NO_QUIRKS = 0
      QUIRKS = 1
      LIMITED_QUIRKS = 2
    end

    class Document < HTML4::Document
      attr_reader :url
      attr_reader :quirks_mode

      # Whether the parser treated the content of +noscript+ elements as text, or +nil+ before the
      # first parse. Fragments parsed into this document must match it, because the serializer emits
      # +noscript+ text literally only when this is true.
      attr_accessor :parse_noscript_content_as_text # :nodoc:

      class << self
        # Parses with the HTML standard's algorithm. The block, if given,
        # gets the options Hash; `url:` and `encoding:` can't be set there.
        def parse(
          string_or_io,
          url_ = nil, encoding_ = nil,
          url: url_, encoding: encoding_,
          **options, &block
        )
          yield options if block
          string_or_io ||= ""

          if string_or_io.respond_to?(:encoding) && string_or_io.encoding != Encoding::ASCII_8BIT
            encoding ||= string_or_io.encoding.name
          end

          if string_or_io.respond_to?(:read) && string_or_io.respond_to?(:path)
            url ||= string_or_io.path
          end
          unless string_or_io.respond_to?(:read) || string_or_io.respond_to?(:to_str)
            raise ArgumentError, "not a string or IO object"
          end

          do_parse(string_or_io, url, encoding, **options)
        end

        def read_io(io, url_ = nil, encoding_ = nil, url: url_, encoding: encoding_, **options)
          raise ArgumentError, "io object doesn't respond to :read" unless io.respond_to?(:read)

          do_parse(io, url, encoding, **options)
        end

        def read_memory(string, url_ = nil, encoding_ = nil, url: url_, encoding: encoding_, **options)
          raise ArgumentError, "string object doesn't respond to :to_str" unless string.respond_to?(:to_str)

          do_parse(string, url, encoding, **options)
        end

        private

        def do_parse(string_or_io, url, encoding, **options)
          string = HTML5.read_and_encode(string_or_io, encoding)

          # lobzique: Html5ever is Nokogiri's Gumbo, renamed after the engine
          options[:max_attributes] ||= Html5ever::DEFAULT_MAX_ATTRIBUTES
          options[:max_errors] ||= options.delete(:max_parse_errors) || Html5ever::DEFAULT_MAX_ERRORS
          options[:max_tree_depth] ||= Html5ever::DEFAULT_MAX_TREE_DEPTH

          noscript_content_as_text = options[:parse_noscript_content_as_text] ? true : false

          doc = Html5ever.parse(string, url, self, **options)
          doc.encoding = "UTF-8"
          doc.parse_noscript_content_as_text = noscript_content_as_text
          doc
        end
      end

      def initialize(*args) # :nodoc:
        super
        @url = nil
        @quirks_mode = nil
        @parse_noscript_content_as_text = nil
      end

      # Parses markup in the context of `<body>`.
      def fragment(markup = nil)
        DocumentFragment.new(self, markup)
      end

      def to_xml(options = {}, &block) # :nodoc:
        # Bypass XML::Document#to_xml which doesn't add
        # XML::Node::SaveOptions::AS_XML like XML::Node#to_xml does.
        XML::Node.instance_method(:to_xml).bind_call(self, options, &block)
      end

      # lobzique: CSS::XPathVisitor::DoctypeConfig::HTML5 in Nokogiri
      def xpath_doctype
        :html5
      end
    end
  end
end
