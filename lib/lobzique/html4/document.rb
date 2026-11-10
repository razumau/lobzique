# frozen_string_literal: true

# Adapted from Nokogiri 1.19.7, lib/nokogiri/html4/document.rb (MIT, see LICENSE-NOKOGIRI.md).
# lobzique: the HTML4 names parse with the HTML5 parser, after decoding the
# input as libxml2 does (plan.md decision 3).

require "pathname"

module Lobzique
  module HTML4
    class Document < XML::Document
      # lobzique: libxml2's first read from an IO, the chunk EncodingReader
      # looks for an encoding in
      IO_CHUNK = 4000
      private_constant :IO_CHUNK

      ###
      # Get the meta tag encoding for this document.  If there is no meta tag,
      # then nil is returned.
      def meta_encoding
        if (meta = at_xpath("//meta[@charset]"))
          meta[:charset]
        elsif (meta = meta_content_type)
          meta["content"][/charset\s*=\s*([\w-]+)/i, 1]
        end
      end

      ###
      # Set the meta tag encoding for this document.
      #
      # If an meta encoding tag is already present, its content is
      # replaced with the given text.
      #
      # Otherwise, this method tries to create one at an appropriate
      # place supplying head and/or html elements as necessary, which
      # is inside a head element if any, and before any text node or
      # content element (typically <body>) if any.
      #
      # The result when trying to set an encoding that is different
      # from the document encoding is undefined.
      def meta_encoding=(encoding)
        if (meta = meta_content_type)
          meta["content"] = format("text/html; charset=%s", encoding)
          encoding
        elsif (meta = at_xpath("//meta[@charset]"))
          meta["charset"] = encoding
        else
          meta = XML::Node.new("meta", self)
          if (dtd = internal_subset) && dtd.html5_dtd?
            meta["charset"] = encoding
          else
            meta["http-equiv"] = "Content-Type"
            meta["content"] = format("text/html; charset=%s", encoding)
          end

          if (head = at_xpath("//head"))
            head.prepend_child(meta)
          else
            set_metadata_element(meta)
          end
          encoding
        end
      end

      def meta_content_type
        xpath("//meta[@http-equiv and boolean(@content)]").find do |node|
          node["http-equiv"] =~ /\AContent-Type\z/i
        end
      end
      private :meta_content_type

      ###
      # Get the title string of this document.  Return nil if there is
      # no title tag.
      def title
        (title = at_xpath("//title")) && title.inner_text
      end

      ###
      # Set the title string of this document.
      #
      # If a title element is already present, its content is replaced
      # with the given text.
      #
      # Otherwise, this method tries to create one at an appropriate
      # place supplying head and/or html elements as necessary, which
      # is inside a head element if any, right after a meta
      # encoding/charset tag if any, and before any text node or
      # content element (typically <body>) if any.
      def title=(text)
        tnode = XML::Text.new(text, self)
        if (title = at_xpath("//title"))
          title.children = tnode
          return text # rubocop:disable Lint/ReturnInVoidContext
        end

        title = XML::Node.new("title", self) << tnode
        if (head = at_xpath("//head"))
          head << title
        elsif (meta = at_xpath("//meta[@charset]") || meta_content_type)
          # better put after charset declaration
          meta.add_next_sibling(title)
        else
          set_metadata_element(title)
        end
      end

      def set_metadata_element(element) # rubocop:disable Naming/AccessorMethodName
        if (head = at_xpath("//head"))
          head << element
        elsif (html = at_xpath("//html"))
          head = html.prepend_child(XML::Node.new("head", self))
          head.prepend_child(element)
        elsif (first = children.find do |node|
                 case node
                 when XML::Element, XML::Text
                   true
                 end
               end)
          # We reach here only if the underlying document model
          # allows <html>/<head> elements to be omitted and does not
          # automatically supply them.
          first.add_previous_sibling(element)
        else
          html = add_child(XML::Node.new("html", self))
          head = html.add_child(XML::Node.new("head", self))
          head.prepend_child(element)
        end
      end
      private :set_metadata_element

      ####
      # Serialize Node using +options+. Save options can also be set using a block.
      def serialize(options = {})
        options[:save_with] ||= XML::Node::SaveOptions::DEFAULT_HTML
        super
      end

      ####
      # Create a Nokogiri::XML::DocumentFragment from +tags+
      def fragment(tags = nil)
        # lobzique: no context. Nokogiri passes the root, and the HTML5
        # parser in the context of <html> would put the input in a <body>
        # of the fragment's own (plan.md decision 3)
        DocumentFragment.new(self, tags)
      end

      # lobzique: CSS::XPathVisitor::DoctypeConfig::HTML4 in Nokogiri
      def xpath_doctype
        :html4
      end

      class << self
        # Parse \HTML4 input from a String or IO object, and return a new HTML4::Document.
        def parse(
          input,
          url_ = nil, encoding_ = nil, options_ = XML::ParseOptions::DEFAULT_HTML,
          url: url_, encoding: encoding_, options: options_
        )
          options = XML::ParseOptions.new(options) if Integer === options
          yield options if block_given?

          url ||= input.respond_to?(:path) ? input.path : nil

          if input.respond_to?(:encoding)
            unless input.encoding == Encoding::ASCII_8BIT
              encoding ||= input.encoding.name
            end
          end

          if input.respond_to?(:read)
            if input.is_a?(Pathname)
              # resolve the Pathname to the file and open it as an IO object, see #2110
              input = input.expand_path.open
              url ||= input.path
            end

            unless encoding
              input = EncodingReader.new(input)
              begin
                return read_io(input, url, encoding, options.to_i)
              rescue EncodingReader::EncodingFound => e
                encoding = e.found_encoding
              end
            end
            return read_io(input, url, encoding, options.to_i)
          end

          # read_memory pukes on empty docs
          if input.nil? || input.empty?
            return encoding ? new.tap { |i| i.encoding = encoding } : new
          end

          encoding ||= EncodingReader.detect_encoding(input)

          read_memory(input, url, encoding, options.to_i)
        end

        # lobzique: native in Nokogiri (libxml2's htmlReadMemory). The
        # options are ignored: the HTML5 parser always recovers, and keeps
        # its own errors, at most 100 as for Node#parse.
        def read_memory(string, url, encoding, options)
          html, encoding, errors = EncodingReader.decode(string, encoding)
          parse_decoded(html, url, encoding, errors)
        end

        # lobzique: native in Nokogiri (libxml2's htmlReadIO), which reads
        # the IO as libxml2 asks for it, so EncodingReader sees the same
        # first chunk.
        def read_io(io, url, encoding, options)
          input = +""
          while (chunk = io.read(IO_CHUNK))
            input << chunk.b
          end
          html, encoding, errors = EncodingReader.decode(input, encoding, chunk: IO_CHUNK)
          parse_decoded(html, url, encoding, errors)
        end

        private

        def parse_decoded(html, url, encoding, errors)
          doc = Html5ever.parse(
            html, url, self,
            max_attributes: Html5ever::DEFAULT_MAX_ATTRIBUTES,
            max_errors: 100,
            max_tree_depth: Html5ever::DEFAULT_MAX_TREE_DEPTH
          )
          doc.encoding = encoding if encoding
          doc.errors = errors + doc.errors
          doc
        end
      end
    end
  end
end
