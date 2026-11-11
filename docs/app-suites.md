# App suites under lobzique

`mise run measure:apps` (`script/apps`) runs an app's suite (`apps.tsv` ref)
as checked out, then with lobzique in its Gemfile and both
`sanitizer_vendor`s set in `config/application.rb` as the README says.
Postgres runs in Docker. Both runs give the same results for every app:

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

Lobsters calls Nokogiri directly and never a sanitizer vendor, so it
waits for the facade. DocuSeal reaches its one `sanitize` call only
from Chrome system specs that need its JS build.
