# frozen_string_literal: true

# Adapted from Nokogiri 1.19.7, lib/nokogiri/css/selector_cache.rb (MIT, see LICENSE-NOKOGIRI.md).

module Lobzique
  module CSS
    module SelectorCache # :nodoc:
      @cache = {}
      @mutex = Mutex.new

      class << self
        def [](key)
          @mutex.synchronize { @cache[key] }
        end

        def []=(key, value)
          @mutex.synchronize { @cache[key] = value }
        end

        def clear_cache(create_new_object = false)
          @mutex.synchronize do
            if create_new_object
              @cache = {}
            else
              @cache.clear
            end
          end
        end

        def key(selector:, visitor:)
          [selector, visitor.config]
        end
      end
    end
  end
end
