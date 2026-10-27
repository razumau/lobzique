# frozen_string_literal: true

require "json"
require "yaml"
require_relative "warc"

module Differential
  # One input to parse with both implementations. `context` is nil for a
  # document, or a fragment context in Nokogiri's string form ("body",
  # "svg:path").
  Case = Data.define(:id, :source, :kind, :input, :context, :note) do
    def initialize(id:, source:, kind:, input:, context: nil, note: nil) = super
  end

  # Inputs for the differential harness (plan.md decision 6). Everything is
  # read as data; nothing from the fixtures or the crawl is evaluated.
  module Corpus
    ROOT = File.expand_path("../..", __dir__)
    FIXTURES = File.join(ROOT, "test", "fixtures")
    CASES = File.join(__dir__, "cases.yml")
    CRAWL_PIN = File.join(__dir__, "crawl.yml")
    CRAWL_DIR = File.join(ROOT, "tmp", "crawl")

    # Section headers of the html5lib .dat format and WPT's extensions to it
    DAT_HEADERS = %w[#data #errors #new-errors #document-fragment #script-off
      #script-on #document #config #description #method #error].freeze

    # Stands in for {{host}} in the sanitizer tests' .sub.dat files
    WPT_HOST = "web-platform.test"

    module_function

    def names = %w[fixtures crawl]

    def each(name, &block)
      return enum_for(:each, name) unless block

      case name
      when "fixtures" then each_fixture(&block)
      when "crawl" then each_crawl(&block)
      else raise ArgumentError, "unknown corpus #{name.inspect} (known: #{names.join(", ")})"
      end
    end

    def each_fixture(&block)
      each_case(&block)
      fixture_glob("wpt-parsing/tree-construction/*.dat").each { |path| each_dat(path, "wpt-parsing", &block) }
      fixture_glob("wpt-sanitizer-api/*.dat").each { |path| each_dat(path, "wpt-sanitizer-api", &block) }
      fixture_glob("html5lib-tests/tokenizer/*.test").each { |path| each_tokenizer(path, &block) }
      each_dompurify(&block)
      %w[wpt-parsing/serializing-reference/*.html loofah/assets/*.html nokogiri/files/*.html].each do |glob|
        fixture_glob(glob).each do |path|
          yield Case.new(id: relative(path), source: glob[%r{\A[^/]+}], kind: :document, input: File.binread(path))
        end
      end
    end

    def each_case
      YAML.safe_load_file(CASES).each do |test|
        context = test["context"]
        yield Case.new(id: "cases/#{test.fetch("id")}", source: "cases",
          kind: context ? :fragment : :document, input: test.fetch("input"), context: context, note: test["note"])
      end
    end

    # html5lib tree-construction format: a test starts at "#data"; the data
    # runs to the next section header, without its final newline.
    def each_dat(path, source)
      rel = relative(path)
      text = File.read(path, encoding: "UTF-8")
      text = text.gsub("{{host}}", WPT_HOST) if path.end_with?(".sub.dat")
      tests = []
      section = nil
      text.each_line(chomp: true) do |line|
        if DAT_HEADERS.include?(line)
          section = line
          tests << {} if line == "#data"
          tests.last&.[]=(line, [])
        elsif section && tests.last
          tests.last[section] << line
        end
      end

      tests.each_with_index do |test, index|
        context = test["#document-fragment"]&.first&.sub(" ", ":")
        yield Case.new(id: "#{rel}##{index + 1}", source: source,
          kind: context ? :fragment : :document, input: test.fetch("#data").join("\n"), context: context)
      end
    end

    def each_tokenizer(path)
      rel = relative(path)
      data = JSON.parse(File.read(path))
      (data["tests"] || data["xmlViolationTests"] || []).each_with_index do |test, index|
        input = test.fetch("input")
        input = unescape(input) if test["doubleEscaped"]
        next unless input.valid_encoding? # lone surrogates can't be a Ruby UTF-8 string

        yield Case.new(id: "#{rel}##{index + 1}", source: "html5lib-tests", kind: :document, input: input)
      end
    end

    # Sanitizers parse fragments, so the payloads go in as fragments
    def each_dompurify
      path = File.join(FIXTURES, "dompurify", "expect.json")
      JSON.parse(File.read(path)).each_with_index do |test, index|
        yield Case.new(id: "dompurify/expect.json##{index + 1}", source: "dompurify",
          kind: :fragment, input: test.fetch("payload"), context: nil, note: test["title"])
      end
    end

    # Pages from the pinned Common Crawl sample, as raw bytes: the parser sniffs
    # the encoding, as it would for an uploaded file. script/fetch_crawl
    # downloads them.
    def each_crawl
      crawl_files.each do |file|
        raise "#{file} is missing; run script/fetch_crawl" unless File.exist?(file)

        Warc.each_html_response(file) do |response|
          yield Case.new(id: "crawl/#{File.basename(file)}@#{response.offset}", source: "crawl",
            kind: :document, input: response.body, note: response.uri)
        end
      end
    end

    def crawl_files
      pin = YAML.safe_load_file(CRAWL_PIN)
      pin.fetch("files").map { |f| File.join(CRAWL_DIR, File.basename(f.fetch("path"))) }
    end

    def fixture_glob(pattern) = Dir.glob(File.join(FIXTURES, pattern)).sort
    def relative(path) = path.delete_prefix("#{FIXTURES}/")

    # The tokenizer tests' doubleEscaped inputs spell code points as \uXXXX
    def unescape(string)
      units = []
      string.scan(/\\u(\h{4})|(.)/m) { |hex, char| units.concat(hex ? [hex.hex] : char.encode("UTF-16BE").unpack("n*")) }
      units.pack("n*").force_encoding("UTF-16BE").encode("UTF-8")
    rescue EncodingError
      +"\xFF" # invalid on purpose; the caller skips it
    end
  end
end
