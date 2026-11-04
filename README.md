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

`rake test:gc_stress` leaves a slow debug build in `lib/`; `rake compile` replaces it.

Every entry point that takes untrusted input gets a fuzz target in `fuzz/fuzz_targets/`, plus a `[[bin]]` entry in `fuzz/Cargo.toml`. CI fuzzes each target for 60 seconds on PRs; for a long run, dispatch the Fuzz workflow (`gh workflow run fuzz.yml -f seconds=7200`). A target's libFuzzer dictionary goes in `fuzz/<target>.dict`. CI also passes `-timeout=10 -malloc_limit_mb=512`, so a slow input or a single huge allocation fails. Slowness under the timeout is `html5_scaling`'s job: it repeats its input and fails if the work the core counts under `cfg(fuzzing)` (`lobzique_core::work`) grows faster than linearly in the input times the tree's depth.

### Differential test against Nokogiri::HTML5

`script/differential` parses the same inputs with Nokogiri (a development dependency) and with lobzique and compares `to_html` and the tree.

| Corpus | Command | When CI runs it |
| --- | --- | --- |
| fixtures: WPT, html5lib, DOMPurify and asset inputs from `test/fixtures` | `script/differential` | every push |
| small hand-written cases in `test/differential/cases.yml` | `script/differential` | every push |
| a pinned Common Crawl sample (~3,100 pages) | `script/fetch_crawl`, then `script/differential --corpus crawl` | weekly |

`--mode sanitize` sends the inputs from the fixtures (DOMPurify, the WPT sanitizer and tree-construction tests, Loofah's sanitizer data, html5lib) or the crawl through `Rails::HTML5`'s sanitizers and through `Lobzique::Sanitizer`, in several configurations. It compares the output, the output sanitized again, and the output parsed and serialized again. Rails::HTML5 itself isn't idempotent on every input, so lobzique has to break these properties exactly where it does; the report counts both. CI runs it next to the parse mode.

Crawled pages aren't committed: `test/differential/crawl.yml` pins byte ranges of WARC files with their SHA-256, and `script/fetch_crawl` downloads them into `tmp/crawl/`. A difference found on a crawled page becomes a small case in `cases.yml`. Accepted differences go in `test/differential/known.yml` with a reason.

## License

The gem is available as open source under the terms of the [MIT License](https://opensource.org/licenses/MIT).

## Code of Conduct

Everyone interacting in the Lobzique project's codebases, issue trackers, chat rooms and mailing lists is expected to follow the [code of conduct](https://github.com/[USERNAME]/lobzique/blob/main/CODE_OF_CONDUCT.md).
