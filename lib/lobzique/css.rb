# frozen_string_literal: true

# Adapted from Nokogiri 1.19.4, lib/nokogiri/css.rb (MIT, see LICENSE-NOKOGIRI.md).
# Nokogiri translates CSS to XPath here; lobzique matches selectors natively
# instead, so only the module and its error come over (ARCHITECTURE.md).

module Lobzique
  module CSS
  end
end

require_relative "css/syntax_error"
