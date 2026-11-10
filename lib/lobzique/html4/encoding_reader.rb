# frozen_string_literal: true

# Adapted from Nokogiri 1.19.7, lib/nokogiri/html4/encoding_reader.rb (MIT, see LICENSE-NOKOGIRI.md).
# lobzique: plus EncodingReader.decode, which decodes input as libxml2 2.13
# does before the HTML5 parser reads it (plan.md decision 3).

module Lobzique
  module HTML4
    # Libxml2's parser has poor support for encoding detection.  First, it does not recognize the
    # HTML5 style meta charset declaration.  Secondly, even if it successfully detects an encoding
    # hint, it does not re-decode or re-parse the preceding part which may be garbled.
    #
    # EncodingReader aims to perform advanced encoding detection beyond what Libxml2 does, and to
    # emulate rewinding of a stream and make Libxml2 redo parsing from the start when an encoding
    # hint is found.

    # :nodoc: all
    class EncodingReader
      class EncodingFound < StandardError
        attr_reader :found_encoding

        def initialize(encoding)
          @found_encoding = encoding
          super(format("encoding found: %s", encoding))
        end
      end

      # lobzique: SAXHandler and JumpSAXHandler are left out with SAX;
      # meta_charset reads what SAXHandler#start_element records.

      def self.detect_encoding(chunk)
        # lobzique: Nokogiri reads the declaration with Nokogiri.XML, which
        # is out of scope (plan.md decision 4)
        (m = chunk.match(/\A(<\?xml[ \t\r\n][^>]*>)/)) &&
          (return m[1].match(/[ \t\r\n]encoding[ \t\r\n]*=[ \t\r\n]*(?:"([^"]*)"|'([^']*)')/)&.captures&.compact&.first)

        meta_charset(chunk)
      end

      # lobzique: Nokogiri pushes the chunk through libxml2's HTML4 SAX
      # parser and keeps the charset of the last <meta> that has one. This
      # parses it with the HTML5 parser, read as ISO-8859-1 so any bytes do,
      # and walks the <meta> elements in document order.
      def self.meta_charset(chunk)
        return unless chunk.b.match?(/<meta/i)

        metas(chunk).filter_map do |meta|
          content = meta["content"] if meta["http-equiv"]&.match?(/\AContent-Type\z/i)
          content&.[](/;\s*charset\s*=\s*([\w-]+)/, 1) || meta["charset"]
        end.last
      end

      # The <meta> elements of bytes in any encoding, read as ISO-8859-1.
      def self.metas(bytes)
        html = bytes.b.force_encoding(Encoding::ISO_8859_1).encode(Encoding::UTF_8)
        doc = Html5ever.parse(
          html, nil, Document,
          max_attributes: Html5ever::DEFAULT_MAX_ATTRIBUTES,
          max_errors: 0,
          max_tree_depth: Html5ever::DEFAULT_MAX_TREE_DEPTH
        )
        doc.xpath("//meta")
      end
      private_class_method :meta_charset, :metas

      def initialize(io)
        @io = io
        @firstchunk = nil
        @encoding_found = nil
      end

      # This method is used by the C extension so that
      # Nokogiri::HTML4::Document#read_io() does not leak memory when
      # EncodingFound is raised.
      attr_reader :encoding_found

      def read(len)
        # no support for a call without len

        unless @firstchunk
          (@firstchunk = @io.read(len)) || return

          # This implementation expects that the first call from
          # htmlReadIO() is made with a length long enough (~1KB) to
          # achieve advanced encoding detection.
          if (encoding = EncodingReader.detect_encoding(@firstchunk))
            # The first chunk is stored for the next read in retry.
            raise @encoding_found = EncodingFound.new(encoding)
          end
        end
        @encoding_found = nil

        ret = @firstchunk.slice!(0, len)
        if (len -= ret.length) > 0
          (rest = @io.read(len)) && ret << rest
        end
        if ret.empty?
          nil
        else
          ret
        end
      end

      # lobzique: from here on, lobzique's own.

      # What becomes of bytes that are invalid in the input's encoding:
      # :replacement (U+FFFD) or :latin1 (each byte read as ISO-8859-1).
      # Open, see docs/html4-invalid-bytes.md.
      INVALID_BYTES = :replacement

      # libxml2's built-in decoders, which give document.encoding their own
      # names; iconv's keep the name as given.
      BUILT_IN = {
        "UTF-16LE" => Encoding::UTF_16LE,
        "UTF-16BE" => Encoding::UTF_16BE,
        "UTF-16" => Encoding::UTF_16LE,
        "ISO-8859-1" => Encoding::ISO_8859_1,
        "ASCII" => Encoding::US_ASCII,
        "US-ASCII" => Encoding::US_ASCII
      }.freeze

      # xmlParseCharEncoding's canonical names, the fallback for a name
      # neither a built-in decoder nor iconv takes.
      CANONICAL = {
        "ISO-LATIN-1" => "ISO-8859-1", "ISO LATIN 1" => "ISO-8859-1",
        "ISO-LATIN-2" => "ISO-8859-2", "ISO LATIN 2" => "ISO-8859-2",
        "ISO-10646-UCS-2" => "UCS-2", "UCS2" => "UCS-2",
        "ISO-10646-UCS-4" => "UCS-4", "UCS4" => "UCS-4"
      }.freeze

      # Names iconv knows and Ruby doesn't, in their usual spellings.
      ICONV = {
        /\AISO[_-]?8859[_-](\d+)(?::\d+)?\z/i => ->(m) { "ISO-8859-#{m[1]}" },
        /\ALATIN-?([1-9]|10)\z/i => ->(m) { "ISO-8859-#{%w[1 2 3 4 9 10 13 14 15 16][m[1].to_i - 1]}" },
        /\AUTF-?(16|32)(LE|BE)?\z/i => ->(m) { "UTF-#{m[1]}#{m[2]&.upcase || "BE"}" },
        /\AUCS-2\z/i => ->(_) { "UTF-16BE" },
        /\AUCS-4\z/i => ->(_) { "UTF-32BE" },
        /\A(?:MAC|MACINTOSH|MACROMAN)\z/i => ->(_) { "macRoman" },
        /\A(?:SHIFT-JIS|MS_KANJI|CSSHIFTJIS)\z/i => ->(_) { "Shift_JIS" },
        /\AWINDOWS-?(\d+)\z/i => ->(m) { "Windows-#{m[1]}" },
        # libiconv's is the bare KS X 1001 set; pages that declare it are
        # EUC-KR or its superset, as browsers read them
        /\A(?:KS_C_5601-198[79]|KSC_5601|CSKSC56011987)\z/i => ->(_) { "CP949" }
      }.freeze

      BOMS = {
        "\xEF\xBB\xBF".b => [Encoding::UTF_8, 3],
        "\xFE\xFF".b => [Encoding::UTF_16BE, 2],
        "\xFF\xFE".b => [Encoding::UTF_16LE, 2],
        "\x3C\x00\x3F\x00".b => [Encoding::UTF_16LE, 0],
        "\x00\x3C\x00\x3F".b => [Encoding::UTF_16BE, 0]
      }.freeze

      # Names Encoding.find resolves to whatever the process is set up with.
      RUBY_ONLY = %w[locale external internal filesystem].freeze

      private_constant :INVALID_BYTES, :BUILT_IN, :CANONICAL, :ICONV, :BOMS, :RUBY_ONLY

      class << self
        # Input bytes as libxml2's HTML parser decodes them with +encoding+
        # (a name, or nil): [UTF-8 string, document.encoding, errors]. Input
        # read from an IO in +chunk+-sized reads is only partly buffered
        # when libxml2 guesses.
        def decode(input, encoding, chunk: nil)
          bytes = input.to_str.b
          errors = []
          if encoding
            ruby_encoding, name = open_decoder(encoding)
            return decode_with(bytes, ruby_encoding, name, errors) if ruby_encoding

            errors << syntax_error("Unsupported encoding: #{encoding}", domain: 1, code: 32, level: 1, str1: encoding)
          end
          guess(bytes, errors, chunk)
        end

        private

        # No encoding: a byte order mark; UTF-8 if the input starts with an
        # XML declaration; a <meta> the parser meets before any non-ASCII
        # byte; else at the first non-ASCII byte an http-equiv charset
        # anywhere after it (htmlFindEncoding), or ISO-8859-1. ASCII input
        # has no encoding.
        def guess(bytes, errors, chunk)
          bom = BOMS.keys.find { |mark| bytes.start_with?(mark) } if bytes.bytesize >= 4
          if bom
            ruby_encoding, size = BOMS[bom]
            return [transcode(bytes.byteslice(size..).force_encoding(ruby_encoding), bytes, errors), nil, errors]
          end

          # htmlParseDocument: "wrong but matches long-standing behavior"
          return decode_with(bytes, Encoding::UTF_8, "UTF-8", errors) if bytes.start_with?("<?xm")

          first = bytes.index(/[\x80-\xFF]/n)
          declared = declared_while_parsing(bytes, first, errors)
          return declared if declared
          return [bytes.force_encoding(Encoding::UTF_8), nil, errors] unless first

          declared = find_encoding(bytes, first, chunk)
          return decode_with(bytes, Encoding::ISO_8859_1, "ISO-8859-1", errors) unless declared

          ruby_encoding, name = open_decoder(declared)
          return decode_with(bytes, ruby_encoding, name, errors) if ruby_encoding

          line, column = position(bytes, first)
          errors << syntax_error("Unsupported encoding: #{declared}", domain: 1, code: 32, level: 1, str1: declared, line:, column:)
          # libxml2 has stopped guessing, so the input is UTF-8
          decode_with(bytes, Encoding::UTF_8, "UTF-8", errors)
        end

        # htmlCheckMeta, which EncodingReader misses when its regexp doesn't
        # match or the <meta> is past an IO's first chunk: the first
        # supported declaration before the first non-ASCII byte decodes the
        # input, and document.encoding is the last one declared, as written.
        def declared_while_parsing(bytes, first, errors)
          prefix = first ? bytes.byteslice(0, first) : bytes
          return unless prefix.match?(/<meta/ni)

          declared = declarations(prefix)
          switch = nil
          declared.each do |name|
            break if (switch = open_decoder(name))

            errors << syntax_error("Unsupported encoding: #{name}", domain: 1, code: 32, level: 3, str1: name)
          end
          return unless switch

          declared = declarations(bytes) if first
          decode_with(bytes, switch.first, declared.last, errors)
        end

        # What htmlCheckMeta declares, meta by meta: a charset attribute's
        # value, then an http-equiv Content-Type's charset.
        def declarations(bytes)
          metas(bytes).flat_map do |meta|
            http_equiv = content = nil
            charsets = meta.attribute_nodes.filter_map do |attribute|
              name, value = attribute.name, attribute.value
              if name.casecmp?("http-equiv") && value.casecmp?("Content-Type")
                http_equiv = true
                nil
              elsif name.casecmp?("content")
                content = value
                nil
              elsif name.casecmp?("charset")
                value
              end
            end
            charset = http_equiv && content && charset_in_content(content)
            # back to the input's bytes; a character reference's are lost
            (charset ? charsets << charset : charsets).map { _1.encode(Encoding::ISO_8859_1, undef: :replace).b }
          end
        end

        # htmlCheckEncoding: everything after the "=" that follows
        # "charset", or after the first "=" if a blank follows "charset".
        def charset_in_content(content)
          at = content.index(/charset/i) or return
          at += 7
          at = content.index("=") if content[at]&.match?(/[ \t\r\n]/)
          content[(at + 1)..] if at && content[at] == "="
        end

        # htmlFindEncoding: a case-insensitive search of the raw bytes up to
        # the first NUL, comments and text included, as far as they're
        # buffered: htmlCurrentChar keeps INPUT_CHUNK (250) bytes ahead.
        def find_encoding(bytes, from, chunk)
          rest = chunk ? bytes.byteslice(from...((from + 250).fdiv(chunk).ceil * chunk)) : bytes.byteslice(from..)
          rest = rest.byteslice(0, rest.index("\0")) if rest.include?("\0")
          (at = rest.index(/HTTP-EQUIV/ni)) &&
            (at = rest.index(/CONTENT/ni, at)) &&
            (at = rest.index(/CHARSET=/ni, at)) &&
            rest.byteslice((at + 8)..)[/\A[A-Za-z0-9\-_:\/]+/n]
        end

        def decode_with(bytes, ruby_encoding, name, errors)
          string = transcode(bytes.dup.force_encoding(ruby_encoding), bytes, errors)
          # xmlDetectEncoding skips a UTF-8 BOM even when the encoding is set
          string = string.byteslice(3..) if string.start_with?("\uFEFF")
          [string, name, errors]
        end

        def transcode(string, bytes, errors)
          invalid_bytes(bytes, string, errors) unless string.valid_encoding?
          if string.encoding != Encoding::UTF_8
            # invalid: always, as a stateful encoding's string (ISO-2022-JP)
            # is valid_encoding? whatever its bytes
            string.encode(Encoding::UTF_8, invalid: :replace, undef: :replace)
          elsif INVALID_BYTES == :latin1
            string.scrub { |b| b.unpack("C*").pack("U*") }
          else
            string.scrub
          end
        end

        def invalid_bytes(bytes, string, errors)
          offset = string.each_char.take_while(&:valid_encoding?).sum(&:bytesize)
          line, column = position(bytes, offset)
          errors << syntax_error("Invalid bytes in character encoding", domain: 8, code: 81, level: 3, line:, column:)
        end

        def position(bytes, offset)
          before = bytes.byteslice(0, offset)
          [before.count("\n") + 1, offset - (before.rindex("\n") || -1)]
        end

        # xmlOpenCharEncodingHandler: [Ruby encoding, document.encoding], or
        # nil for a name libxml2 doesn't support.
        def open_decoder(name)
          return [Encoding::UTF_8, "UTF-8"] if name.casecmp?("UTF-8") || name.casecmp?("UTF8")

          if (ruby_encoding = BUILT_IN[built_in = name.upcase])
            return [ruby_encoding, built_in]
          end
          if (ruby_encoding = iconv(name))
            return [ruby_encoding, name]
          end

          canonical = CANONICAL[name.upcase]
          return unless canonical

          open_decoder(canonical)
        end

        # A decoder iconv would open, if Ruby can transcode from it. Ruby's
        # names for UTF-8 aren't iconv's, and UTF-8 itself is built in.
        def iconv(name)
          return if RUBY_ONLY.any? { _1.casecmp?(name) }

          candidates = ICONV.filter_map { |pattern, rename| (m = pattern.match(name)) && rename.call(m) } << name
          candidates.each do |candidate|
            encoding = Encoding.find(candidate)
            next if encoding == Encoding::BINARY

            Encoding::Converter.new(encoding, Encoding::UTF_8)
            return encoding
          rescue ArgumentError, Encoding::ConverterNotFoundError
            next
          end
          nil
        end

        def syntax_error(message, domain:, code:, level:, str1: nil, line: 0, column: 0)
          XML::SyntaxError.new(message).tap do |error|
            {domain:, code:, level:, file: nil, line:, str1:, str2: nil, str3: nil, int1: 0, column:}.each do |name, value|
              error.instance_variable_set(:"@#{name}", value)
            end
          end
        end
      end
    end
  end
end
