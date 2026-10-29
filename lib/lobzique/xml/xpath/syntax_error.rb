# frozen_string_literal: true

# Adapted from Nokogiri 1.19.4, lib/nokogiri/xml/xpath/syntax_error.rb (MIT, see LICENSE-NOKOGIRI.md).

module Lobzique
  module XML
    module XPath
      class SyntaxError < XML::SyntaxError
        def to_s
          [super.chomp, str1].compact.join(": ")
        end
      end

      # lobzique: valid XPath outside the subset lobzique evaluates (plan.md
      # decision 2); the message names the construct
      class UnsupportedError < SyntaxError
      end
    end
  end
end
