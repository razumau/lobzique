# frozen_string_literal: true

# Adapted from Nokogiri 1.19.4, lib/nokogiri/html5/node.rb (MIT, see LICENSE-NOKOGIRI.md).

require_relative "../xml/node"

module Lobzique
  module HTML5
    # Overrides prepended into every node class, active in HTML documents.
    module Node
      def fragment(tags)
        # lobzique: Nokogiri checks for HTML5::Document; HTML4 documents use
        # the HTML5 parser too (plan.md decision 3)
        return super unless document.is_a?(HTML4::Document)

        DocumentFragment.new(document, tags, self)
      end
    end

    XML::Node.prepend(HTML5::Node)
  end
end
