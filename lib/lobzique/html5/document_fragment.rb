# frozen_string_literal: true

# Adapted from Nokogiri 1.19.4, lib/nokogiri/html5/document_fragment.rb (MIT, see LICENSE-NOKOGIRI.md).

require_relative "../html4/document_fragment"

module Lobzique
  module HTML5
    class DocumentFragment < HTML4::DocumentFragment
      class << self
        # Parses in a new HTML5::Document. Takes `context:` among the options.
        def parse(
          input,
          encoding_ = nil, positional_options_hash = nil,
          encoding: encoding_, **options
        )
          unless positional_options_hash.nil? || positional_options_hash.empty?
            options.merge!(positional_options_hash)
          end

          context = options.delete(:context)

          document = HTML5::Document.new
          document.encoding = "UTF-8"
          input = HTML5.read_and_encode(input, encoding)

          new(document, input, context, options)
        end
      end

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
        return self unless input # rubocop:disable Lint/ReturnInVoidContext

        input = HTML5.read_and_encode(input, nil)

        context = options.delete(:context) if options.key?(:context)

        # lobzique: Html5ever is Nokogiri's Gumbo, renamed after the engine
        options[:max_attributes] ||= Html5ever::DEFAULT_MAX_ATTRIBUTES
        options[:max_errors] ||= options.delete(:max_parse_errors) || Html5ever::DEFAULT_MAX_ERRORS
        options[:max_tree_depth] ||= Html5ever::DEFAULT_MAX_TREE_DEPTH

        Html5ever.fragment(self, input, context, **options)
      end

      def serialize(options = {}, &block) # :nodoc:
        # Bypass XML::Document.serialize which doesn't support options even
        # though we might be passed options (like :preserve_newline)
        XML::Node.instance_method(:serialize).bind_call(self, options, &block)
      end

      def extract_params(params) # :nodoc:
        handler = params.find do |param|
          # lobzique: as in Searchable#extract_params
          ![Hash, String, Symbol, CSS::Selector].include?(param.class)
        end
        params -= [handler] if handler

        hashes = []
        while Hash === params.last || params.last.nil?
          hashes << params.pop
          break if params.empty?
        end
        ns, binds = hashes.reverse

        ns ||=
          begin
            ns = {}
            children.each { |child| ns.merge!(child.namespaces) }
            ns
          end

        [params, handler, ns, binds]
      end
    end
  end
end
