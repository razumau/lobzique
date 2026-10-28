# frozen_string_literal: true

# Adapted from Nokogiri 1.19.4, lib/nokogiri/xml.rb (MIT, see LICENSE-NOKOGIRI.md).
# XML parsing waits for plan.md decision 4; the XML classes are here because
# the HTML ones inherit from them.

require_relative "xml/pp"
require_relative "xml/parse_options"
require_relative "xml/searchable"
require_relative "xml/node"
require_relative "xml/character_data"
require_relative "xml/namespace"
require_relative "xml/attr"
require_relative "xml/dtd"
require_relative "xml/cdata"
require_relative "xml/text"
require_relative "xml/document"
require_relative "xml/document_fragment"
require_relative "xml/node_set"
require_relative "xml/syntax_error"
require_relative "xml/xpath"
require_relative "xml/xpath_context"
