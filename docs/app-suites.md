# App suites under lobzique

`mise run measure:apps` (`script/apps`) runs an app's suite (`apps.tsv` ref)
as checked out, and then in each mode that applies to it (plan.md
decision 1). Postgres runs in Docker.

## Seam mode

lobzique in the Gemfile and both `sanitizer_vendor`s set in
`config/application.rb`, as the README says. Both runs give the same
results for every app:

- Campfire renders Action Text, and its content filters and Opengraph
  metadata sanitize with `ActionText::ContentHelper.sanitizer.class`.
  649 runs, 3921 assertions, all pass, Ruby 4.0.6. DNS is answered
  locally in both runs, because Campfire's SSRF guard resolves hosts
  before WebMock stubs the request.
- Fizzy renders Action Text and scrubs it with its own Loofah
  scrubber. 1809 tests, 6894 assertions, all pass, Ruby 4.0.9. Its
  initializer adds tags to `Rails::HTML5::SafeListSanitizer.allowed_tags`.
  The vendor's class had its own lists and dropped `<table>` in
  `sanitize`; no test noticed, because Fizzy never calls `sanitize`
  and copies the lists into `ActionText::ContentHelper` itself. The
  vendor now reads and writes Rails' lists.
- Loomio calls `strip_tags`, `sanitize` in mailers and
  `ActionView::Base.full_sanitizer`; its rich text goes through
  `Rails::Html::WhiteListSanitizer` directly and stays on Loofah. 3540
  runs, 35387 assertions, the same 86 failures in both runs: they need
  the built Vue client or `node`. Ruby 4.0.9.

## Facade mode

The app's nokogiri replaced with `gem "nokogiri", path:` to this repository,
and nothing else changed. A check before the suite makes sure `Nokogiri` is
lobzique.

- Lobsters calls `Nokogiri::HTML` and `Nokogiri::HTML5.fragment` on
  rendered Markdown and fetched pages, and never a sanitizer vendor, so
  seam mode changes nothing for it. 899 examples, the same one failure
  in both runs (`user_administration_spec`, banning). Its feature specs
  run on Capybara's rack_test driver, which works on the facade.
  `css_widely_available_spec` is left out: it downloads browser data
  when it loads.
- Fizzy: 1809 tests, 6894 assertions, all pass in both runs.
- Campfire: 649 runs, two new errors, both in
  `Users::AvatarsControllerTest`. `assert_select "text"` on the SVG
  avatar makes Action Dispatch parse the response with
  `Nokogiri::XML::Document.parse`, as it does for any media type ending
  in `xml`, and the facade has no XML parser (decision 4). The other 647
  pass.
- DocuSeal isn't run. It loads a pdfium build of its own, published for
  Linux only, and its one `sanitize` call is reached only from Chrome
  system specs that need its JS build.
- Loomio has XML gems, so it is a seam-mode app.
