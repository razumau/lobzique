# frozen_string_literal: true

module Lobzique
  module Sanitizer
    # html5ever parses <select> as the HTML standard has since 2025
    # (customizable select). It keeps raw-text elements inside
    # it, and serializes their text unescaped. Browsers on the older rules,
    # and gumbo, ignore those start tags and parse the text as markup. So scrub!
    # escapes it, as Loofah escapes the CDATA of libxml2. This lasts
    # until Nokogiri adopts the new rules (sparklemotion/nokogiri#3317)
    # and Loofah handles them.
    module EscapeRawTextInSelect
      RAW_TEXT = %w[iframe noembed noframes plaintext style xmp].freeze

      # Unprefixed names match only HTML elements. Escaped text has no
      # "<" or ">" left, so scrubbing a tree again changes nothing
      # (the fragments of Loofah scrub each child, and callers chain scrub!).
      XPATH = "descendant-or-self::select//*[#{RAW_TEXT.map { |name| "self::#{name}" }.join(" or ")}]" \
        "/text()[contains(., '<') or contains(., '>')]"

      def self.call(root)
        # Most trees have no select, and the short query costs a third of XPATH.
        return if root.text? || !root.at_xpath("descendant-or-self::select")

        root.xpath(XPATH).each do |text|
          text.replace(text.document.create_text_node(Loofah::HTML5::Scrub.escape_tags(text.content)))
        end
      end
    end
  end
end
