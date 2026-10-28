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

      # A copy of the node: 0 copies the node alone, 2 also its attributes,
      # and 1, the default, the whole subtree.
      def dup(level = 1, new_parent_doc = document)
        super().initialize_copy_with_args(self, level, new_parent_doc)
      end

      def clone(level = 1, new_parent_doc = document)
        super().initialize_copy_with_args(self, level, new_parent_doc)
      end

      def decorate!
        document.decorate(self)
      end

      # :section: Manipulating Document Structure

      def add_child(node_or_tags)
        node_or_tags = coerce(node_or_tags)
        if node_or_tags.is_a?(XML::NodeSet)
          node_or_tags.each { |n| add_child_node_and_reparent_attrs(n) }
        else
          add_child_node_and_reparent_attrs(node_or_tags)
        end
        node_or_tags
      end

      def prepend_child(node_or_tags)
        if (first = children.first)
          # Mimic the error add_child would raise.
          raise "Document already has a root node" if document? && !(node_or_tags.comment? || node_or_tags.processing_instruction?)

          first.__send__(:add_sibling, :previous, node_or_tags)
        else
          add_child(node_or_tags)
        end
      end

      def wrap(node_or_tags)
        case node_or_tags
        when String
          context_node = parent || document
          new_parent = context_node.coerce(node_or_tags).first
          if new_parent.nil?
            raise "Failed to parse '#{node_or_tags}' in the context of a '#{context_node.name}' element"
          end
        when Node
          new_parent = node_or_tags.dup
        else
          raise ArgumentError, "Requires a String or Node argument, and cannot accept a #{node_or_tags.class}"
        end

        if parent
          add_next_sibling(new_parent)
        else
          new_parent.unlink
        end
        new_parent.add_child(self)

        self
      end

      def <<(node_or_tags)
        add_child(node_or_tags)
        self
      end

      def add_previous_sibling(node_or_tags)
        if parent&.document? && !(node_or_tags.comment? || node_or_tags.processing_instruction?)
          raise ArgumentError, "A document may not have multiple root nodes."
        end

        add_sibling(:previous, node_or_tags)
      end

      def add_next_sibling(node_or_tags)
        if parent&.document? && !(node_or_tags.comment? || node_or_tags.processing_instruction?)
          raise ArgumentError, "A document may not have multiple root nodes."
        end

        add_sibling(:next, node_or_tags)
      end

      def before(node_or_tags)
        add_previous_sibling(node_or_tags)
        self
      end

      def after(node_or_tags)
        add_next_sibling(node_or_tags)
        self
      end

      def inner_html=(node_or_tags)
        self.children = node_or_tags
      end

      def children=(node_or_tags)
        node_or_tags = coerce(node_or_tags)
        children.unlink
        if node_or_tags.is_a?(XML::NodeSet)
          node_or_tags.each { |n| add_child_node_and_reparent_attrs(n) }
        else
          add_child_node_and_reparent_attrs(node_or_tags)
        end
      end

      def replace(node_or_tags)
        raise("Cannot replace a node with no parent") unless parent

        # We cannot replace a text node directly, otherwise libxml will return
        # an internal error at parser.c:13031, I don't know exactly why
        # libxml is trying to find a parent node that is an element or document
        # so I can't tell if this is bug in libxml or not. issue #775.
        if text?
          replacee = XML::Node.new("dummy", document)
          add_previous_sibling_node(replacee)
          unlink
          return replacee.replace(node_or_tags)
        end

        node_or_tags = parent.coerce(node_or_tags)

        if node_or_tags.is_a?(XML::NodeSet)
          node_or_tags.each { |n| add_previous_sibling(n) }
          unlink
        else
          replace_node(node_or_tags)
        end
        node_or_tags
      end

      def swap(node_or_tags)
        replace(node_or_tags)
        self
      end

      # Sets the content to +string+ as text, escaped for the native setter,
      # which unescapes it.
      def content=(string)
        self.native_content = encode_special_chars(string.to_s)
      end

      def parent=(parent_node)
        parent_node.add_child(self)
      end

      # lobzique: default_namespace=, namespace= and do_xinclude need
      # namespace definitions and XInclude, which are out of scope

      alias_method :next, :next_sibling
      alias_method :previous, :previous_sibling
      alias_method :next=, :add_next_sibling
      alias_method :previous=, :add_previous_sibling
      alias_method :remove, :unlink
      alias_method :name=, :node_name=
      alias_method :inner_text, :content
      alias_method :text, :content
      alias_method :to_str, :content
      alias_method :name, :node_name
      alias_method :type, :node_type
      alias_method :elements, :element_children

      # :section: Working With Node Attributes

      def [](name)
        get(name.to_s)
      end

      def []=(name, value)
        set(name.to_s, value.to_s)
      end

      def attributes
        attribute_nodes.each_with_object({}) do |node, hash|
          hash[node.node_name] = node
        end
      end

      def values
        attribute_nodes.map(&:value)
      end

      def value?(value)
        values.include?(value)
      end

      def keys
        attribute_nodes.map(&:node_name)
      end

      def each
        attribute_nodes.each do |node|
          yield [node.node_name, node.value]
        end
      end

      def remove_attribute(name)
        attributes[name].remove if key?(name)
      end

      def classes
        kwattr_values("class")
      end

      def add_class(names)
        kwattr_add("class", names)
      end

      def append_class(names)
        kwattr_append("class", names)
      end

      def remove_class(names = nil)
        kwattr_remove("class", names)
      end

      def kwattr_values(attribute_name)
        keywordify(get_attribute(attribute_name) || [])
      end

      def kwattr_add(attribute_name, keywords)
        keywords = keywordify(keywords)
        current_kws = kwattr_values(attribute_name)
        new_kws = (current_kws + (keywords - current_kws)).join(" ")
        set_attribute(attribute_name, new_kws)
        self
      end

      def kwattr_append(attribute_name, keywords)
        keywords = keywordify(keywords)
        current_kws = kwattr_values(attribute_name)
        new_kws = (current_kws + keywords).join(" ")
        set_attribute(attribute_name, new_kws)
        self
      end

      def kwattr_remove(attribute_name, keywords)
        if keywords.nil?
          remove_attribute(attribute_name)
          return self
        end

        keywords = keywordify(keywords)
        current_kws = kwattr_values(attribute_name)
        new_kws = current_kws - keywords
        if new_kws.empty?
          remove_attribute(attribute_name)
        else
          set_attribute(attribute_name, new_kws.join(" "))
        end
        self
      end

      alias_method :delete, :remove_attribute
      alias_method :get_attribute, :[]
      alias_method :attr, :[]
      alias_method :set_attribute, :[]=
      alias_method :has_attribute?, :key?

      def matches?(selector)
        ancestors.last.search(selector).include?(self)
      end

      def fragment(tags)
        document.related_class("DocumentFragment").new(document, tags, self)
      end

      def namespaces
        namespace_scopes.each_with_object({}) do |ns, hash|
          prefix = ns.prefix
          key = prefix ? "xmlns:#{prefix}" : "xmlns"
          hash[key] = ns.href
        end
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

      def css_path
        path.split(%r{/}).filter_map do |part|
          part.empty? ? nil : part.gsub(/\[(\d+)\]/, ':nth-of-type(\1)')
        end.join(" > ")
      end

      def ancestors(selector = nil)
        return NodeSet.new(document) unless respond_to?(:parent)
        return NodeSet.new(document) unless parent

        parents = [parent]

        while parents.last.respond_to?(:parent)
          break unless (ctx_parent = parents.last.parent)

          parents << ctx_parent
        end

        return NodeSet.new(document, parents) unless selector

        root = parents.last
        search_results = root.search(selector)

        NodeSet.new(document, parents.find_all do |parent|
          search_results.include?(parent)
        end)
      end

      def traverse(&block)
        children.each { |j| j.traverse(&block) }
        yield(self)
      end

      def accept(visitor)
        visitor.visit(self)
      end

      def ==(other)
        return false unless other
        return false unless other.respond_to?(:pointer_id)

        pointer_id == other.pointer_id
      end

      def <=>(other)
        return unless other.is_a?(XML::Node)
        return unless document == other.document

        compare(other)
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

      DECONSTRUCT_KEYS = [:name, :attributes, :children, :namespace, :content, :elements, :inner_html].freeze # :nodoc:
      DECONSTRUCT_METHODS = {attributes: :attribute_nodes}.freeze # :nodoc:

      def deconstruct_keys(keys)
        requested_keys = DECONSTRUCT_KEYS & keys
        {}.tap do |values|
          requested_keys.each do |key|
            method = DECONSTRUCT_METHODS[key] || key
            values[key] = send(method)
          end
        end
      end

      protected

      # The nodes +data+ stands for: markup is parsed in the context of
      # this node.
      def coerce(data)
        case data
        when XML::NodeSet
          return data
        when XML::DocumentFragment
          return data.children
        when String
          return fragment(data).children
        when Document, XML::Attr
          # unacceptable
        when XML::Node
          return data
        end

        raise ArgumentError, <<~EOERR
          Requires a Node, NodeSet or String argument, and cannot accept a #{data.class}.
          (You probably want to select a node from the Document with at() or search(), or create a new Node via Node.new().)
        EOERR
      end

      private

      def keywordify(keywords)
        case keywords
        when Enumerable
          keywords
        when String
          keywords.scan(/\S+/)
        else
          raise ArgumentError,
            "Keyword attributes must be passed as either a String or an Enumerable, but received #{keywords.class}"
        end
      end

      def add_sibling(next_or_previous, node_or_tags)
        raise("Cannot add sibling to a node with no parent") unless parent

        impl = (next_or_previous == :next) ? :add_next_sibling_node : :add_previous_sibling_node
        iter = (next_or_previous == :next) ? :reverse_each : :each

        node_or_tags = parent.coerce(node_or_tags)
        if node_or_tags.is_a?(XML::NodeSet)
          if text?
            pivot = XML::Node.new("dummy", document)
            send(impl, pivot)
          else
            pivot = self
          end
          node_or_tags.send(iter) { |n| pivot.send(impl, n) }
          pivot.unlink if text?
        else
          send(impl, node_or_tags)
        end
        node_or_tags
      end

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

      IMPLIED_XPATH_CONTEXTS = [".//"].freeze

      def add_child_node_and_reparent_attrs(node)
        add_child_node(node)
        node.attribute_nodes.find_all { |a| a.name.include?(":") }.each do |attr_node|
          attr_node.remove
          node[attr_node.name] = attr_node.value
        end
      end
    end
  end
end

require_relative "node/save_options"
