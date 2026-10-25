# Rust-backed HTML gem to take libxml2 out of Rails' default path

## Goal

Every Rails app ships libxml2 (C) via Nokogiri, and libxml2 is a steady source
of CVEs. Since 2025 its sole maintainer treats security reports as ordinary
public bugs and the README calls it foolish to use on untrusted data.
Sanitizing user HTML is exactly that use. Build a Rust-based gem covering only what Rails needs.

## Rails' dependency is HTML, not XML

- `rails-html-sanitizer` -> Loofah -> Nokogiri: `sanitize`, `strip_tags`,
  Action Text sanitizing. This is what forces Nokogiri into every lockfile
  (1.7.0, Feb 2026, still hard-depends on loofah ~> 2.25 and nokogiri).
- `rails-dom-testing`: `assert_select` / CSS matching (test-time, actionpack).
- Action Text: `ActionText::Fragment`, `html_conversion`, `markdown_conversion`
  find, replace and serialize nodes.
- actionpack `TestResponse#parsed_body` returns a Nokogiri doc for HTML.
- ActiveSupport `XmlMini` defaults to REXML (pure Ruby); Nokogiri is optional.
- Nokogiri's HTML5 parser (gumbo) still builds a libxml2 tree, so the HTML5
  path does not avoid libxml2.

## Scope

In: HTML5 parsing of documents and fragments (html5ever); arena DOM with
traversal and mutation; CSS selectors (Servo `selectors`); a serializer that
matches Nokogiri::HTML5 byte for byte; a sanitizer with Rails' Full / Link /
SafeList semantics.

Out (for now): XPath, XSLT, XSD/RelaxNG/DTD, SAX, the HTML4 quirks parser,
namespace-heavy XML, and any XmlMini backend.

This should be a drop-in replacement for Nokogiri. We will test by patching existing Rails app to use this gem, and that patching should be minimal.

## Testing 

HTML (and XML) parsing has a lot of existing tests, specs, and data. We should use them instead of creating synthetic data for anything larger than a unit test for a method.

## Phases

0. Prepare test fixtures.
1. Inventory the Nokogiri API used by Rails, Loofah, rails-dom-testing and the
   top ~50 gems; that list is the API contract.
2. Core crate + gem: parse, DOM, select, serialize. Differential-test against
   Nokogiri::HTML5 on real HTML (output must match: apps have stored Action
   Text content and snapshot tests).
3. Sanitizer vendor. Seam already exists: rails-html-sanitizer vendor classes,
   `best_supported_vendor`, `config.action_view.sanitizer_vendor`.
4. Action Text + assert_select: actiontext and actionview suites green.
