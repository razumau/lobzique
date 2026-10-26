# frozen_string_literal: true

# Adapted from Nokogiri 1.19.4, lib/nokogiri/xml/character_data.rb (MIT, see LICENSE-NOKOGIRI.md).

module Lobzique
  module XML
    class CharacterData < Node
      include PP::CharacterData
    end
  end
end
