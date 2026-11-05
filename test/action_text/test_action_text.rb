# frozen_string_literal: true

require "test_helper"
require "nokogiri"
require_relative "action_text_helper"

# Action Text with its document classes from each root. Inputs and outputs
# from test/fixtures/rails/actiontext/test/unit/content_test.rb, for the
# cases that need no database or controller.
module ActionTextContract
  def setup
    super
    ActionTextHelper.use(root)
  end

  def test_parses_with_the_roots_classes
    content = ActionText::Content.new("<div>Hello</div>")
    assert_instance_of root::HTML5::DocumentFragment, content.fragment.source
    assert_instance_of root::XML::Element, ActionText::HtmlConversion.create_element("p", "class" => "x")
  end

  def test_wraps_a_fragment_without_parsing_it_again
    source = root::HTML5::DocumentFragment.parse("<p>a</p>")
    assert_same source, ActionText::Fragment.wrap(source).source
  end

  def test_roundtrips_html_without_additional_newlines
    html = "<div>a<br></div>"
    assert_equal html, ActionText::Content.new(html).to_html
  end

  def test_extracts_links
    content = ActionText::Content.new('<a href="http://example.com/1">1</a><br><a href="http://example.com/1">1</a>')
    assert_equal ["http://example.com/1"], content.links
  end

  def test_converts_trix_formatted_attachments
    html = %(<figure data-trix-attachment='{"sgid":"123","contentType":"text/plain","width":100,"height":100}' data-trix-attributes='{"caption":"Captioned"}'></figure>)
    assert_equal '<action-text-attachment sgid="123" content-type="text/plain" width="100" height="100" caption="Captioned"></action-text-attachment>',
      ActionText::Content.new(html).to_html
  end

  def test_ignores_trix_formatted_attachments_with_malformed_json
    html = %(<div data-trix-attachment='{"sgid":"garbage...'></div>)
    assert_empty ActionText::Content.new(html).attachments
  end

  def test_minifies_attachment_markup
    html = '<action-text-attachment sgid="123"><div>HTML</div></action-text-attachment>'
    assert_equal '<action-text-attachment sgid="123"></action-text-attachment>', ActionText::Content.new(html).to_html
  end

  def test_canonicalizes_attachment_gallery_markup
    attachments = '<action-text-attachment sgid="1" presentation="gallery"></action-text-attachment>' \
      '<action-text-attachment sgid="2" presentation="gallery"></action-text-attachment>'
    content = ActionText::Content.new("<div class=\"attachment-gallery\">#{attachments}</div>")
    assert_equal "<div>#{attachments}</div>", content.to_html
    assert_equal 1, content.attachment_galleries.size
  end

  def test_removes_content_attribute_if_empty_after_sanitization
    html = '<action-text-attachment sgid="123" content="<script></script>"></action-text-attachment>'
    assert_equal '<figure data-trix-attachment="{&quot;sgid&quot;:&quot;123&quot;}"></figure>',
      ActionText::Content.new(html).to_trix_html.strip
  end

  def test_sanitizes_attachment_markup_for_trix
    html = '<action-text-attachment content="<img src=\&quot;.\&quot; onerror=alert>"></action-text-attachment>'
    assert_equal '<figure data-trix-attachment="{&quot;content&quot;:&quot;<img src=\\&quot;\\\\%22.\\\\%22\\&quot;>&quot;}"></figure>',
      ActionText::Content.new(html).to_trix_html.strip
  end

  def test_converts_to_plain_text
    html = "<h1>Title</h1><ul><li>one<ol><li>two</li></ol></li></ul><blockquote>quote</blockquote>" \
      '<action-text-attachment content-type="image/png" url="http://example.com/a.png" caption="A"></action-text-attachment>'
    assert_equal "Title\n\n• one\n  1. two\n\n“quote”\n\n[A]", ActionText::Content.new(html).to_plain_text
  end

  def test_replace_certain_nodes
    content = ActionText::Content.new("<div>\n  <p>replace me</p>\n  <p>ignore me</p>\n</div>\n")
    replaced = content.fragment.replace("p") { |node| (node.text =~ /replace me/) ? "<p>replaced</p>" : node }
    assert_equal "<div>\n  <p>replaced</p>\n  <p>ignore me</p>\n</div>", replaced.to_html
  end

  def test_delegates_pattern_matching
    content = ActionText::Content.new(%(<h1 id="hello-world">Hello, world</h1>\n\n<div>The body</div>\n))
    content => [h1, div]

    assert_pattern { h1 => {name: "h1", content: "Hello, world", attributes: [{name: "id", value: "hello-world"}]} }
    assert_pattern { div => {content: "The body"} }
  end
end

class TestActionTextNokogiri < Minitest::Test
  include ActionTextContract

  def root = Nokogiri
end

class TestActionTextLobzique < Minitest::Test
  include ActionTextContract

  def root = Lobzique
end
