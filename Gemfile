# frozen_string_literal: true

source "https://rubygems.org"

gemspec

gem "irb"
gem "rake"
gem "rake-compiler"
gem "minitest"
gem "standard"
gem "nokogiri", "~> 1.19"

# The sanitizer vendor (sanitizer.md) runs on these; apps bring their own.
# loofah 2.25.3 and rails-html-sanitizer 1.7.2 need Nokogiri >= 1.19.5, so
# they move with the oracle above.
gem "loofah", "2.25.2"
gem "rails-html-sanitizer", "1.7.1"
gem "actionview"
