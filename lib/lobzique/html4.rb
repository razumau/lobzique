# frozen_string_literal: true

# Adapted from Nokogiri 1.19.4, lib/nokogiri/html4.rb (MIT, see LICENSE-NOKOGIRI.md).
# The HTML4 names keep Nokogiri's hierarchy but parse and serialize with the
# HTML5 parser (plan.md decision 3).

require_relative "html4/document"
require_relative "html4/document_fragment"
