# frozen_string_literal: true

# lobzique's own (plan.md phase 5): on the facade, Loofah's scrub! runs
# unpatched, so it would leave raw text in a <select> as html5ever keeps it.
# This adds the step Lobzique::Sanitizer's scrub! ends with
# (lobzique/sanitizer/escape_raw_text_in_select.rb).

require "lobzique/sanitizer/escape_raw_text_in_select"

module Nokogiri
  module Facade
    module Loofah
      module ScrubBehaviorNode
        def scrub!(scrubber)
          super
          Sanitizer::EscapeRawTextInSelect.call(self)
          self
        end
      end

      # Opens Loofah's module ahead of Loofah, which reopens it to define
      # scrub!.
      def self.patch
        node = %i[ScrubBehavior Node].inject(::Loofah) do |mod, name|
          mod.const_defined?(name, false) ? mod.const_get(name, false) : mod.const_set(name, Module.new)
        end
        node.prepend(ScrubBehaviorNode)
      end

      # Loofah requires nokogiri before it defines anything, so the patch
      # waits for the Loofah constant. The hook is gone once it has run.
      module WhenLoofahIsDefined
        def const_added(name)
          super
          return unless name == :Loofah

          WhenLoofahIsDefined.send(:remove_method, :const_added)
          Facade::Loofah.patch
        end
      end

      if Object.const_defined?(:Loofah, false)
        patch
      else
        Object.singleton_class.prepend(WhenLoofahIsDefined)
      end
    end
  end
end
