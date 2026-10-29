# frozen_string_literal: true

# Sends real Nokogiri's HTML4 names through gumbo, to measure plan.md
# decision 3 on the suites of gems that use them. run.rb loads it with
# RUBYOPT="-rbundler/setup -r<this file>".
#
#   HTML4_MODE=html5-tree  HTML5 tree; libxml2's HTML serializer, plus the
#                          HTML 4.0 doctype libxml2's parser adds when the
#                          input has none
#   HTML4_MODE=html5       HTML5 tree and serializer (decision 3)
#   HTML4_FRAGMENT_CONTEXT=template  parse every HTML4 fragment in a
#                          <template> context
#
# In both modes HTML4 documents get HTML5::Node's DOM overrides and HTML5's
# CSS doctype config, and Node#parse uses the HTML5 fragment parser with the
# node as context, as lobzique does.

require "nokogiri"
require "pathname"

module HTML4Shim
  MODE = ENV.fetch("HTML4_MODE")
  raise "HTML4_MODE must be html5-tree or html5" unless %w[html5-tree html5].include?(MODE)

  HTML4_DTD = ["html", "-//W3C//DTD HTML 4.0 Transitional//EN", "http://www.w3.org/TR/REC-html40/loose.dtd"].freeze
  GUMBO_OPTIONS = {
    max_attributes: Nokogiri::Gumbo::DEFAULT_MAX_ATTRIBUTES,
    max_errors: Nokogiri::Gumbo::DEFAULT_MAX_ERRORS,
    max_tree_depth: Nokogiri::Gumbo::DEFAULT_MAX_TREE_DEPTH
  }.freeze

  def self.html4_only?(node)
    doc = node.document
    doc.is_a?(Nokogiri::HTML4::Document) && !doc.is_a?(Nokogiri::HTML5::Document)
  end

  # Gumbo wraps the tree as `klass` without calling #initialize.
  def self.parse(klass, input, url, encoding)
    string = Nokogiri::HTML5.read_and_encode(input, encoding)
    doc = Nokogiri::Gumbo.parse(string, url, klass, **GUMBO_OPTIONS)
    doc.encoding = "UTF-8"
    doc.create_internal_subset(*HTML4_DTD) if MODE == "html5-tree" && !doc.internal_subset
    doc
  end

  module DocumentClass
    def parse(input, url_ = nil, encoding_ = nil, options_ = Nokogiri::XML::ParseOptions::DEFAULT_HTML,
      url: url_, encoding: encoding_, options: options_)
      return super if self <= Nokogiri::HTML5::Document

      options = Nokogiri::XML::ParseOptions.new(options) if Integer === options
      yield options if block_given?
      input = input.expand_path.open if input.is_a?(Pathname)
      url ||= input.respond_to?(:path) ? input.path : nil
      if input.respond_to?(:encoding) && input.encoding != Encoding::ASCII_8BIT
        encoding ||= input.encoding.name
      end
      HTML4Shim.parse(self, input || "", url, encoding)
    end

    def read_memory(string, url = nil, encoding = nil, _options = nil)
      return super if self <= Nokogiri::HTML5::Document

      HTML4Shim.parse(self, string, url, encoding)
    end

    def read_io(io, url = nil, encoding = nil, _options = nil)
      return super if self <= Nokogiri::HTML5::Document

      HTML4Shim.parse(self, io, url, encoding)
    end
  end
  Nokogiri::HTML4::Document.singleton_class.prepend(DocumentClass)

  module Document
    # HTML4 passes the root element as context; HTML5 passes none.
    def fragment(tags = nil)
      Nokogiri::HTML4::DocumentFragment.new(self, tags)
    end

    def xpath_doctype
      Nokogiri::CSS::XPathVisitor::DoctypeConfig::HTML5
    end
  end
  Nokogiri::HTML4::Document.prepend(Document)

  module DocumentFragment
    def initialize(document, input = nil, context_ = nil, options_ = Nokogiri::XML::ParseOptions::DEFAULT_HTML,
      context: context_, options: options_)
      return unless input

      options = Nokogiri::XML::ParseOptions.new(options) if Integer === options
      @parse_options = options
      yield options if block_given?
      context = nil if context.is_a?(Nokogiri::XML::Document)
      context = "template" if ENV["HTML4_FRAGMENT_CONTEXT"] == "template"
      Nokogiri::Gumbo.fragment(self, Nokogiri::HTML5.read_and_encode(input, nil), context, **GUMBO_OPTIONS)
    end
  end
  Nokogiri::HTML4::DocumentFragment.prepend(DocumentFragment)

  module Node
    def parse(string_or_io, options = nil)
      return super unless document.is_a?(Nokogiri::HTML4::Document)

      unless options.is_a?(Nokogiri::XML::ParseOptions)
        options = Nokogiri::XML::ParseOptions.new(options || Nokogiri::XML::ParseOptions::DEFAULT_HTML)
      end
      yield options if block_given?
      contents = string_or_io.respond_to?(:read) ? string_or_io.read : string_or_io
      return Nokogiri::XML::NodeSet.new(document) if contents.empty?

      context = element? ? self : parent
      context = nil unless context.is_a?(Nokogiri::XML::Element)
      fragment_class = HTML4Shim.html4_only?(self) ? Nokogiri::HTML4::DocumentFragment : Nokogiri::HTML5::DocumentFragment
      fragment_class.new(document, contents, context).children
    end

    private

    def add_child_node_and_reparent_attrs(node)
      return super unless HTML4Shim.html4_only?(self)

      add_child_node(node)
    end
  end
  Nokogiri::XML::Node.prepend(Node)

  # HTML5::Node's serializer overrides, for HTML4 documents too.
  module Serializer
    def inner_html(options = {})
      return super unless HTML4Shim.html4_only?(self)

      result = (options[:preserve_newline] && prepend_newline?) ? +"\n" : +""
      result << children.map { |child| child.to_html(options) }.join
      result
    end

    def write_to(io, *options)
      return super unless HTML4Shim.html4_only?(self)

      options = options.first.is_a?(Hash) ? options.shift : {}
      encoding = options[:encoding] || options[0]
      save_options = options[:save_with] || options[1] || Nokogiri::XML::Node::SaveOptions::FORMAT
      indent_string = (options[:indent_text] || " ") * (options[:indent] || 2)
      config = Nokogiri::XML::Node::SaveOptions.new(save_options.to_i)
      yield config if block_given?
      encoding = encoding.name if encoding.is_a?(Encoding)
      if config.options & (Nokogiri::XML::Node::SaveOptions::AS_XML | Nokogiri::XML::Node::SaveOptions::AS_XHTML) != 0
        native_write_to(io, encoding, indent_string, config.options)
      else
        html = html_standard_serialize(options[:preserve_newline] || false)
        encoding ||= document.encoding || Encoding::UTF_8
        io << html.encode(encoding, fallback: lambda { |c| "&#x#{c.ord.to_s(16)};" })
      end
    end
  end
  Nokogiri::XML::Node.prepend(Serializer) if MODE == "html5"
end
