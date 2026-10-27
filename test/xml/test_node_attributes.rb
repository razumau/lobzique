# frozen_string_literal: true

require "test_helper"
require "nokogiri"

# Reading attributes. libxml2 looks names up differently in each reader:
# `[]` takes `prefix:name` when the prefix is in scope and otherwise
# matches no-namespace attributes only, while `key?` and `attribute` match a
# local name in any namespace.
module NodeAttributesContract
  def test_reading_values
    node = element(%(<p id=a class="x  y" data-empty>))
    assert_equal "a", node["id"]
    assert_equal "a", node[:id]
    assert_equal "a", node.get_attribute("id")
    assert_equal "a", node.attr("id")
    assert_equal "", node["data-empty"]
    assert_nil node["missing"]
    assert_nil node[nil]
    assert_equal Encoding::UTF_8, node["id"].encoding
  end

  def test_listing_attributes
    node = element(%(<p id=a class="x  y" data-empty>))
    assert_equal ["id", "class", "data-empty"], node.keys
    assert_equal ["a", "x  y", ""], node.values
    assert_equal [["id", "a"], ["class", "x  y"], ["data-empty", ""]], node.to_a
    assert node.value?("a")
    refute node.value?("x")
    attributes = node.attributes
    assert_equal ["id", "class", "data-empty"], attributes.keys
    assert_instance_of root::XML::Attr, attributes["id"]
    assert_equal "a", attributes["id"].value
  end

  def test_presence
    node = element(%(<p id=a>))
    assert node.key?("id")
    assert node.has_attribute?("id")
    refute node.key?("missing")
    assert_equal "id", node.attribute("id").name
    assert_same node.attribute("id"), node.attribute_nodes.first
    assert_nil node.attribute("missing")
    assert node.namespaced_key?("id", nil)
    assert_equal "id", node.attribute_with_ns("id", nil).name
    assert_raises(TypeError) { node.key?(:id) }
  end

  def test_namespaced_attributes
    doc = root::HTML5(%(<svg><a xlink:href=h xml:lang=it></a></svg>))
    node = doc.root.children.last.child.child
    xlink = "http://www.w3.org/1999/xlink"
    assert_equal "h", node["xlink:href"]
    assert_equal "it", node["xml:lang"]
    assert_nil node["href"]
    assert node.key?("href")
    assert_equal "href", node.attribute("href").name
    assert node.namespaced_key?("href", xlink)
    refute node.namespaced_key?("href", nil)
    assert_equal "href", node.attribute_with_ns("href", xlink).name
    assert_nil node.attribute_with_ns("href", nil)
    assert_equal ["href", "lang"], node.keys
  end

  # On an HTML element these are plain attributes with a colon in the name
  def test_prefixed_names_on_html_elements
    node = element(%(<p xml:lang=fr xlink:href=h foo:bar=baz>))
    assert_nil node["xml:lang"]
    assert node.key?("xml:lang")
    assert_equal "h", node["xlink:href"]
    assert_equal "baz", node["foo:bar"]

    # Once a foreign element defines xlink, the prefix resolves instead
    doc = root::HTML5(%(<p xlink:href=h></p><svg><a xlink:href=x /></svg>))
    assert_nil doc.root.children.last.child["xlink:href"]
  end

  def test_attributes_of_other_nodes
    doc = root::HTML5("<!DOCTYPE html><p>t")
    text = doc.root.children.last.child.child
    assert_nil text["id"]
    refute text.key?("id")
    assert_empty text.keys
    assert_nil doc.internal_subset["name"]
  end

  def test_classes
    assert_equal ["x", "y"], element(%(<p class=" x  y\t">)).classes
    assert_equal [], element("<p>").classes
    assert_equal ["a", "b"], element(%(<p rel="a b">)).kwattr_values("rel")
  end

  private

  def element(html)
    root::HTML5(html).root.children.last.child
  end
end

class TestNodeAttributesNokogiri < Minitest::Test
  include NodeAttributesContract

  def root = Nokogiri
end

class TestNodeAttributesLobzique < Minitest::Test
  include NodeAttributesContract

  def root = Lobzique
end
