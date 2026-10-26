# frozen_string_literal: true

# Adapted from Nokogiri 1.19.4, lib/nokogiri/xml/parse_options.rb (MIT, see LICENSE-NOKOGIRI.md).
# libxml2's parser flags. The HTML5 parser ignores them, but `Node#parse`
# and the fragment initializers take and pass them on, as in Nokogiri.

module Lobzique
  module XML
    class ParseOptions
      STRICT = 0
      RECOVER = 1 << 0
      NOENT = 1 << 1
      DTDLOAD = 1 << 2
      DTDATTR = 1 << 3
      DTDVALID = 1 << 4
      NOERROR = 1 << 5
      NOWARNING = 1 << 6
      PEDANTIC = 1 << 7
      NOBLANKS = 1 << 8
      SAX1 = 1 << 9
      XINCLUDE = 1 << 10
      NONET = 1 << 11
      NODICT = 1 << 12
      NSCLEAN = 1 << 13
      NOCDATA = 1 << 14
      NOXINCNODE = 1 << 15
      COMPACT = 1 << 16
      OLD10 = 1 << 17
      NOBASEFIX = 1 << 18
      HUGE = 1 << 19
      BIG_LINES = 1 << 22

      DEFAULT_XML = RECOVER | NONET | BIG_LINES
      DEFAULT_XSLT = RECOVER | NONET | NOENT | DTDLOAD | DTDATTR | NOCDATA | BIG_LINES
      DEFAULT_HTML = RECOVER | NOERROR | NOWARNING | NONET | BIG_LINES
      DEFAULT_SCHEMA = NONET | BIG_LINES

      attr_accessor :options

      def initialize(options = STRICT)
        @options = options
      end

      constants.each do |constant|
        next if constant.to_sym == :STRICT

        class_eval <<~RUBY, __FILE__, __LINE__ + 1
          def #{constant.downcase}
            @options |= #{constant}
            self
          end

          def no#{constant.downcase}
            @options &= ~#{constant}
            self
          end

          def #{constant.downcase}?
            #{constant} & @options == #{constant}
          end
        RUBY
      end

      def strict
        @options &= ~RECOVER
        self
      end

      def strict?
        @options & RECOVER == STRICT
      end

      def ==(other)
        other.to_i == to_i
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
