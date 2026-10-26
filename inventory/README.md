# Nokogiri API inventory (phase 1)

`contract.yml` lists the parts of Nokogiri that lobzique has to provide. Each
entry is a class, module, constant, method, selector string or extension point.
It records who uses the entry, how it is called (receiver classes, argument
shapes, blocks, return classes) and a scope tag. `summary.md` shows the same
data as tables. Both files are generated; scope decisions go in `scope.yml`.

Data is from Nokogiri 1.19.4 and Rails `main` at d5c1019, the same commit
`test/fixtures` pins. The inventory was collected on 2026-10-25.

## Sources

| What | Where | Pinned in |
|---|---|---|
| Target gems: Tier 1 (Rails components, Loofah, rails-html-sanitizer, rails-dom-testing) and Tier 2 (50 more) | git tags / packaged gems | `script/inventory/targets.yml` |
| Public Rails apps used for ranking (19 with a committed `Gemfile.lock`) | GitHub, GitLab | `script/inventory/apps.tsv` |
| Runtime dependents of nokogiri and loofah with download counts | RubyGems API | fetched into `raw/rank/` |

Redmine and Solidus were planned as corpus apps but don't commit a lockfile,
so they are left out. The apps were chosen as public stand-ins; nothing from
private apps is in here.

Tier 2 was picked by `script/inventory/rank.rb`, in this order:

1. gems that depend on nokogiri/loofah in 2+ corpus apps (14);
2. gems in 2+ apps that use Nokogiri without declaring it (12), found by
   grepping the 774 gems that occur in at least two apps;
3. other runtime dependents released since 2023, by downloads (24).

Each target has a `domain`: `html` (runtime HTML), `html-test`, `xml` or
`debug`. Entries whose only users are `xml` gems are tagged `out:xml`.

## How the data was collected

- **Recorder** (`script/inventory/recorder.rb`). Loaded with
  `RUBYOPT=-rbundler/setup -r…`. It wraps every method Nokogiri defines, plus
  `is_a?`/`===`/`respond_to?`, `new` and `inherited` on its root classes. It
  records each call made from outside Nokogiri together with its call site.
  It also records subclasses, mixins and methods that other gems add to
  Nokogiri classes.
- **Static scan** (`script/inventory/scan.rb`). A Prism visitor collects:
  - constant references and the role each one plays (superclass, rescue,
    `when`, `is_a?`, …);
  - calls on Nokogiri constants;
  - reopened classes;
  - selector and XPath literals;
  - name-only method matches. These are kept only for Nokogiri-specific names,
    or in files that reference Nokogiri.
- **Corpus** (`script/inventory/apps.rb`). Lockfiles are read with Bundler's
  parser and Gemfiles with Prism; neither is evaluated. This gives every
  app's direct nokogiri/loofah dependents, and whether each one can be reached
  from runtime groups or only from dev/test groups.

Evidence marks in `summary.md`:

- no mark: seen at runtime by the recorder;
- `*`: static, receiver known;
- `?`: name match only.

Tier 1 suites ran under the recorder. Tier 2 suites have not been run yet (see
"Not done yet"), so Tier 2 evidence is static only. Every Tier 2 name match
used by an HTML gem was checked by hand against its call site; the results
are in `scope.yml`.

## Regenerating

```sh
ruby script/inventory/rank.rb                     # prints the tier2 block for targets.yml; inputs in its header
ruby -rbundler/setup script/inventory/api_surface.rb inventory/raw/nokogiri-api.json   # in a bundle with nokogiri
ruby script/inventory/setup.rb [NAME...]          # checks out and bundles targets with a suite into raw/src
ruby script/inventory/run_suites.rb [NAME...]     # traces into raw/traces
ruby script/inventory/scan_all.rb                 # scans into raw/scans
ruby script/inventory/apps.rb script/inventory/apps.tsv inventory/raw/lockfiles inventory/raw/rank/apps.json
ruby script/inventory/aggregate.rb                # writes contract.yml and summary.md
```

Run `setup.rb` and `run_suites.rb` with the repo's Ruby (3.3, from
`mise.toml`). Checkouts are used as tagged, with their own lockfiles; a target
whose Gemfile needs a newer Ruby sets `ruby:` in `targets.yml` (only
view_component, 4.0).

`inventory/raw/` is in `.gitignore` for now. It holds the source checkouts,
bundles, packages, traces, scans and the corpus, about 2 GB in total.

## Results

| scope | entries | meaning |
|---|---:|---|
| `in` | 151 | the DOM below, plus the HTML4 entry points (10), `xpath`/`at_xpath` and the `Nokogiri::CSS` namespace |
| `stub` | 5 | `VERSION`, `VersionInfo#libxml2?`, `jruby?`, `uses_gumbo?` |
| `later:xml` | 2 | `XML::Document.parse`, `Nokogiri.XML`; XML comes after the HTML work |
| `out:xpath` | 1 | `Nokogiri::CSS.xpath_for`, used only by Capybara through the xpath gem |
| `out:xml` | 52 | used only by XML-domain gems |
| `out:sax` | 7 | SAX; the only HTML user is truncato |
| `noise` | 5 | name matches whose receivers aren't Nokogiri objects |

The `in` set is the DOM most Rails code would expect: Node and NodeSet
traversal and mutation, `css`/`at_css`/`search`/`matches?`, attributes,
`to_html`/`to_s`/`inner_html`, `HTML5.fragment`/`parse` and Document/Fragment
construction. Some parts are less obvious:

- **Extension points.**
  - Loofah subclasses `HTML5::Document`/`DocumentFragment` and their HTML4/XML
    counterparts. It also registers `ScrubBehavior` with
    `Document#decorators(Nokogiri::XML::Node) << …`, so every node a document
    hands out must be extended with the registered modules.
  - html-pipeline 2 reopens `Nokogiri::XML::Node`.
  - ActiveSupport's XmlMini mixes conversions into node classes, but that is
    `out:xml`.
- **Class identity.** Code checks classes with `case`/`when` and `is_a?`
  (`XML::DocumentFragment`, `XML::Text`, `XML::NodeSet`). It also compares
  `node.type` with `XML::Node::*_NODE` and passes `SaveOptions` flags
  (`AS_HTML`, `NO_DECLARATION`, `NO_EMPTY_TAGS`). The class tree and
  constants have to match Nokogiri's names, not just behave alike.
- **CSS dialect.** The selector strings in use go beyond standard CSS:
  - Nokogiri's XPath-style attribute syntax `[@attr]` and `[@attr^='#']`
    (premailer, mechanize);
  - custom pseudo-class handlers registered through the second argument of
    `css` (rails-dom-testing's `:match('id', …)`, rspec-html-matchers'
    `:content()`/`:regexp()`);
  - the non-standard `:first` (`code a:first`) and `:has(... + ...)`
    (Action Text);
  - `Nokogiri::CSS::SyntaxError` for invalid selectors, which roadie and
    Capybara rescue.

  Servo's `selectors` crate needs a front end that rewrites these.

## Decisions this raised

These were decided in `plan.md` ("Decisions from the inventory"); the
numbers below follow it. This section keeps the evidence.

1. **Which gem name and constant lobzique occupies.** Decided: a core under
   `Lobzique`, plus a `nokogiri` facade for apps that can drop real
   Nokogiri. actionpack, actiontext, loofah, rails-html-sanitizer and
   rails-dom-testing all hard-depend on `nokogiri`.
   - In 15 of 19 corpus apps, at least one other runtime gem also depends on
     it. The common ones are ruby-saml (7), sanitize (6), inline_svg (5),
     reverse_markdown (5), fog-xml (4) and twilio-ruby (3).
   - Only Lobsters, Fizzy, Campfire and DocuSeal use Nokogiri at runtime
     solely through Rails.
   - Discourse would also qualify if sanitize, reverse_markdown and
     ruby-readability run on lobzique.
   - Maybe and Errbit would qualify once XML parsing lands (decision 4).
2. **XPath.** Decided: a location-path subset; anything else raises.
   Runtime HTML code uses a small, fixed set of expressions:

   | Gem | Expressions |
   |---|---|
   | Loofah | `/html/body`, `./body`, `./form`, `//p[br[following-sibling::br]]` |
   | html-pipeline | `.//text()` |
   | inline_svg | `//comment()` |
   | reverse_markdown | `preceding::text()[1]`, `li` |
   | roadie | `html`, `html/head`, `html/head/meta` |
   | mechanize | `//head/meta[@http-equiv="Set-Cookie"]`, `string(((/html/head \| /html \| /head \| /)/title)[1])` |

   mechanize is the only HTML gem that needs unions and functions. It stays
   out of the subset unless it turns out to be runtime in the corpus.

   Two cases need more than the subset:
   - `Rails::HTML::Sanitizer#remove_xpaths` passes its argument straight to
     `xpath`. It is private, and only rails-html-sanitizer's own tests call
     it, so it is a hook for subclasses.
   - Capybara's rack_test driver needs full XPath 1.0 through the `xpath`
     gem and `Nokogiri::CSS.xpath_for`. Capybara/xpath are in 16 of 19 apps
     and are test-only in all 16. Facade apps can't keep real Nokogiri next
     to the facade, so rack_test is unsupported there for now.
3. **HTML4 entry points.** Decided: they keep Nokogiri's names and
   hierarchy but parse and serialize with the HTML5 parser.
   - Rails' own suites go through `HTML4::Document.parse`, because
     rails-dom-testing defaults to HTML4 unless `load_defaults` sets HTML5.
   - Capybara parses with HTML4 unless `Capybara.use_html5_parsing` is set.
   - Loofah, Action Text and rails-dom-testing fall back to HTML4 when
     `defined?(Nokogiri::HTML5)` is false.
   - `sanitizer_vendor` defaults to `Rails::HTML4::Sanitizer` before
     `load_defaults 7.1`.
   - premailer, roadie, reverse_markdown, html2text, inky-rb, mechanize and
     html-pipeline always use HTML4.
4. **XML parsing of SVG.** Decided: after the HTML work, starting with
   inline_svg. inline_svg runs in 5 of 19 apps at runtime. It parses SVG
   files with `XML::Document.parse` and edits them.

## Not done yet

- **Tier 2 suites under the recorder.** Run `setup.rb`, then `run_suites.rb`
  without arguments and `aggregate.rb`. This turns their `*`/`?` evidence
  into recorded calls with receiver types.
- **Tier 1 suites with known failures**, all unrelated to the recorder:
  - actiontext: one JS package test needs `yarn`;
  - actionpack: three timing-sensitive `ServerTimingTest` cases.
