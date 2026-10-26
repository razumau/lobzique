# frozen_string_literal: true

# Adapted from Nokogiri 1.19.4, lib/nokogiri/gumbo.rb (MIT, see LICENSE-NOKOGIRI.md).
# lobzique: renamed from Gumbo after the engine it binds (ARCHITECTURE.md,
# departures).

module Lobzique
  module Html5ever
    DEFAULT_MAX_ATTRIBUTES = 400
    # Nokogiri::HTML5 keeps no parse errors unless asked to
    DEFAULT_MAX_ERRORS = 0
    DEFAULT_MAX_TREE_DEPTH = 400
  end
end
