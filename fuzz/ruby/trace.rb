#!/usr/bin/env ruby
# frozen_string_literal: true

# Runs api.rb with coverage of the Ruby code that it loads after this point.
# The coverage feeds libFuzzer, next to the coverage of the extension.
# The dependencies load first, so their branches do not crowd the counters
# of Ruzzy. The file is executable, because the -fork of libFuzzer runs
# its workers by the path of this script.

$LOAD_PATH.unshift(File.expand_path("../../lib", __dir__))

require "fiddle"
require "ruzzy"
require "nokogiri"
require "loofah"
require "rails-html-sanitizer"
require "yaml"

Ruzzy.trace("api.rb")
