# frozen_string_literal: true

# Adapted from Nokogiri 1.19.7, lib/nokogiri.rb (MIT, see LICENSE-NOKOGIRI.md).
# lobzique: Nokogiri's classes are lobzique's (plan.md decision 1). The module
# is named Nokogiri first, so every class reports Nokogiri::…, and lobzique
# reopens it through the Lobzique constant. Left out, as they need XML
# parsing or the Builder: Nokogiri.parse, make, Slop and Nokogiri().

module Nokogiri
end

if !defined?(::Lobzique)
  ::Lobzique = Nokogiri
elsif !::Lobzique.equal?(Nokogiri)
  raise LoadError, "lobzique was loaded before the nokogiri facade; require nokogiri first, " \
    "or use lobzique's seams with the real nokogiri gem (README)"
end

require_relative "nokogiri/version"
require_relative "nokogiri/loofah"
