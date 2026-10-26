# frozen_string_literal: true

# Adapted from Nokogiri 1.19.4, lib/nokogiri/xml/node/save_options.rb (MIT, see LICENSE-NOKOGIRI.md).

module Lobzique
  module XML
    class Node
      class SaveOptions
        FORMAT = 1
        NO_DECLARATION = 2
        NO_EMPTY_TAGS = 4
        NO_XHTML = 8
        AS_XHTML = 16
        AS_XML = 32
        AS_HTML = 64

        DEFAULT_XML = FORMAT | AS_XML
        DEFAULT_HTML = FORMAT | NO_DECLARATION | NO_EMPTY_TAGS | AS_HTML
        DEFAULT_XHTML = FORMAT | NO_DECLARATION | AS_XHTML

        attr_reader :options

        def initialize(options = 0)
          @options = options
        end

        constants.each do |constant|
          class_eval <<~RUBY, __FILE__, __LINE__ + 1
            def #{constant.downcase}
              @options |= #{constant}
              self
            end

            def #{constant.downcase}?
              #{constant} & @options == #{constant}
            end
          RUBY
        end

        alias_method :to_i, :options

        def inspect
          options = []
          self.class.constants.each do |k|
            options << k.downcase if send(:"#{k.downcase}?")
          end
          super.sub(/>$/, " " + options.join(", ") + ">")
        end
      end
    end
  end
end
