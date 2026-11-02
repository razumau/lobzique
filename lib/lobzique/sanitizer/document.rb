# frozen_string_literal: true

# Adapted from Loofah 2.25.3, lib/loofah/html5/document.rb (MIT).

module Lobzique
  module Sanitizer
    class Document < HTML5::Document
      include ScrubBehavior::Node
      include DocumentDecorator
      # lobzique: TextBehavior isn't ported yet (concerns.rb)
      include HtmlDocumentBehavior
    end
  end
end
