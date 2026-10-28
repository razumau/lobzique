# frozen_string_literal: true

# Adapted from Nokogiri 1.19.4, lib/nokogiri/xml/searchable.rb (MIT, see LICENSE-NOKOGIRI.md).

module Lobzique
  module XML
    # The search methods shared by Node and NodeSet. CSS selectors compile
    # into CSS::Selector where Nokogiri translates them into XPath strings,
    # and go through #xpath the same way (ARCHITECTURE.md).
    module Searchable
      LOOKS_LIKE_XPATH = %r{^(\./|/|\.\.|\.$)}

      def search(*args)
        paths, handler, ns, binds = extract_params(args)

        xpaths = paths.map(&:to_s).map do |path|
          LOOKS_LIKE_XPATH.match?(path) ? path : xpath_query_from_css_rule(path, ns)
        end.flatten.uniq

        xpath(*(xpaths + [ns, handler, binds].compact))
      end

      alias_method :/, :search

      def at(*args)
        search(*args).first
      end

      alias_method :%, :at

      def css(*args)
        rules, handler, ns, _ = extract_params(args)

        css_internal(self, rules, handler, ns)
      end

      def at_css(*args)
        css(*args).first
      end

      def xpath(*args)
        paths, handler, ns, binds = extract_params(args)

        xpath_internal(self, paths, handler, ns, binds)
      end

      def at_xpath(*args)
        xpath(*args).first
      end

      def >(selector) # rubocop:disable Naming/BinaryOperatorParameterName
        ns = document.root&.namespaces || {}
        # lobzique: CSS.xpath_for(selector, prefix: "./", ns: ns).first
        query = CSS::Selector.new(
          selector, prefixes: [XPath::CURRENT_SEARCH_PREFIX], doctype: :xml, namespaces: ns,
          builtins: false, first: true
        )
        xpath(query)
      end

      private

      def extract_params(params) # :nodoc:
        handler = params.find do |param|
          # lobzique: compiled CSS is a query too, where Nokogiri has a String
          ![Hash, String, Symbol, CSS::Selector].include?(param.class)
        end
        params -= [handler] if handler

        hashes = []
        while Hash === params.last || params.last.nil?
          hashes << params.pop
          break if params.empty?
        end
        ns, binds = hashes.reverse

        ns ||= document.root&.namespaces || {}

        [params, handler, ns, binds]
      end

      def css_internal(node, rules, handler, ns)
        xpath_internal(node, css_rules_to_xpath(rules, ns), handler, ns, nil)
      end

      def css_rules_to_xpath(rules, ns)
        rules.map { |rule| xpath_query_from_css_rule(rule, ns) }
      end

      # lobzique: one CSS::Selector for every implied context, where Nokogiri
      # joins an XPath string per context with " | "
      def xpath_query_from_css_rule(rule, ns)
        CSS::Selector.new(
          rule.to_s, prefixes: self.class::IMPLIED_XPATH_CONTEXTS, doctype: document.xpath_doctype,
          namespaces: ns
        )
      end

      def xpath_internal(node, paths, handler, ns, binds)
        document = node.document
        return NodeSet.new(document) unless document

        if paths.length == 1
          return xpath_impl(node, paths.first, handler, ns, binds)
        end

        NodeSet.new(document) do |combined|
          paths.each do |path|
            xpath_impl(node, path, handler, ns, binds).each { |set| combined << set }
          end
        end
      end

      def xpath_impl(node, path, handler, ns, binds)
        context = XPathContext.new(node)
        context.register_namespaces(ns)
        context.register_variables(binds)

        context.evaluate(path, handler)
      end
    end
  end
end
