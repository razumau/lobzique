# frozen_string_literal: true

# Adapted from Nokogiri 1.19.7, lib/nokogiri/css/xpath_visitor.rb (MIT, see LICENSE-NOKOGIRI.md).

module Lobzique
  module CSS
    # lobzique: the configuration only. The visit_* methods are in the core
    # (crates/lobzique-core/src/css/write.rs), which CSS::Parser#xpath_for
    # runs with this configuration, so a subclass can't override them.
    class XPathVisitor
      module BuiltinsConfig
        NEVER = :never
        ALWAYS = :always
        # lobzique: the same as ALWAYS, as in Nokogiri on libxml2
        OPTIMAL = :optimal

        VALUES = [NEVER, ALWAYS, OPTIMAL]
      end

      module DoctypeConfig
        XML = :xml
        HTML4 = :html4
        HTML5 = :html5

        VALUES = [XML, HTML4, HTML5]
      end

      attr_reader :builtins

      attr_reader :doctype

      attr_reader :prefix

      attr_reader :namespaces

      def initialize(
        builtins: BuiltinsConfig::NEVER,
        doctype: DoctypeConfig::XML,
        prefix: XML::XPath::GLOBAL_SEARCH_PREFIX,
        namespaces: nil
      )
        unless BuiltinsConfig::VALUES.include?(builtins)
          raise(ArgumentError, "Invalid values #{builtins.inspect} for builtins: keyword parameter")
        end
        unless DoctypeConfig::VALUES.include?(doctype)
          raise(ArgumentError, "Invalid values #{doctype.inspect} for doctype: keyword parameter")
        end

        @builtins = builtins
        @doctype = doctype
        @prefix = prefix
        @namespaces = namespaces
      end

      def config
        {builtins: @builtins, doctype: @doctype, prefix: @prefix, namespaces: @namespaces}
      end
    end
  end
end
