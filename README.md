# lobzique

This will be an implementation of a subset of Nokogiri that used by Rails and related libraries with a Rust extension in place of libxml.

## Sanitizing in Rails

`Lobzique::Sanitizer` is a rails-html-sanitizer vendor: `sanitize`,
`strip_tags`, `strip_links` and Action Text parse, scrub and serialize with
lobzique, so user HTML never reaches libxml2. Its output matches
`Rails::HTML5::Sanitizer`'s byte for byte, except for one known gap (below).

```ruby
# config/application.rb, after require "rails/all"
require "lobzique/sanitizer"

config.action_view.sanitizer_vendor = Lobzique::Sanitizer
config.action_text.sanitizer_vendor = Lobzique::Sanitizer
```

It needs `loofah` and `rails-html-sanitizer`, which every Rails app already
has, and raises at `require` on a rails-html-sanitizer outside 1.7.1–1.7.x:
the vendor is built from that gem's internal modules. Loofah's and Rails'
scrubbers run unchanged, and so do app scrubbers passed with `scrubber:` or
set as `ActionText::ContentHelper.scrubber`, with these limits:

- A scrubber that checks `is_a?(Nokogiri::XML::Element)` (or another
  Nokogiri class) or builds nodes with `Nokogiri::XML::Text.new` and the like fails on
  lobzique nodes. Build nodes through `node.document` (`create_element`,
  `create_text_node`) or with `node.add_next_sibling("<b>…</b>")`.
- The vendor is HTML5 only. An app on `load_defaults` below 7.1, whose
  default vendor is `Rails::HTML4::Sanitizer`, sanitizes with an HTML5
  parser after switching.
- The parser keeps markup inside `<select>`, as the 2025 HTML standard's
  customizable select does; Nokogiri's parser, and browsers without
  customizable select, drop or move it. So output differs from
  `Rails::HTML5` when `select` is in the allowed tags. The text of `style`,
  `xmp`, `iframe`, `noembed`, `noframes` and `plaintext` inside a `select` is
  escaped, as Loofah escapes it on libxml2, so the output is safe with either
  set of rules.
- Loofah's `text` and `to_text` and its document-level comment removal
  aren't ported; Rails' sanitizers call neither.

## Action Text

With the sanitizer vendor above, Action Text still parses stored rich text
with Nokogiri to find attachments, convert Trix markup and build plain text.
To parse it with lobzique as well:

```ruby
# config/application.rb, after require "rails/all"
require "lobzique/action_text"
```

`ActionText::Content#fragment.source` and the nodes passed to
`Fragment#replace` blocks are then lobzique nodes, with the limits listed
for scrubbers above. Stored content reads the same as on
`Nokogiri::HTML5`, apart from the `<select>` difference above.

## Tests: assert_dom and parsed_body

rails-dom-testing parses HTML for `assert_dom` (`assert_select`),
`assert_dom_equal`, `css_select`, `response.parsed_body`, and
`rendered.html` in view tests. To parse it with lobzique:

```ruby
# test/test_helper.rb, after require "rails/test_help"
require "lobzique/rails_dom_testing"
```

Wherever rails-dom-testing would use `Nokogiri::HTML5`, it then uses
lobzique: on `load_defaults` 7.1 and later, or with
`config.dom_testing_default_html_version = :html5`. `:html4` stays on
Nokogiri, and so do XML responses. It raises at `require` on a
rails-dom-testing outside 2.3.x, because it replaces one of that gem's
private methods. Limits:

- Selected elements, `parsed_body` and `rendered.html` are lobzique
  objects, so a test asserting `Nokogiri::…` classes fails.
- An invalid selector raises `Lobzique::CSS::SyntaxError`, not
  `Nokogiri::CSS::SyntaxError`.

## Development

`bundle exec rake` compiles the extension, runs the tests and lints.

Library code goes in `crates/lobzique-core`, which must not depend on Ruby, so the checks below can run it without a Ruby VM. `ext/lobzique` holds only the Ruby bindings.

The CI workflow runs these checks on every PR and daily. Locally they need nightly Rust (`rustup toolchain install nightly --component miri,rust-src`) and `cargo install cargo-fuzz`.

| Check | Locally | Catches |
| --- | --- | --- |
| Miri | `cargo +nightly miri test -p lobzique-core` | undefined behavior in unsafe code, ours or our dependencies' |
| ASan (Rust) | `RUSTFLAGS=-Zsanitizer=address cargo +nightly test -Zbuild-std --target aarch64-apple-darwin -p lobzique-core` | memory errors in the core crate |
| ASan (Ruby) | CI only: ASan builds of Ruby exist for Linux x64 only | memory errors in the extension and at the Ruby boundary |
| GC.stress | `bundle exec rake test:gc_stress` | Ruby objects that Rust holds without the GC knowing |
| Fuzzing | `cargo +nightly fuzz run <target>` | panics and failed assertions on arbitrary input |
| Fuzzing the Ruby API | `bundle exec rake fuzz:ruby` (Linux, see below) | memory errors in the extension, GC and compaction mistakes, and errors Nokogiri doesn't raise, on fuzzed sequences of Ruby calls |

`rake test:gc_stress` and `rake fuzz:ruby` leave a special build in `lib/`; `rake compile` replaces it. `rake test:stress` runs GC.stress on whatever build is in `lib/` (the ASan job's, for one), each test file in its own process, `STRESS_JOBS` at a time.

Every entry point that takes untrusted input gets a fuzz target in `fuzz/fuzz_targets/`, plus a `[[bin]]` entry in `fuzz/Cargo.toml`. CI fuzzes each target for 60 seconds on PRs; for a long run, dispatch the Fuzz workflow (`gh workflow run fuzz.yml -f seconds=7200`). A target's libFuzzer dictionary goes in `fuzz/<target>.dict`. CI also passes `-timeout=10 -malloc_limit_mb=512`, so a slow input or a single huge allocation fails. Slowness under the timeout is `html5_scaling`'s job: it repeats its input and fails if the work the core counts under `cfg(fuzzing)` (`lobzique_core::work`) grows faster than linearly in the input times the tree's depth.

The Rust targets never reach the bindings or the Ruby layer. `fuzz/ruby/api.rb` does: with [Ruzzy](https://github.com/trailofbits/ruzzy), it runs fuzzed sequences of Ruby calls (editing, copying, searching, scrubbing and serializing nodes, XPath functions that edit the tree, GC and compaction in between) on an extension built with ASan and coverage instrumentation. It fails on a crash, an ASan report or a panic; on an exception that Nokogiri doesn't raise on the same calls from the same tree, apart from the departures in `ARCHITECTURE.md`; and on sanitized output that isn't idempotent or doesn't survive a reparse where Rails::HTML5's does. Nokogiri runs in a child process, only to judge an input; trees it builds differently (as `known.yml` lists) aren't compared from where they differ, and neither are the sanitizer's properties on markup it parses differently, though a sanitizer exception still is. Ruzzy is Linux-only and needs clang with its sanitizer runtimes (`libclang-rt-18-dev` on Ubuntu) and nightly Rust:

```sh
bundle config set --local with fuzz
CC=clang CXX=clang++ LDSHARED="clang -shared" LDSHAREDXX="clang++ -shared" \
  MAKE="make --environment-overrides" bundle install
bundle exec rake fuzz:ruby FUZZOPTS="-max_total_time=600 -fork=4"
```

Failing inputs go to `fuzz/artifacts/ruby_api/`. To run one again, after `rake fuzz:ruby` has built the extension:

```sh
LD_PRELOAD=$(ruby -e 'require "ruzzy"; print Ruzzy::ASAN_PATH') ASAN_OPTIONS=detect_leaks=0:use_sigaltstack=0 \
  bundle exec fuzz/ruby/trace.rb fuzz/artifacts/ruby_api/crash-...
```

CI runs it next to the Rust targets.

### Differential test against Nokogiri::HTML5

`script/differential` parses the same inputs with Nokogiri (a development dependency) and with lobzique and compares `to_html` and the tree.

| Corpus | Command | When CI runs it |
| --- | --- | --- |
| fixtures: WPT, html5lib, DOMPurify and asset inputs from `test/fixtures` | `script/differential` | every push |
| small hand-written cases in `test/differential/cases.yml` | `script/differential` | every push |
| a pinned Common Crawl sample (~3,100 pages) | `script/fetch_crawl`, then `script/differential --corpus crawl` | weekly |

`--mode sanitize` sends the inputs from the fixtures (DOMPurify, the WPT sanitizer and tree-construction tests, Loofah's sanitizer data, html5lib) or the crawl through `Rails::HTML5`'s sanitizers and through `Lobzique::Sanitizer`, in several configurations. It compares the output, the output sanitized again, and the output parsed and serialized again. Rails::HTML5 itself isn't idempotent on every input, so lobzique has to break these properties exactly where it does; the report counts both. CI runs it next to the parse mode.

`--mode action_text` reads the sanitize mode's inputs, and the markup in Action Text's tests, as `ActionText::Content` with Action Text's document classes set to Nokogiri's and then to lobzique's, and compares `to_html`, `to_trix_html` (and what saving it back stores), `to_plain_text`, the attachments, galleries and links. CI runs it next to the other modes.

Crawled pages aren't committed: `test/differential/crawl.yml` pins byte ranges of WARC files with their SHA-256, and `script/fetch_crawl` downloads them into `tmp/crawl/`. A difference found on a crawled page becomes a small case in `cases.yml`. Accepted differences go in `test/differential/known.yml` with a reason.

## License

The gem is available as open source under the terms of the [MIT License](https://opensource.org/licenses/MIT).

## Code of Conduct

Everyone interacting in the Lobzique project's codebases, issue trackers, chat rooms and mailing lists is expected to follow the [code of conduct](https://github.com/[USERNAME]/lobzique/blob/main/CODE_OF_CONDUCT.md).
