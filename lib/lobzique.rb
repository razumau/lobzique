# frozen_string_literal: true

# Adapted from Nokogiri 1.19.4, lib/nokogiri.rb (MIT, see LICENSE-NOKOGIRI.md).
# The extension defines the native classes first; these files reopen them.

require "lobzique/lobzique"

require_relative "lobzique/version"
require_relative "lobzique/class_resolver"
require_relative "lobzique/syntax_error"
require_relative "lobzique/xml"
require_relative "lobzique/html4"
require_relative "lobzique/html"
require_relative "lobzique/css"
require_relative "lobzique/html5"
