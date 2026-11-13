# nokogiri, by lobzique

[lobzique](https://github.com/razumau/lobzique) under Nokogiri's name, for
apps that need Nokogiri only for HTML. Rails, Loofah, rails-html-sanitizer
and rails-dom-testing run on it unpatched, and libxml2 leaves the bundle.

```ruby
# Gemfile, in place of any gem "nokogiri" line
gem "nokogiri", git: "https://github.com/razumau/lobzique"
```

Bundler finds both gems in the repository. The facade is never pushed to
RubyGems, and building it needs a Rust toolchain. It reports version
1.19.7, the Nokogiri whose Ruby layer lobzique ports, and `Nokogiri` and
`Lobzique` are the same module: classes are named `Nokogiri::…`, and
`rescue Nokogiri::CSS::SyntaxError` and `is_a?(Nokogiri::XML::Node)` work.

## Is it for your app?

Only if nothing in the bundle needs what lobzique leaves out. Each of these
raises `NotImplementedError` or `NameError` at the first call, with a message
that names the facade:

- XML parsing: `Nokogiri::XML()`, `XML::Document.parse`, `XML::Reader`,
  schemas. That rules out inline_svg, ruby-saml, fog-xml, twilio-ruby and
  most API clients that parse XML. In tests, `assert_select` and
  `assert_dom` on a response whose media type ends in `xml` (an SVG, a
  feed, a sitemap) parse it as XML and raise too;
- `Builder`, SAX, XSLT and `Slop`;
- `Nokogiri::CSS.xpath_for`. Capybara's rack_test driver works without it
  (Lobsters' feature specs pass), except for expressions built with the
  xpath gem's `css`.

`bundle exec ruby -e 'puts Gem.loaded_specs.values.select { |s|
s.dependencies.any? { _1.name == "nokogiri" } }.map(&:name)'` lists the
gems to check. An app with any of them keeps the real nokogiri gem and uses
lobzique's seams (the main README): a sanitizer vendor, Action Text's
document classes and rails-dom-testing's parser.

## Differences from Nokogiri

`Nokogiri::HTML5` behaves as Nokogiri's, apart from the departures listed in
`ARCHITECTURE.md`. The one most likely to show: markup inside `<select>` is
kept, as the HTML standard has it since 2025, where Nokogiri's parser drops
it. Loofah on the facade escapes raw text inside `select`, so sanitized
output is safe under either set of rules.

`Nokogiri::HTML`, `Nokogiri::HTML4` and their document and fragment classes
keep their names and hierarchy, but parse and serialize as HTML5. Input is
still decoded as libxml2 decodes it (`<meta>` charsets, ISO-8859-1 by
default), and `document.encoding` is the same. What changes:

- the tree is a browser's: table parts outside a table are dropped (inky-rb
  builds its components from them and breaks), `<frameset>` content, the
  newline after `<pre>` and `<textarea>`, stray end tags; SVG and MathML
  elements have their namespaces;
- output is HTML5's: `<!DOCTYPE html>` or none instead of the HTML 4.0
  doctype, characters instead of `&#233;` and `&nbsp;` kept as an entity,
  no percent-escaping of URI attributes, no `Content-Type` `<meta>` added,
  no formatting newlines. Snapshot tests of HTML4 output need new snapshots;
- parse options are ignored, and errors are the HTML5 parser's;
- bytes invalid in the document's encoding become U+FFFD.

An app on `load_defaults` below 7.1 sanitizes with `Rails::HTML4::Sanitizer`
and asserts with rails-dom-testing's `:html4`; both run on the HTML5 parser
here. Set them to HTML5 explicitly, so the app behaves the same on either
nokogiri:

```ruby
config.action_view.sanitizer_vendor = Rails::HTML5::Sanitizer
config.action_text.sanitizer_vendor = Rails::HTML5::Sanitizer
config.dom_testing_default_html_version = :html5
```

Loofah's `text`/`to_text` and its other entry points work as on Nokogiri;
`Loofah.xml_document` and `xml_fragment` need the XML parser.
