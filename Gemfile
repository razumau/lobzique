# frozen_string_literal: true

source "https://rubygems.org"

gemspec

gem "irb"
gem "rake"
gem "rake-compiler"
gem "minitest"
# Minitest 6 moved stub here; rails-dom-testing's suite uses it
gem "minitest-mock"
gem "standard"
gem "nokogiri", "~> 1.19"

# The sanitizer vendor (plan.md phase 3) runs on these; apps bring their own.
# loofah 2.25.3 and rails-html-sanitizer 1.7.2 need Nokogiri >= 1.19.5, so
# they move with the oracle above.
gem "loofah", "2.25.2"
gem "rails-html-sanitizer", "1.7.1"
gem "actionview"
# The Action Text and rails-dom-testing seams (plan.md phase 4)
gem "actiontext"
gem "rails-dom-testing"

# Fuzzes the Ruby API (fuzz/ruby/). Its extension builds with clang and
# links libFuzzer, so only `bundle config set with fuzz` installs it.
group :fuzz, optional: true do
  gem "ruzzy"
end
