# frozen_string_literal: true

# Adapted from Nokogiri 1.19.4, lib/nokogiri/xml/text.rb (MIT, see LICENSE-NOKOGIRI.md).

module Lobzique
  module XML
    class Text < CharacterData
      # Sets the text as given; Node#content= would escape it.
      def content=(string)
        self.native_content = string.to_s
      end
    end
  end
end
