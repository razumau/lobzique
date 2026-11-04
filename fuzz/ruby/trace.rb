#!/usr/bin/env ruby
# frozen_string_literal: true

# Runs api.rb with coverage of the Ruby code it loads after this point
# feeding libFuzzer, next to the extension's own. The dependencies load
# first, so their branches don't crowd Ruzzy's counters. Executable, since
# libFuzzer's -fork runs its workers by this script's path.

$LOAD_PATH.unshift(File.expand_path("../../lib", __dir__))

require "fiddle"
require "ruzzy"
require "nokogiri"
require "loofah"
require "rails-html-sanitizer"
require "yaml"

Ruzzy.trace("api.rb")
