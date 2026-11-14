# frozen_string_literal: true

# Adapted from Nokogiri 1.19.7, lib/nokogiri/css.rb (MIT, see LICENSE-NOKOGIRI.md).
# Searchable doesn't come through here: it compiles selectors into
# CSS::Selector (ARCHITECTURE.md). xpath_for is for callers that want the
# XPath strings, which the core writes as Nokogiri's visitor does.

module Lobzique
  module CSS
    class << self
      # lobzique: no CSS.parse, which is deprecated and returns CSS::Nodes

      def xpath_for(
        selector, options = nil,
        prefix: options&.delete(:prefix),
        visitor: options&.delete(:visitor),
        ns: options&.delete(:ns),
        cache: true
      )
        unless options.nil?
          # lobzique: Nokogiri names itself
          warn("#{self}.xpath_for: Passing options as an explicit hash is deprecated. Use keyword arguments instead. This will become an error in a future release.", uplevel: 1, category: :deprecated)
        end

        raise(TypeError, "no implicit conversion of #{selector.inspect} to String") unless selector.respond_to?(:to_str)

        selector = selector.to_str
        raise(CSS::SyntaxError, "empty CSS selector") if selector.empty?

        if visitor
          raise ArgumentError, "cannot provide both :prefix and :visitor" if prefix
          raise ArgumentError, "cannot provide both :ns and :visitor" if ns
        end

        visitor ||= begin
          visitor_kw = {}
          visitor_kw[:prefix] = prefix if prefix
          visitor_kw[:namespaces] = ns if ns

          CSS::XPathVisitor.new(**visitor_kw)
        end

        if cache
          key = SelectorCache.key(selector: selector, visitor: visitor)
          SelectorCache[key] ||= Parser.new.xpath_for(selector, visitor)
        else
          Parser.new.xpath_for(selector, visitor)
        end
      end
    end
  end
end

require_relative "css/selector_cache"
require_relative "css/xpath_visitor"
require_relative "css/syntax_error"
