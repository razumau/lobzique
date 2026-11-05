# frozen_string_literal: true

# Action Text on lobzique (plan.md decision 1, seam mode):
#
#   require "lobzique/action_text"
#
# Action Text memoizes its document classes in instance variables, which is
# its own seam. Fragment.wrap knows only Nokogiri's fragment class, so without
# the patch it would serialize a lobzique fragment and parse it again.

require "lobzique"
require "action_text"

module Lobzique
  module ActionText
    module Fragment
      def wrap(fragment_or_html)
        return new(fragment_or_html) if fragment_or_html.is_a?(XML::DocumentFragment)

        super
      end
    end
  end
end

ActionText.instance_variable_set(:@html_document_class, Lobzique::HTML5::Document)
ActionText.instance_variable_set(:@html_document_fragment_class, Lobzique::HTML5::DocumentFragment)
ActionText::Fragment.singleton_class.prepend(Lobzique::ActionText::Fragment)
