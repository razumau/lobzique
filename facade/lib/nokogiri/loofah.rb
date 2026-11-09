# frozen_string_literal: true

# lobzique's own (plan.md phase 5): on the facade, Loofah's scrub! runs
# unpatched, so it would leave raw text in a <select> as html5ever keeps it.
# This adds the step Lobzique::Sanitizer's scrub! ends with
# (lobzique/sanitizer/escape_raw_text_in_select.rb). Loofah requires nokogiri
# before it defines ScrubBehavior::Node, so the step is prepended when that
# module is defined.

require "lobzique/sanitizer/escape_raw_text_in_select"

module Nokogiri
  module Facade
    module Loofah
      module ScrubBehaviorNode
        # Loofah scrubs a fragment through children.scrub!, and escaping
        # isn't idempotent, so only the outermost scrub! escapes.
        def scrub!(scrubber)
          return super if Thread.current[:lobzique_facade_scrubbing]

          begin
            Thread.current[:lobzique_facade_scrubbing] = true
            super
          ensure
            Thread.current[:lobzique_facade_scrubbing] = nil
          end
          Sanitizer::EscapeRawTextInSelect.call(self)
          self
        end
      end

      # Each hook waits for the next constant on the way to
      # Loofah::ScrubBehavior::Node.
      def self.on_const_added(name, mod, &block)
        hook = Module.new do
          define_method(:const_added) do |added|
            super(added)
            block.call(const_get(added, false)) if added == name
          end
        end
        mod.singleton_class.prepend(hook)
      end

      on_const_added(:Loofah, Object) do |loofah|
        on_const_added(:ScrubBehavior, loofah) do |scrub_behavior|
          on_const_added(:Node, scrub_behavior) { |node| node.prepend(ScrubBehaviorNode) }
        end
      end
    end
  end
end
