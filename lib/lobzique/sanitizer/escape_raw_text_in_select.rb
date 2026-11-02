# frozen_string_literal: true

module Lobzique
  module Sanitizer
    # html5ever parses <select> as the HTML standard has since 2025
    # (customizable select) and keeps raw-text elements inside it, whose text
    # is serialized unescaped. Browsers on the older rules, and gumbo, ignore
    # those start tags and parse the text as markup, so scrub! escapes it, as
    # Loofah escapes libxml2's CDATA. Until Nokogiri adopts the new rules
    # (sparklemotion/nokogiri#3317) and Loofah handles them.
    module EscapeRawTextInSelect
      RAW_TEXT = %w[iframe noembed noframes plaintext style xmp].freeze

      # Unprefixed names match only HTML elements.
      XPATH = "descendant-or-self::select//*[#{RAW_TEXT.map { |name| "self::#{name}" }.join(" or ")}]/text()"

      def self.call(root)
        root.xpath(XPATH).each do |text|
          text.replace(text.document.create_text_node(Loofah::HTML5::Scrub.escape_tags(text.content)))
        end
      end
    end
  end
end
