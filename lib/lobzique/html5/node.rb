# frozen_string_literal: true

# Adapted from Nokogiri 1.19.4, lib/nokogiri/html5/node.rb (MIT, see LICENSE-NOKOGIRI.md).

require_relative "../xml/node"

module Lobzique
  module HTML5
    # Overrides prepended into every node class, active in HTML documents.
    module Node
      def inner_html(options = {})
        # lobzique: Nokogiri checks for HTML5::Document; HTML4 documents use
        # the HTML5 serializer too (plan.md decision 3)
        return super unless document.is_a?(HTML4::Document)

        result = (options[:preserve_newline] && prepend_newline?) ? +"\n" : +""
        result << children.map { |child| child.to_html(options) }.join
        result
      end

      def write_to(io, *options)
        return super unless document.is_a?(HTML4::Document) # lobzique: as in #inner_html

        options = options.first.is_a?(Hash) ? options.shift : {}
        encoding = options[:encoding] || options[0]
        save_options = options[:save_with] || options[1] || XML::Node::SaveOptions::FORMAT
        indent_times = options[:indent] || 2
        indent_string = (options[:indent_text] || " ") * indent_times

        config = XML::Node::SaveOptions.new(save_options.to_i)
        yield config if block_given?

        encoding = encoding.is_a?(Encoding) ? encoding.name : encoding

        config_options = config.options
        if config_options & (XML::Node::SaveOptions::AS_XML | XML::Node::SaveOptions::AS_XHTML) != 0
          native_write_to(io, encoding, indent_string, config_options)
        else
          html = html_standard_serialize(options[:preserve_newline] || false)
          encoding ||= document.encoding || Encoding::UTF_8
          io << html.encode(encoding, fallback: lambda { |c| "&#x#{c.ord.to_s(16)};" })
        end
      end

      def fragment(tags)
        # lobzique: Nokogiri checks for HTML5::Document; HTML4 documents use
        # the HTML5 parser too (plan.md decision 3)
        return super unless document.is_a?(HTML4::Document)

        DocumentFragment.new(document, tags, self)
      end
    end

    XML::Node.prepend(HTML5::Node)
  end
end
