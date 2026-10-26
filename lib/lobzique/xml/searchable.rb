# frozen_string_literal: true

# Adapted from Nokogiri 1.19.4, lib/nokogiri/xml/searchable.rb (MIT, see LICENSE-NOKOGIRI.md).

module Lobzique
  module XML
    # The search methods shared by Node and NodeSet. Their bodies come with
    # the CSS and XPath engines (plan.md, phase 2).
    module Searchable
      LOOKS_LIKE_XPATH = %r{^(\./|/|\.\.|\.$)}
    end
  end
end
