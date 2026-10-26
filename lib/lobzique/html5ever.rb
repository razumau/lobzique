# frozen_string_literal: true

# The native parser binding, in the role of Nokogiri's Gumbo module
# (lib/nokogiri/gumbo.rb), with the same limits.

module Lobzique
  module Html5ever
    DEFAULT_MAX_ATTRIBUTES = 400
    # Nokogiri::HTML5 keeps no parse errors unless asked to
    DEFAULT_MAX_ERRORS = 0
    DEFAULT_MAX_TREE_DEPTH = 400
  end
end
