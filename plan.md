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
   top ~50 gems; that list is the API contract. Done: `inventory/contract.yml`
   (138 entries in scope), method and findings in `inventory/README.md`.
   Tier 2 suites still have to run under the recorder.
2. Core crate + gem: parse, DOM, select, serialize. Differential-test against
   Nokogiri::HTML5 on real HTML (output must match: apps have stored Action
   Text content and snapshot tests).
3. Sanitizer vendor. Seam already exists: rails-html-sanitizer vendor classes,
   `best_supported_vendor`, `config.action_view.sanitizer_vendor`.
4. Action Text + assert_select: actiontext and actionview suites green.

## Open decisions from the inventory

Evidence is in `inventory/README.md` and `inventory/summary.md`.

1. **Gem name and namespace.** actionpack, actiontext, loofah,
   rails-html-sanitizer and rails-dom-testing all hard-depend on `nokogiri`.
   In 15 of 19 public apps, another runtime gem needs it as well (ruby-saml
   7, sanitize 6, inline_svg 5, reverse_markdown 5, fog-xml 4). There is one
   `Nokogiri` constant per process, so a shim that *is* `nokogiri` can only
   replace it in apps where no XML gem remains. Options:
   - A: ship as `nokogiri` from git/path, patched in with one Gemfile line.
   - B: own namespace, plugged into Rails seams (sanitizer vendor; patches
     for Action Text and rails-dom-testing). libxml2 stays installed but
     never sees Rails' HTML.
   - C: core under `Lobzique`, with an optional `Nokogiri` facade for apps
     that can drop nokogiri.

   Recommendation: C. Phase 3 then gives the sanitizing win to every app,
   and the facade covers Lobsters/Fizzy/Campfire/DocuSeal-style apps.
2. **XPath.** Runtime HTML code uses about ten fixed expressions:
   - Loofah: `/html/body`, `./body`, `./form`, `//p[br[following-sibling::br]]`
   - html-pipeline: `.//text()`
   - inline_svg: `//comment()`
   - roadie: `html/head/meta`
   - reverse_markdown: `preceding::text()[1]`

   `Rails::HTML::Sanitizer#remove_xpaths` takes arbitrary XPath, and
   Capybara rack_test needs full XPath 1.0 (via `Nokogiri::CSS.xpath_for`).
   Capybara is test-only in all 16 apps that have it.

   Recommendation: support only a location-path subset that covers the
   fixed expressions, and raise on anything else. Leave Capybara on real
   Nokogiri in the `:test` group.
3. **HTML4 entry points.** Rails' suites, rails-dom-testing (unless
   `load_defaults` >= 7.1) and Capybara (unless `use_html5_parsing`) parse
   with `HTML4::Document`. Loofah and Action Text pick HTML5 when
   `defined?(Nokogiri::HTML5)`.

   Recommendation: provide HTML5 only, so those checks choose it. Make the
   HTML4 names route to the HTML5 parser, and document the differences.
4. **XML parsing for SVG.** inline_svg calls `XML::Document.parse` on SVG
   files at runtime (5 of 19 apps).

   Decided: HTML first. XML users wait, smallest first: inline_svg (parse,
   attribute edits, `//comment()`), then fog-xml and twilio-ruby. ruby-saml
   needs XSD, C14N and namespaced XPath, so it stays on Nokogiri. sanitize
   (HTML5 only), reverse_markdown, premailer, roadie and html-pipeline are
   HTML gems and belong to the HTML work; the last four parse with the HTML4
   names (decision 3).

Two extra phase 2 requirements came out of the inventory:

- **Extension points.** Loofah's `Document#decorators` hook, subclassing of
  the Document/Fragment classes, and Nokogiri's class names and constants
  for `case`/`is_a?` checks.
- **CSS dialect.** A front end for Nokogiri's selector extensions:
  `[@attr]`, `:first`, custom pseudo-class handlers passed to `css`, and
  `CSS::SyntaxError`.
