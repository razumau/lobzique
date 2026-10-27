# lobzique

This will be an implementation of a subset of Nokogiri that used by Rails and related libraries with a Rust extension in place of libxml.

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

Every entry point that takes untrusted input gets a fuzz target in `fuzz/fuzz_targets/`, plus a `[[bin]]` entry in `fuzz/Cargo.toml`. CI fuzzes each target for 60 seconds on PRs; for a long run, dispatch the Fuzz workflow (`gh workflow run fuzz.yml -f seconds=7200`). A target's libFuzzer dictionary goes in `fuzz/<target>.dict`.

### Differential test against Nokogiri::HTML5

`script/differential` parses the same inputs with Nokogiri (a development dependency) and with lobzique and compares `to_html` and the tree.

| Corpus | Command | When CI runs it |
| --- | --- | --- |
| fixtures: WPT, html5lib, DOMPurify and asset inputs from `test/fixtures` | `script/differential` | every push |
| small hand-written cases in `test/differential/cases.yml` | `script/differential` | every push |
| a pinned Common Crawl sample (~3,100 pages) | `script/fetch_crawl`, then `script/differential --corpus crawl` | weekly |

Crawled pages aren't committed: `test/differential/crawl.yml` pins byte ranges of WARC files with their SHA-256, and `script/fetch_crawl` downloads them into `tmp/crawl/`. A difference found on a crawled page becomes a small case in `cases.yml`. Accepted differences go in `test/differential/known.yml` with a reason.

## License

The gem is available as open source under the terms of the [MIT License](https://opensource.org/licenses/MIT).

## Code of Conduct

Everyone interacting in the Lobzique project's codebases, issue trackers, chat rooms and mailing lists is expected to follow the [code of conduct](https://github.com/[USERNAME]/lobzique/blob/main/CODE_OF_CONDUCT.md).
