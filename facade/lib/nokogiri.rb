# frozen_string_literal: true

# Adapted from Nokogiri 1.19.7, lib/nokogiri.rb (MIT, see LICENSE-NOKOGIRI.md).
# lobzique: Nokogiri's classes are lobzique's (plan.md decision 1). The module
# is named Nokogiri first, so every class reports Nokogiri::…, and lobzique
# reopens it through the Lobzique constant. What lobzique leaves out raises,
# saying so (nokogiri/unsupported.rb).

module Nokogiri
  class << self
    ###
    # Parse an HTML or XML document.  +string+ contains the document.
    def parse(string, url = nil, encoding = nil, options = nil)
      if string.respond_to?(:read) ||
          /^\s*<(?:!DOCTYPE\s+)?html[\s>]/i.match?(string[0, 512])
        # Expect an HTML indicator to appear within the first 512
        # characters of a document. (<?xml ?> + <?xml-stylesheet ?>
        # shouldn't be that long)
        Nokogiri.HTML4(
          string,
          url,
          encoding,
          options || XML::ParseOptions::DEFAULT_HTML
        )
      else
        Nokogiri.XML(
          string,
          url,
          encoding,
          options || XML::ParseOptions::DEFAULT_XML
        )
      end.tap do |doc|
        yield doc if block_given?
      end
    end

    ###
    # Create a new Nokogiri::XML::DocumentFragment
    def make(input = nil, opts = {}, &blk)
      if input
        Nokogiri::HTML4.fragment(input).children.first
      else
        Nokogiri(&blk)
      end
    end
  end
end

###
# Parse a document contained in +args+.  Nokogiri will try to guess what type of document you are
# attempting to parse.  For more information, see Nokogiri.parse
#
# To specify the type of document, use {Nokogiri.XML}, {Nokogiri.HTML4}, or {Nokogiri.HTML5}.
def Nokogiri(*args, &block)
  if block
    # lobzique: there is no HTML4::Builder
    raise NotImplementedError, Nokogiri::Facade::Unsupported.message("Nokogiri() with a block", "no Builder")
  else
    Nokogiri.parse(*args)
  end
end

if !defined?(::Lobzique)
  ::Lobzique = Nokogiri
elsif !::Lobzique.equal?(Nokogiri)
  raise LoadError, "lobzique was loaded before the nokogiri facade; require nokogiri first, " \
    "or use lobzique's seams with the real nokogiri gem (README)"
end

require_relative "nokogiri/version"
require_relative "nokogiri/unsupported"
require_relative "nokogiri/loofah"
