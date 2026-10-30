# frozen_string_literal: true

require "test_helper"
require "nokogiri"
require "stringio"

# XML and XHTML output, libxml2's serializer behind native_write_to. Adapted
# from test/fixtures/nokogiri/xml/test_node.rb, test_cdata.rb and
# test_text.rb onto documents built with XML::Document.new, from
# html5/test_encoding.rb and test_nokogumbo.rb, and from premailer's calls
# to to_xhtml.
module SerializeContract
  HTML = "<html><head><title>t</title></head><body><p>Señor<br><input disabled>" \
    "<a name=n lang=en>x</a></p><div><span>a</span><span>b</span></div></body></html>"

  def test_write_to
    io = StringIO.new
    xml.write_to(io)
    assert_equal xml.to_xml, io.string
  end

  def test_write_to_with_block
    io = StringIO.new
    conf = nil
    xml.write_to(io) do |config|
      conf = config
      config.format.as_xml.no_empty_tags
    end
    assert_equal xml.serialize(nil, conf.options), io.string
    assert_equal xml.serialize(nil, conf), io.string
    assert_includes io.string, "<e></e>"
  end

  ["xml", "xhtml"].each do |type|
    define_method(:"test_write_#{type}_to") do
      io = StringIO.new
      assert xml.send(:"write_#{type}_to", io)
      assert_equal xml.send(:"to_#{type}"), io.string
    end
  end

  def test_xml_documents
    assert_equal <<~XML, xml.to_xml
      <?xml version="1.0"?>
      <root a="1 &amp; &quot;2&quot;&#10;">
        <ul>
          <li>Hello</li>
        </ul>
        <e/>
      </root>
    XML
    assert_equal xml.to_xml, xml.to_s
    assert_equal %(<?xml version="1.1"?>\n), root::XML::Document.new("1.1").to_xml
    assert_equal "1.1", root::XML::Document.new("1.1").dup.version
  end

  def test_to_xml_allows_to_serialize_with_as_xml_save_option
    node = xml.root.children.first
    refute_match("<ul>\n  <li>", xml.to_xml(save_with: root::XML::Node::SaveOptions::AS_XML))
    refute_match("<ul>\n  <li>", node.to_xml(save_with: root::XML::Node::SaveOptions::AS_XML))
  end

  def test_character_data
    doc = root::XML::Document.new
    assert_equal "&amp; &lt;foo&gt; &amp;amp;", doc.create_text_node("& <foo> &amp;").to_xml
    assert_equal "<![CDATA[& <foo> &amp;]]>", doc.create_cdata("& <foo> &amp;").to_xml
    assert_equal "<![CDATA[foo ]]]]><![CDATA[> bar]]>", doc.create_cdata("foo ]]> bar").to_xml
    comment = doc.create_comment("")
    comment.native_content = " < "
    assert_equal "<!-- < -->", comment.to_xml
  end

  def test_encodings
    text = root::XML::Document.new.create_text_node("é\r")
    assert_equal "&#xE9;&#xD;", text.to_xml
    assert_equal "é&#13;", text.to_xml(encoding: "UTF-8")
    assert_equal "&#233;&#13;", text.to_xml(encoding: "US-ASCII")
    assert_equal Encoding::US_ASCII, text.to_xml(encoding: "US-ASCII").encoding
    assert_equal "\xE9&#13;".b, text.to_xml(encoding: "ISO-8859-1").b

    io = StringIO.new
    text.write_to(io, encoding: "bogus")
    assert_empty io.string
  end

  def test_html5_encoding
    iso8859 = (+"<span>Se\xF1or</span>").force_encoding(Encoding::ASCII_8BIT)
    assert_equal "<span>Señor</span>", root::HTML5(iso8859).at("span").to_xml
    utf8 = (+"<meta charset='utf-8'><span>Se\xC3\xB1or</span>").force_encoding(Encoding::ASCII_8BIT)
    assert_equal "<span>Señor</span>", root::HTML5(utf8).at("span").to_xml
  end

  def test_fragments_and_node_sets
    fragment = root::HTML5.fragment("<main><span>content</span></main><br>")
    assert_equal "<span>content</span>", fragment.xpath("main/span").to_xml
    assert_equal "<main>\n  <span>content</span>\n</main><br/>", fragment.to_xml
    assert_equal "<main>\n  <span>content</span>\n</main><br />", fragment.to_xhtml
  end

  # What premailer writes with its xhtml_output option
  def test_html5_documents_as_xhtml
    doc = root::HTML5(HTML)
    assert_equal <<~XHTML, doc.to_xhtml(encoding: nil)
      <html xmlns="http://www.w3.org/1999/xhtml">
        <head>
          <meta http-equiv="Content-Type" content="text/html; charset=UTF-8" />
          <title>t</title>
        </head>
        <body>
          <p>Señor<br /><input disabled="disabled" /><a name="n" lang="en" id="n" xml:lang="en">x</a></p>
          <div>
            <span>a</span>
            <span>b</span>
          </div>
        </body>
      </html>
    XHTML
    assert_includes doc.to_xhtml(encoding: "US-ASCII"), "<p>Se&#241;or<br />"
    assert_includes doc.to_xhtml(encoding: "US-ASCII"), "charset=US-ASCII"
  end

  def test_a_doctype_other_than_xhtml_keeps_xml_rules
    doc = root::HTML5("<!DOCTYPE html><p>x<br><input disabled></p>x<div></div>")
    assert_includes doc.to_xhtml, %(<p>x<br/><input disabled="disabled"/></p>x<div/>)
    assert_includes doc.at("body").to_xhtml, %(<p>x<br /><input disabled="disabled" /></p>x<div></div>)
  end

  def test_html5_documents_as_xml
    doc = root::HTML5("<p>a</p>")
    assert_equal <<~XML, doc.to_xml
      <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
      <html>
        <head/>
        <body>
          <p>a</p>
        </body>
      </html>
    XML
  end

  def test_a_write_that_raises_ends_the_output_quietly
    io = Object.new
    def io.write(_) = raise(IOError, "closed stream")
    assert_same io, root::HTML5("<p>").at("p").write_xml_to(io)
  end

  def xml
    @xml ||= root::XML::Document.new.tap do |doc|
      doc.root = doc.create_element("root", "a" => "1 & \"2\"\n")
      list = doc.create_element("ul")
      list << doc.create_element("li", "Hello")
      doc.root << list << doc.create_element("e")
    end
  end
end

class TestNodeSerializeNokogiri < Minitest::Test
  include SerializeContract

  def root = Nokogiri
end

class TestNodeSerializeLobzique < Minitest::Test
  include SerializeContract

  def root = Lobzique

  # A departure: libxml2's HTML serializer isn't ported (ARCHITECTURE.md)
  def test_xml_documents_have_no_html_output
    assert_raises(NotImplementedError) { xml.root.to_html }
  end
end
