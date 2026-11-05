# frozen_string_literal: true

# rails-dom-testing on lobzique (plan.md decision 1, seam mode):
#
#   require "lobzique/rails_dom_testing"
#
# Wherever rails-dom-testing picks Nokogiri::HTML5's classes, it gets
# lobzique's instead: assert_dom, assert_dom_equal, Action View's `rendered`
# and Action Dispatch's html_document and parsed_body. :html4 stays on
# Nokogiri (decision 3).
#
# HTMLSelector#filter and SelectorAssertions#nodeset build Nokogiri node
# sets, so for lobzique nodes they build lobzique's (filter is ported from
# rails-dom-testing 2.3.0); Nokogiri's still take the original code.

require "lobzique"
require "rails-dom-testing"
require "rails/dom/testing/version"

module Lobzique
  module RailsDomTesting
    # filter is a private method whose code changed in 2.3.0
    REQUIREMENT = Gem::Requirement.new(">= 2.3.0", "< 2.4")

    unless REQUIREMENT.satisfied_by?(Gem::Version.new(Rails::Dom::Testing::VERSION))
      raise "#{self} needs rails-dom-testing #{REQUIREMENT}, not #{Rails::Dom::Testing::VERSION}"
    end

    module ParserSelection
      def html_document(...)
        parser = super
        parser.equal?(::Nokogiri::HTML5::Document) ? HTML5::Document : parser
      end

      def html_document_fragment(...)
        parser = super
        parser.equal?(::Nokogiri::HTML5::DocumentFragment) ? HTML5::DocumentFragment : parser
      end
    end

    module SelectorAssertions
      private

      def nodeset(node)
        case node
        when XML::NodeSet then node
        when XML::Node then XML::NodeSet.new(node.document, [node])
        else super
        end
      end
    end

    module HTMLSelector
      NO_STRIP = Rails::Dom::Testing::Assertions::SelectorAssertions::HTMLSelector::NO_STRIP

      private

      # Ported from rails-dom-testing 2.3.0,
      # lib/rails/dom/testing/assertions/selector_assertions/html_selector.rb
      def filter(matches)
        return super unless matches.is_a?(XML::NodeSet)

        match_with = tests[:text] || tests[:html]
        return matches if matches.empty? || !match_with

        content_mismatch = nil
        text_matches = tests.has_key?(:text)
        html_matches = tests.has_key?(:html)
        regex_matching = match_with.is_a?(Regexp)

        remaining = matches.reject do |match|
          # Preserve markup with to_s for html elements
          content = text_matches ? match.text : match.inner_html

          content.strip! unless NO_STRIP.include?(match.name)
          content.delete_prefix!("\n") if text_matches && match.name == "textarea"
          collapse_html_whitespace!(content) unless NO_STRIP.include?(match.name) || html_matches

          next if regex_matching ? (content =~ match_with) : (content == match_with)
          content_mismatch ||= diff(match_with, content)
          true
        end

        @message ||= content_mismatch if remaining.empty?
        # lobzique: lobzique's NodeSet, not Nokogiri::XML::NodeSet
        XML::NodeSet.new(matches.document, remaining)
      end
    end
  end
end

Rails::Dom::Testing.singleton_class.prepend(Lobzique::RailsDomTesting::ParserSelection)
Rails::Dom::Testing::Assertions::SelectorAssertions.prepend(Lobzique::RailsDomTesting::SelectorAssertions)
Rails::Dom::Testing::Assertions::SelectorAssertions::HTMLSelector.prepend(Lobzique::RailsDomTesting::HTMLSelector)
