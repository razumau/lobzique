# frozen_string_literal: true

require_relative "harness"

module Differential
  # The html4 mode (plan.md phase 5): each document case, as raw bytes,
  # parsed with the HTML4 names from a String and from an IO. Their trees
  # differ from libxml2's by design (decision 3), so what is compared is the
  # decoding: document.encoding, and for input with non-ASCII bytes how
  # often each non-ASCII character occurs in the text, which doesn't depend
  # on where the text went. ASCII input decodes to itself, and its
  # characters would only show the parsers' entities and NUL handling,
  # which are escaped everywhere.
  class HTML4Harness < Harness
    MODE = "html4"

    def observe(root, kase)
      bytes = decoding_only(kase.input.b)
      string = root::HTML4(bytes)
      io = root::HTML4(StringIO.new(bytes))
      observed = {encoding: string.encoding, io_encoding: io.encoding}
      return observed unless bytes.match?(/[\x80-\xFF]/n)

      observed.merge(characters: characters(string), io_characters: characters(io))
    rescue => e
      {error: "#{e.class.name.sub(ROOT_NAME, "")}: #{e.message}"}
    end

    def explain(result)
      kase, expected, actual = result.kase, result.expected, result.actual
      out = ["FAIL #{kase.id}"]
      out << "  input: #{truncate(kase.input.b.inspect, 200)}"
      (expected.keys | actual.keys).each do |part|
        a, b = expected[part], actual[part]
        next if a == b

        if a.is_a?(Hash) && b.is_a?(Hash)
          # only the characters whose counts differ
          differing = (a.keys | b.keys).reject { a[_1] == b[_1] }
          a, b = a.slice(*differing), b.slice(*differing)
        end
        out.concat(["  #{part}:", "    nokogiri #{truncate(a.inspect, 300)}", "    lobzique #{truncate(b.inspect, 300)}"])
      end
      out
    end

    private

    # Entities and NUL bytes escaped, so neither parser decodes them and
    # only the bytes' decoding shows in the text
    def decoding_only(bytes)
      bytes.gsub("&".b, "&amp;".b).delete("\0")
    end

    # libxml2 keeps invalid bytes raw in text; they count as U+FFFD, which
    # is what lobzique reads them as (docs/html4-invalid-bytes.md).
    def characters(doc)
      doc.xpath("//text()").map(&:text).join.scrub.scan(/[^\x00-\x7F]/).tally.sort.to_h
    end
  end
end
