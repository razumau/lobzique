# frozen_string_literal: true

# Adapted from Nokogiri 1.19.4, lib/nokogiri/xml/node_set.rb and the native
# methods of ext/nokogiri/xml_node_set.c (MIT, see LICENSE-NOKOGIRI.md).

module Lobzique
  module XML
    class NodeSet
      include Searchable
      include Enumerable

      attr_accessor :document

      # lobzique: Nokogiri implements the methods from here to #initialize in
      # C over libxml2's xmlNodeSet. Here they work on an Array, which Ruby's
      # GC already marks.

      # Nokogiri's C noko_xml_node_set_wrap: a decorated set of +nodes+,
      # which must be distinct, without #initialize or a duplicate check per
      # node. Native search results come through here.
      def self.wrap(document, nodes) # :nodoc:
        allocate.send(:adopt, document, nodes)
      end

      def length
        @nodes.length
      end

      def push(node)
        check_member(node)
        @nodes << node unless @nodes.any? { |member| member.equal?(node) }
        self
      end

      def delete(node)
        check_member(node)
        index = @nodes.index { |member| member.equal?(node) }
        @nodes.delete_at(index) if index
      end

      def include?(node)
        check_member(node)
        @nodes.any? { |member| member.equal?(node) }
      end

      def [](*args)
        result = @nodes[*args]
        result.is_a?(Array) ? NodeSet.wrap(document, result) : result
      end
      alias_method :slice, :[]

      def to_a
        @nodes.dup
      end

      def &(other)
        check_node_set(other)
        members = identity_set(other.to_a)
        NodeSet.wrap(document, @nodes.select { |node| members.key?(node) })
      end

      def -(other)
        check_node_set(other)
        members = identity_set(other.to_a)
        NodeSet.wrap(document, @nodes.reject { |node| members.key?(node) })
      end

      def |(other)
        check_node_set(other)
        members = identity_set(@nodes)
        NodeSet.wrap(document, @nodes + other.to_a.reject { |node| members.key?(node) })
      end

      def initialize_copy(other)
        super
        @nodes = other.to_a
        document&.decorate(self)
      end

      def initialize(document, list = [])
        @nodes = [] # lobzique: what libxml2's xmlNodeSet is in Nokogiri
        @document = document
        document.decorate(self)
        list.each { |x| self << x }
        yield self if block_given?
      end

      def first(n = nil)
        return self[0] unless n

        list = []
        [n, length].min.times { |i| list << self[i] }
        list
      end

      def last
        self[-1]
      end

      def empty?
        length == 0
      end

      def index(node = nil)
        if node
          warn("given block not used") if block_given?
          each_with_index { |member, j| return j if member == node }
        elsif block_given?
          each_with_index { |member, j| return j if yield(member) }
        end
        nil
      end

      alias_method :<<, :push

      def each
        return to_enum unless block_given?

        0.upto(length - 1) do |x|
          yield self[x]
        end
        self
      end

      alias_method :size, :length
      alias_method :to_ary, :to_a

      def pop
        return if length == 0

        delete(last)
      end

      def shift
        return if length == 0

        delete(first)
      end

      def ==(other)
        return false unless other.is_a?(XML::NodeSet)
        return false unless length == other.length

        each_with_index do |node, i|
          return false unless node == other[i]
        end
        true
      end

      def children
        node_set = NodeSet.new(document)
        each do |node|
          node.children.each { |n| node_set.push(n) }
        end
        node_set
      end

      def reverse
        node_set = NodeSet.new(document)
        (length - 1).downto(0) do |x|
          node_set.push(self[x])
        end
        node_set
      end

      def inspect
        "[#{map(&:inspect).join(", ")}]"
      end

      alias_method :+, :|

      def inner_text
        collect(&:inner_text).join("")
      end
      alias_method :text, :inner_text

      def inner_html(*args)
        collect { |j| j.inner_html(*args) }.join("")
      end

      def to_s
        map(&:to_s).join # rubocop:disable Style/MapJoin -- a NodeSet has no #join
      end

      def to_html(*args)
        if empty?
          encoding = (args.first.is_a?(Hash) ? args.first[:encoding] : nil)
          encoding ||= document.encoding
          encoding.nil? ? "" : "".encode(encoding)
        else
          map { |x| x.to_html(*args) }.join
        end
      end

      def deconstruct
        to_a
      end

      private

      def adopt(document, nodes)
        @nodes = nodes
        @document = document
        document&.decorate(self)
        self
      end

      def identity_set(nodes)
        nodes.each_with_object({}.compare_by_identity) { |node, set| set[node] = true }
      end

      def check_member(node)
        return if node.is_a?(XML::Node) || node.is_a?(XML::Namespace)

        raise ArgumentError, "node must be a #{XML::Node} or #{XML::Namespace}"
      end

      def check_node_set(other)
        raise ArgumentError, "node_set must be a #{XML::NodeSet}" unless other.is_a?(XML::NodeSet)
      end
    end
  end
end
