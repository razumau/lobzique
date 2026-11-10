# frozen_string_literal: true

# Adapted from Nokogiri 1.19.7, lib/nokogiri/extension.rb (MIT, see LICENSE-NOKOGIRI.md).
# lobzique: loads lobzique in place of the native extension, then defines the
# constants Nokogiri's extension defines and lobzique's doesn't.

require "lobzique"

module Nokogiri
  # lobzique: the libxml2 whose behavior lobzique reproduces (ARCHITECTURE.md).
  # It makes VersionInfo#libxml2? and uses_gumbo? true, which Loofah reads:
  # without them it has no HTML5 classes, and it escapes attribute values for
  # libxml2's serializer, as lobzique's vendor does to match Rails::HTML5.
  LIBXML_COMPILED_VERSION = "2.13.9"
  LIBXML_LOADED_VERSION = "21309"
  LIBXML_MEMORY_MANAGEMENT = "ruby"
  LIBXML_ICONV_ENABLED = false
  LIBXML_ZLIB_ENABLED = false
  LIBXML_HTTP_ENABLED = false
  PACKAGED_LIBRARIES = true
  PRECOMPILED_LIBRARIES = false
  LIBXML2_PATCHES = [].freeze
end
