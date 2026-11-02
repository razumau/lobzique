# frozen_string_literal: true

# Adapted from Loofah 2.25.3, lib/loofah/html5/document_fragment.rb (MIT).

module Lobzique
  module Sanitizer
    class DocumentFragment < HTML5::DocumentFragment
      # lobzique: TextBehavior isn't ported yet (concerns.rb)
      include HtmlFragmentBehavior
    end
  end
end
