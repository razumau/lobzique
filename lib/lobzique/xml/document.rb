# frozen_string_literal: true

# Adapted from Nokogiri 1.19.4, lib/nokogiri/xml/document.rb (MIT, see LICENSE-NOKOGIRI.md).

module Lobzique
  module XML
    class Document < Node
      # The xmlns keys #create_element treats as namespace definitions.
      NCNAME_START_CHAR = "A-Za-z_"
      NCNAME_CHAR = NCNAME_START_CHAR + "\\-\\.0-9"
      NCNAME_RE = /^xmlns(?::([#{NCNAME_START_CHAR}][#{NCNAME_CHAR}]*))?$/

      OBJECT_DUP_METHOD = Object.instance_method(:dup)
      OBJECT_CLONE_METHOD = Object.instance_method(:clone)
      private_constant :OBJECT_DUP_METHOD, :OBJECT_CLONE_METHOD

      attr_accessor :errors
      attr_accessor :namespace_inheritance

      # Every way of making a document runs this, parsing included, so
      # subclasses (Loofah's) can extend it with `super`.
      def initialize(*args) # rubocop:disable Lint/MissingSuper
        @errors = []
        @decorators = nil
        @namespace_inheritance = false
      end

      # A copy of the document, with its whole tree unless +level+ is 0.
      def dup(level = 1)
        copy = OBJECT_DUP_METHOD.bind_call(self)
        copy.initialize_copy_with_args(self, level)
      end

      def clone(level = 1)
        copy = OBJECT_CLONE_METHOD.bind_call(self)
        copy.initialize_copy_with_args(self, level)
      end

      # An element named +name+. Each argument is a Hash of attributes or
      # the element's content; the block gets the element first.
      def create_element(name, *contents_or_attrs, &block)
        elm = XML::Element.new(name, self, &block)
        contents_or_attrs.each do |arg|
          case arg
          when Hash
            arg.each do |k, v|
              key = k.to_s
              if NCNAME_RE.match?(key)
                # lobzique: Nokogiri adds a namespace definition here, and
                # makes an unprefixed one the element's namespace; adding
                # definitions is out of scope
                raise NotImplementedError, "namespace definitions can't be added"
              else
                elm[k.to_s] = v.to_s
              end
            end
          else
            elm.content = arg
          end
        end
        elm
      end

      def create_text_node(string, &block)
        Text.new(string.to_s, self, &block)
      end

      def create_cdata(string, &block)
        CDATA.new(self, string.to_s, &block)
      end

      def create_comment(string, &block)
        Comment.new(self, string.to_s, &block)
      end

      def name
        "document"
      end

      def document
        self
      end

      # The modules extended into every node and node set of this document,
      # by the class they apply to.
      def decorators(key)
        @decorators ||= {}
        @decorators[key] ||= []
      end

      def decorate(node)
        return unless @decorators

        @decorators.each do |klass, list|
          next unless node.is_a?(klass)

          list.each { |mod| node.extend(mod) }
        end
      end

      def namespaces
        root ? root.namespaces : {}
      end

      # lobzique: Nokogiri also undefines default_namespace=,
      # add_namespace_definition, line and add_namespace, which aren't
      # defined
      undef_method :swap, :parent, :namespace, :attributes, :namespace_definitions

      def add_child(node_or_tags)
        raise "A document may not have multiple root nodes." if root && root.name != "nokogiri_text_wrapper" && !(node_or_tags.comment? || node_or_tags.processing_instruction?)

        node_or_tags = coerce(node_or_tags)
        if node_or_tags.is_a?(XML::NodeSet)
          raise "A document may not have multiple root nodes." if node_or_tags.size > 1

          super(node_or_tags.first)
        else
          super
        end
      end
      alias_method :<<, :add_child

      def deconstruct_keys(keys)
        {root: root}
      end

      private

      def inspect_attributes
        [:name, :children]
      end
    end
  end
end
