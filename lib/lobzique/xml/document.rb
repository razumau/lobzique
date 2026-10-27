# frozen_string_literal: true

# Adapted from Nokogiri 1.19.4, lib/nokogiri/xml/document.rb (MIT, see LICENSE-NOKOGIRI.md).

module Lobzique
  module XML
    class Document < Node
      attr_accessor :errors
      attr_accessor :namespace_inheritance

      # Every way of making a document runs this, parsing included, so
      # subclasses (Loofah's) can extend it with `super`.
      def initialize(*args) # rubocop:disable Lint/MissingSuper
        @errors = []
        @decorators = nil
        @namespace_inheritance = false
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

      # lobzique: Nokogiri also undefines swap, default_namespace=,
      # add_namespace_definition, line and add_namespace, which aren't
      # defined yet
      undef_method :parent, :namespace, :attributes, :namespace_definitions

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
