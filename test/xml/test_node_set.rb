# frozen_string_literal: true

require "test_helper"
require "nokogiri"

# NodeSet's collection behavior, which lobzique implements in Ruby where
# Nokogiri uses libxml2's xmlNodeSet. Runs against both.
module NodeSetContract
  def setup
    @doc = root::HTML5::Document.new
    @a, @b, @c = %w[a b c].map { |name| root::XML::Node.new(name, @doc) }
  end

  def set(*nodes) = root::XML::NodeSet.new(@doc, nodes)

  def names(node_set) = node_set.map(&:name)

  def test_push_ignores_duplicates
    node_set = set(@a, @b, @a)
    node_set << @b
    assert_equal %w[a b], names(node_set)
  end

  def test_push_rejects_other_objects
    error = assert_raises(ArgumentError) { set << "a" }
    assert_equal "node must be a #{root}::XML::Node or #{root}::XML::Namespace", error.message
  end

  def test_indexing
    node_set = set(@a, @b, @c)
    assert_same @c, node_set[-1]
    assert_nil node_set[3]
    assert_equal %w[b c], names(node_set[1..])
    assert_equal %w[a b], names(node_set.slice(0, 2))
    assert_instance_of root::XML::NodeSet, node_set[1, 5]
    assert_equal [], names(node_set[3, 1])
    assert_nil node_set[4, 1]
    assert_equal %w[a b], names(node_set.first(2))
  end

  def test_set_operations
    left = set(@a, @b)
    right = set(@b, @c)
    assert_equal %w[a b c], names(left | right)
    assert_equal %w[a b c], names(left + right)
    assert_equal %w[b], names(left & right)
    assert_equal %w[a], names(left - right)
    assert_raises(ArgumentError) { left | [@c] }
  end

  def test_delete_pop_shift
    node_set = set(@a, @b, @c)
    assert_same @b, node_set.delete(@b)
    assert_nil node_set.delete(@b)
    assert_same @c, node_set.pop
    assert_same @a, node_set.shift
    assert_empty node_set
    assert_nil node_set.pop
  end

  def test_enumerable
    node_set = set(@a, @b, @c)
    assert_equal 1, node_set.index(@b)
    assert_equal 2, node_set.index { |node| node.name == "c" }
    assert_equal %w[c b a], names(node_set.reverse)
    assert_equal [@a, @b, @c], node_set.to_a
    assert_equal [@a, @b, @c], node_set.to_ary
    assert_equal 3, node_set.size
    assert_equal set(@a, @b, @c), node_set
    refute_equal set(@a, @c), node_set
    assert_includes node_set, @a
  end

  def test_dup_is_a_separate_decorated_set
    @doc.decorators(root::XML::NodeSet) << (marker = Module.new)
    node_set = set(@a)
    copy = node_set.dup
    copy << @b
    assert_equal %w[a], names(node_set)
    assert_kind_of marker, copy
  end
end

class TestNodeSetNokogiri < Minitest::Test
  include NodeSetContract

  def root = Nokogiri
end

class TestNodeSetLobzique < Minitest::Test
  include NodeSetContract

  def root = Lobzique

  def test_wrap_decorates_without_initialize
    @doc.decorators(Lobzique::XML::NodeSet) << (marker = Module.new)
    initialized = false
    node_set_class = Class.new(Lobzique::XML::NodeSet) do
      define_method(:initialize) { |*| initialized = true }
    end
    node_set = node_set_class.wrap(@doc, [@c, @a])
    assert_kind_of marker, node_set
    assert_same @doc, node_set.document
    assert_equal %w[c a], names(node_set)
    refute initialized
  end
end
