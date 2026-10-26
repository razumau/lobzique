# frozen_string_literal: true

require "zlib"

module Differential
  # Reads the HTML responses out of a gzipped WARC file, as Common Crawl
  # publishes them: one gzip member per record. Stdlib only, nothing from the
  # file is evaluated.
  module Warc
    Response = Data.define(:offset, :uri, :body)

    module_function

    # Yields [offset, compressed size, record bytes] for each complete gzip
    # member. A member cut off at the end of the file (a byte-range download)
    # ends the iteration.
    def each_member(io)
      return enum_for(:each_member, io) unless block_given?

      until io.eof?
        offset = io.pos
        gz = Zlib::GzipReader.new(io)
        begin
          data = gz.read
        rescue Zlib::Error, EOFError
          return
        end
        # GzipReader reads ahead; rewind to the start of the next member
        unused = gz.unused
        io.pos -= unused.bytesize if unused
        gz.finish
        yield offset, io.pos - offset, data
      end
    end

    # HTML responses with HTTP status 200.
    def each_html_response(path)
      return enum_for(:each_html_response, path) unless block_given?

      File.open(path, "rb") do |io|
        each_member(io) do |offset, _size, data|
          warc_headers, block = split_headers(data)
          next unless warc_headers["warc-type"] == "response"

          http_headers, body = split_headers(block)
          status = http_headers[:start_line].to_s.split(" ", 3)[1]
          type = warc_headers["warc-identified-payload-type"] || http_headers["content-type"]
          next unless status == "200" && type.to_s.start_with?("text/html")

          yield Response.new(offset: offset, uri: warc_headers["warc-target-uri"], body: body)
        end
      end
    end

    # Splits "start line, headers, blank line, rest" into a header hash
    # (lowercase names, plus :start_line) and the rest.
    def split_headers(data)
      head, rest = data.b.split("\r\n\r\n", 2)
      start_line, *lines = head.split("\r\n")
      headers = {start_line: start_line}
      lines.each do |line|
        name, value = line.split(":", 2)
        headers[name.strip.downcase] = value.to_s.strip if value
      end
      [headers, rest.to_s]
    end
  end
end
