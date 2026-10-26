# frozen_string_literal: true

# Adapted from Nokogiri 1.19.4, lib/nokogiri/xml/syntax_error.rb (MIT, see LICENSE-NOKOGIRI.md).

module Lobzique
  module XML
    class SyntaxError < Lobzique::SyntaxError
      class << self
        def aggregate(errors)
          return nil if errors.empty?
          return errors.first if errors.length == 1

          messages = ["Multiple errors encountered:"]
          errors.each do |error|
            messages << error.to_s
          end
          new(messages.join("\n"))
        end
      end

      attr_reader :domain
      attr_reader :code
      attr_reader :level
      attr_reader :file
      attr_reader :line
      attr_reader :path
      attr_reader :str1
      attr_reader :str2
      attr_reader :str3
      attr_reader :int1
      attr_reader :column

      def none?
        level == 0
      end

      def warning?
        level == 1
      end

      def error?
        level == 2
      end

      def fatal?
        level == 3
      end

      def to_s
        message = super.chomp
        [location_to_s, level_to_s, message]
          .compact.join(": ")
          .force_encoding(message.encoding)
      end

      private

      def level_to_s
        case level
        when 3 then "FATAL"
        when 2 then "ERROR"
        when 1 then "WARNING"
        end
      end

      def nil_or_zero?(attribute)
        attribute.nil? || attribute.zero?
      end

      def location_to_s
        return if nil_or_zero?(line) && nil_or_zero?(column)

        "#{line}:#{column}"
      end
    end
  end
end
