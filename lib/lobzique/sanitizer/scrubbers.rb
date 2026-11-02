# frozen_string_literal: true

# Adapted from Loofah 2.25.3, lib/loofah/scrubbers.rb (MIT): the scrubbers
# that build nodes with Nokogiri::XML::Text.new, which takes only a Nokogiri
# document. The others run on lobzique nodes as they are.

module Lobzique
  module Sanitizer
    module Scrubbers
      class Escape < Loofah::Scrubbers::Escape
        def scrub(node)
          return CONTINUE if html5lib_sanitize(node) == CONTINUE

          # lobzique: create_text_node, not Nokogiri::XML::Text.new
          node.add_next_sibling(node.document.create_text_node(node.to_s))
          node.remove
          STOP
        end
      end

      class NewlineBlockElements < Loofah::Scrubbers::NewlineBlockElements # :nodoc:
        def scrub(node)
          return CONTINUE unless Loofah::Elements::LINEBREAKERS.include?(node.name)

          replacement = if Loofah::Elements::INLINE_LINE_BREAK.include?(node.name)
            "\n"
          else
            "\n#{node.content}\n"
          end
          # lobzique: create_text_node, not Nokogiri::XML::Text.new
          node.add_next_sibling(node.document.create_text_node(replacement))
          node.remove
        end
      end

      # Overrides of Loofah::Scrubbers::MAP.
      MAP = {
        escape: Escape,
        newline_block_elements: NewlineBlockElements
      }.freeze

      # The Loofah classes they port; their instances hold no state.
      PORTS = MAP.values.to_h { |port| [port.superclass, port] }.freeze
    end
  end
end
