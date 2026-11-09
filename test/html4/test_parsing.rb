# frozen_string_literal: true

require "test_helper"
require "nokogiri"
require "stringio"
require "tempfile"

# The HTML4 names (plan.md decision 3): HTML5's tree, libxml2's encodings.
# The trees differ from Nokogiri's, so the shared tests compare what the
# encoding decides: document.encoding and the decoded text.
module HTML4ParsingContract
  # [input, given encoding] => [document.encoding, text of the first <p>]
  ENCODINGS = {
    ["<p>café</p>", nil] => ["UTF-8", "café"],
    ["<p>hi</p>".b, nil] => [nil, "hi"],
    # libxml2 reads undeclared non-ASCII input as ISO-8859-1, UTF-8 too
    ["<p>café</p>".b, nil] => ["ISO-8859-1", "cafÃ©"],
    ["<p>caf\xE9</p>".b, nil] => ["ISO-8859-1", "café"],
    ["<meta charset=iso-8859-1><p>caf\xE9</p>".b, nil] => ["ISO-8859-1", "café"],
    [%(<meta http-equiv="Content-Type" content="text/html; charset=Shift_JIS"><p>\x93\xFA</p>).b, nil] => ["Shift_JIS", "日"],
    # the last <meta> wins
    [%(<meta charset=utf-8><meta charset=iso-8859-1><p>caf\xE9</p>).b, nil] => ["ISO-8859-1", "café"],
    [%(<title>é</title><meta charset=koi8-r><p>\xC1</p>).b, nil] => ["koi8-r", "а"],
    [%(<meta charset="koi8-r"><p>\xC1</p>).b, nil] => ["koi8-r", "а"],
    [%(<meta charset=windows-1252><p>\x80</p>).b, nil] => ["windows-1252", "€"],
    [%(<meta http-equiv=content-type content="text/html;charset=UTF8"><p>café</p>).b, nil] => ["UTF-8", "café"],
    # neither comments nor scripts declare, but libxml2's own search of the
    # raw bytes finds http-equiv anywhere after the first non-ASCII byte
    [%(<!-- <meta charset=iso-8859-1> --><p>café</p>).b, nil] => ["ISO-8859-1", "cafÃ©"],
    [%(<script>x="<meta charset=koi8-r>"</script><p>\xC1</p>).b, nil] => ["ISO-8859-1", "Á"],
    [%(<p>caf\xE9 HTTP-EQUIV="Content-Type" CONTENT="text/html; charset=windows-1251"</p>).b, nil] =>
      ["windows-1251", %(cafй HTTP-EQUIV="Content-Type" CONTENT="text/html; charset=windows-1251")],
    [%(<?xml version="1.0" encoding="iso-8859-1"?><p>caf\xE9</p>).b, nil] => ["ISO-8859-1", "café"],
    ["\xEF\xBB\xBF<p>café</p>".b, nil] => [nil, "café"],
    ["﻿<p>café</p>".encode("UTF-16LE").b, nil] => [nil, "café"],
    ["<p>日</p>".encode("Shift_JIS"), nil] => ["Shift_JIS", "日"],
    # a given name is kept as written, unless a built-in decoder takes it
    ["<p>café</p>".b, "UTF8"] => ["UTF-8", "café"],
    ["<p>caf\xE9</p>".b, "iso-8859-1"] => ["ISO-8859-1", "café"],
    ["<p>caf\xE9</p>".b, "ISO_8859-1"] => ["ISO_8859-1", "café"],
    ["<p>caf\xE9</p>".b, "latin1"] => ["latin1", "café"],
    ["<p>caf\xE9</p>".b, "bogus"] => ["ISO-8859-1", "café"],
    [%(<meta charset=x-bogus><p>caf\xE9</p>).b, nil] => ["ISO-8859-1", "café"],
    ["<p>café 日</p>", "iso-8859-1"] => ["ISO-8859-1", "cafÃ© æ\u0097¥"],
    # names only Ruby has, and ones no input's bytes can spell
    ["<p>caf\xE9</p>".b, "internal"] => ["ISO-8859-1", "café"],
    ["<p>caf\xE9</p>".b, "locale"] => ["ISO-8859-1", "café"],
    ["<p>caf\xE9</p>".b, "binary"] => ["ISO-8859-1", "café"],
    ["<p>café</p>".b, "CP65001"] => ["ISO-8859-1", "cafÃ©"],
    [%(<meta charset=internal><p>caf\xE9</p>).b, nil] => ["ISO-8859-1", "café"],
    [%(<meta charset="&#x65e5;"><p>caf\xE9</p>).b, nil] => ["ISO-8859-1", "café"],
    [%(<meta charset="&#x65e5;"><p>x</p>).b, nil] => [nil, "x"]
  }.freeze

  def test_encodings
    ENCODINGS.each do |(input, encoding), expected|
      doc = root::HTML4(input, nil, encoding)
      assert_equal expected, [doc.encoding, doc.at_css("p")&.text], "#{input.inspect} with #{encoding.inspect}"
    end
  end

  def test_io
    {
      "<meta charset=iso-8859-1><p>caf\xE9</p>".b => ["ISO-8859-1", "café"],
      "<p>caf\xE9</p>".b => ["ISO-8859-1", "café"],
      "<p>café</p>" => ["ISO-8859-1", "cafÃ©"],
      ("<b>x</b>" * 300 + "<meta charset=iso-8859-1><p>caf\xE9</p>").b => ["ISO-8859-1", "café"]
    }.each do |input, expected|
      doc = root::HTML4(StringIO.new(input))
      assert_equal expected, [doc.encoding, doc.at_css("p").text], input.inspect
    end
  end

  def test_files_and_urls
    Tempfile.create(["page", ".html"]) do |file|
      file.write("<title>T</title><p>caf\xE9</p>".b)
      file.flush
      doc = root::HTML4::Document.parse(Pathname.new(file.path))
      assert_equal [file.path, "ISO-8859-1", "T"], [doc.url, doc.encoding, doc.title]
    end
    assert_equal "http://example.com/", root::HTML("<p>x", "http://example.com/").url
  end

  def test_empty_input
    assert_equal "UTF-8", root::HTML4("").encoding
    assert_nil root::HTML4("".b).encoding
    assert_nil root::HTML4(nil).encoding
    assert_equal "iso-8859-1", root::HTML4("", nil, "iso-8859-1").encoding
  end

  def test_errors_for_encodings
    doc = root::HTML4("<p>caf\xE9</p>".b, nil, "bogus")
    error = doc.errors.first
    assert_equal [1, 32, 1, "bogus", "WARNING: Unsupported encoding: bogus"], [error.domain, error.code, error.level, error.str1, error.to_s]

    doc = root::HTML4("<p>caf\xE9 x</p>".b, nil, "UTF-8")
    error = doc.errors.first
    assert_equal [8, 81, 3, "1:7: FATAL: Invalid bytes in character encoding"], [error.domain, error.code, error.level, error.to_s]
  end

  def test_classes_and_entry_points
    assert_instance_of root::HTML4::Document, root::HTML("<p>x")
    assert_instance_of root::HTML4::Document, root::HTML4.parse("<p>x")
    assert_instance_of root::HTML4::DocumentFragment, root::HTML4.fragment("<p>x")
    doc = root::HTML4("<p>x")
    assert_instance_of root::HTML4::DocumentFragment, doc.fragment("<b>y</b>")
    assert_instance_of root::HTML4::DocumentFragment, doc.at_css("p").fragment("<b>y</b>")
    assert_equal "<b>y</b>", doc.fragment("<b>y</b>").to_html
  end

  def test_fragment_encoding
    assert_equal "UTF-8", root::HTML4.fragment("<p>café</p>".b).document.encoding
    assert_equal "café", root::HTML4.fragment("<p>café</p>".b).text
    assert_equal "Shift_JIS", root::HTML4.fragment("<p>日</p>".encode("Shift_JIS")).document.encoding
    assert_equal "日", root::HTML4.fragment("<p>日</p>".encode("Shift_JIS")).text
    assert_equal "café", root::HTML4::Document.new.fragment("<p>café</p>").text
  end

  def test_title_and_meta_encoding
    doc = root::HTML4("<html><head><meta charset=koi8-r><title>T</title></head><body><p>x</p></body></html>")
    assert_equal ["T", "koi8-r"], [doc.title, doc.meta_encoding]
    doc.title = "New"
    doc.meta_encoding = "UTF-8"
    assert_equal ["New", "UTF-8"], [doc.title, doc.meta_encoding]

    doc = root::HTML4(%(<html><head><meta http-equiv="Content-Type" content="text/html; charset=EUC-JP"></head></html>))
    assert_equal "EUC-JP", doc.meta_encoding
  end

  def test_serializes_into_the_documents_encoding
    html = root::HTML4(%(<meta charset=iso-8859-1><p>caf\xE9</p>).b).at_css("p").to_html
    assert_equal ["<p>caf\xE9</p>".b, Encoding::ISO_8859_1], [html.b, html.encoding]
  end
end

class TestHTML4ParsingNokogiri < Minitest::Test
  include HTML4ParsingContract

  def root = Nokogiri
end

class TestHTML4ParsingLobzique < Minitest::Test
  include HTML4ParsingContract

  def root = Lobzique

  # What libxml2 does differently, with the reason.

  # docs/html4-invalid-bytes.md: libxml2 keeps the raw byte in text
  def test_invalid_bytes_in_a_declared_encoding
    doc = Lobzique::HTML4("<p>caf\xE9 x</p><p>y</p>".b, nil, "UTF-8")
    assert_equal ["UTF-8", "caf� x"], [doc.encoding, doc.at_css("p").text]
    doc = Lobzique::HTML4("<p>\x93\xFA \x81</p><p>after</p>".b, nil, "Shift_JIS")
    assert_equal ["日 �", "after"], doc.css("p").map(&:text)
  end

  # libiconv decodes EUC-TW, and stops at a byte invalid in ISO-2022-JP
  def test_encodings_ruby_cannot_decode
    doc = Lobzique::HTML4("<p>caf\xE9</p>".b, nil, "EUC-TW")
    assert_equal ["ISO-8859-1", "café", "WARNING: Unsupported encoding: EUC-TW"], [doc.encoding, doc.at_css("p").text, doc.errors.first.message]
    doc = Lobzique::HTML4("<p>\e$BF|\xFF\xFF\e(B x</p>".b, nil, "ISO-2022-JP")
    assert_equal ["ISO-2022-JP", "日\uFFFD\uFFFD x"], [doc.encoding, doc.at_css("p").text]
  end

  # html5ever's tree: libxml2 has no <head> here and keeps the fragment's
  # leading <td>
  def test_html5_trees
    assert_equal "<html><head></head><body><p>x</p></body></html>", Lobzique::HTML4("<p>x").to_html
    assert_equal "a<p>b</p>", Lobzique::HTML4.fragment("<td>a</td><p>b</p>").to_html
  end
end
