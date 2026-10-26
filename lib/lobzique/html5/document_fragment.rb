# frozen_string_literal: true

# Adapted from Nokogiri 1.19.4, lib/nokogiri/html5/document_fragment.rb (MIT, see LICENSE-NOKOGIRI.md).

require_relative "../html4/document_fragment"

module Lobzique
  module HTML5
    class DocumentFragment < HTML4::DocumentFragment
      # Ruby attributes, not the native `Node#document`: Loofah and others
      # build a fragment with `new(document, input)` and read them back.
      attr_accessor :document
      attr_accessor :errors

      attr_reader :quirks_mode

      def initialize(
        doc, input = nil,
        context_ = nil, positional_options_hash = nil,
        context: context_,
        **options
      ) # rubocop:disable Lint/MissingSuper
        unless positional_options_hash.nil? || positional_options_hash.empty?
          options.merge!(positional_options_hash)
        end

        @document = doc
        @errors = []
        return unless input

        input = HTML5.read_and_encode(input, nil)

        context = options.delete(:context) if options.key?(:context)

        options[:max_attributes] ||= Html5ever::DEFAULT_MAX_ATTRIBUTES
        options[:max_errors] ||= options.delete(:max_parse_errors) || Html5ever::DEFAULT_MAX_ERRORS
        options[:max_tree_depth] ||= Html5ever::DEFAULT_MAX_TREE_DEPTH

        Html5ever.fragment(self, input, context, **options)
      end
    end
  end
end
