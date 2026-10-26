# frozen_string_literal: true

# Adapted from Nokogiri 1.19.4, lib/nokogiri/css/syntax_error.rb (MIT, see LICENSE-NOKOGIRI.md).

require_relative "../syntax_error"

module Lobzique
  module CSS
    class SyntaxError < Lobzique::SyntaxError
    end
  end
end
