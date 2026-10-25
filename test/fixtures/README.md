# Test fixtures

Third-party conformance and compatibility data, pinned by commit. Don't edit
files here by hand; change `sources.yml` and re-run the fetch script.

```sh
script/fetch_fixtures                 # fetch exactly what sources.lock.yml pins
script/fetch_fixtures --update        # move every pin to upstream HEAD
script/fetch_fixtures --update nokogiri
script/fetch_fixtures --verify        # offline checksum check, for CI
```

Commit `sources.yml`, `sources.lock.yml`, `MANIFEST.json`, `NOTICE.md` and the
fixture directories.

## Formats a harness has to handle

| Directory | Format | Notes |
|---|---|---|
| `html5lib-tests/tokenizer` | JSON (`tests`, `xmlViolationTests`) | Inputs may need `doubleEscaped` unescaping. |
| `html5lib-tests/serializer`, `encoding` | JSON / `.dat` | See each directory's README. |
| `wpt-parsing/tree-construction` | html5lib `.dat` | `#data`, `#errors`, `#document-fragment`, `#script-on`/`off`, `#document`. Error lines are informative only. |
| `wpt-sanitizer-api` | html5lib `.dat` + `#config` | `#config` is a JSON Sanitizer config. Replace `{{host}}` before parsing. `.sub` only means "substituted". |
| `wpt-selectors/selectors.js` | JS object literals | Data only, but not JSON; needs a data-only extractor (never evaluate it). |
| `dompurify/expect.json` | JSON `[{title, payload, expected[]}]` | Derived from `expect.mjs`. Any element of `expected` is acceptable. Outputs reflect DOMPurify's defaults in browsers, so use payloads as attack inputs and the expectations as hints. |
| `nokogiri`, `loofah`, `rails-*` | Minitest files | Behavioral contracts to port or run against the shim. `loofah/assets/testdata_sanitizer_tests1.dat` is JSON keyed by libxml2 version. |
| `rails/` | Minitest files | Need Rails' own test environment; read as a contract. |
