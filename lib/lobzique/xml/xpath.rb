# frozen_string_literal: true

# Adapted from Nokogiri 1.19.4, lib/nokogiri/xml/xpath.rb (MIT, see LICENSE-NOKOGIRI.md).

module Lobzique
  module XML
    module XPath
      GLOBAL_SEARCH_PREFIX = "//"
      ROOT_SEARCH_PREFIX = "/"
      CURRENT_SEARCH_PREFIX = "./"
      SUBTREE_SEARCH_PREFIX = ".//"
    end
  end
end

require_relative "xpath/syntax_error"
