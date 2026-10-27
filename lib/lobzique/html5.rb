# frozen_string_literal: true

# Adapted from Nokogiri 1.19.4, lib/nokogiri/html5.rb (MIT, see LICENSE-NOKOGIRI.md).

require_relative "html5/document"
require_relative "html5/document_fragment"
require_relative "html5/node"

module Lobzique
  def self.HTML5(...)
    HTML5::Document.parse(...)
  end

  module HTML5
    class << self
      def parse(...)
        Document.parse(...)
      end

      def fragment(...)
        DocumentFragment.parse(...)
      end

      # Reads an IO or String and returns it as UTF-8, which the parser takes.
      def read_and_encode(string, encoding)
        if string.respond_to?(:read)
          string = if encoding.nil?
            string.read
          else
            string.read(encoding: encoding)
          end
        else
          string = string.to_s
          if encoding
            string = string.dup
            string.force_encoding(encoding)
          end
        end

        if string.encoding != Encoding::UTF_8
          string = reencode(string)
        end
        string
      end

      private

      # A binary string's encoding comes from its byte order mark, the
      # content type, or a `<meta charset>` in its first 1024 bytes, falling
      # back to ISO-8859-1.
      def reencode(body, content_type = nil)
        if body.encoding == Encoding::ASCII_8BIT
          encoding = nil

          initial_bytes = body[0..2].bytes
          if initial_bytes[0..2] == [0xEF, 0xBB, 0xBF]
            encoding = Encoding::UTF_8
          elsif initial_bytes[0..1] == [0xFE, 0xFF]
            encoding = Encoding::UTF_16BE
          elsif initial_bytes[0..1] == [0xFF, 0xFE]
            encoding = Encoding::UTF_16LE
          end

          if content_type
            encoding ||= content_type[/charset=["']?(.*?)($|["';\s])/i, 1]
          end

          unless encoding
            data = body[0..1023].gsub(/<!--.*?(-->|\Z)/m, "")
            data.scan(/<meta.*?>/im).each do |meta|
              encoding ||= meta[/charset=["']?([^>]*?)($|["'\s>])/im, 1]
            end
          end

          encoding ||= Encoding::ISO_8859_1

          body = body.dup
          begin
            body.force_encoding(encoding)
          rescue ArgumentError
            body.force_encoding(Encoding::ISO_8859_1)
          end
        end

        body.encode(Encoding::UTF_8)
      end
    end
  end
end

require_relative "html5ever" # lobzique: Nokogiri's "gumbo"
