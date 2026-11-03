# frozen_string_literal: true

require_relative "harness"

module Differential
  # The sanitize mode (plan.md phase 3): each case goes through Rails::HTML5's
  # sanitizers on Nokogiri and through lobzique's vendor, in the
  # configurations below, and the bytes are compared. So are what
  # sanitizing the output again gives and what parsing and serializing it
  # again gives: the properties that sanitizing is idempotent and that the
  # output survives a round trip (mutation XSS). Neither holds for every
  # case on Nokogiri either, so lobzique has to break them exactly where
  # Nokogiri does; the report counts both sides.
  class SanitizeHarness < Harness
    MODE = "sanitize"

    # name => [vendor method, new's arguments, sanitize's options]
    CONFIGS = {
      "full" => [:full_sanitizer, {}, {}],
      "link" => [:link_sanitizer, {}, {}],
      "safe_list" => [:safe_list_sanitizer, {}, {}],
      "safe_list prune" => [:safe_list_sanitizer, {prune: true}, {}],
      "safe_list foreign" => [:safe_list_sanitizer, {},
        {tags: %w[svg math style a p img use image mtext mi], attributes: %w[href src style xlink:href fill class]}],
      "safe_list structure" => [:safe_list_sanitizer, {},
        {tags: %w[table tr td th select option form input textarea template ul li div span], attributes: %w[id name type value title]}]
    }.freeze

    PROPERTIES = %i[twice reparsed].freeze

    def initialize(...)
      super
      @violations = Hash.new { |h, k| h[k] = Hash.new(0) }
    end

    def observe(root, kase)
      vendor = root.equal?(@oracle) ? Rails::HTML5::Sanitizer : root::Sanitizer
      results = CONFIGS.to_h do |name, (sanitizer, arguments, options)|
        [name, sanitize(root, vendor.public_send(sanitizer).new(**arguments), kase.input, options)]
      end
      tally_violations(root, results)
      results
    end

    def report(io = $stdout, show: 10)
      super
      io.puts "", "properties broken (results whose second sanitize or reparse differs):"
      @violations.sort_by { |root, _| root.name }.each do |root, counts|
        io.puts "  #{root.name}: " + PROPERTIES.map { |property| "#{property} #{counts[property]}" }.join(", ")
      end
    end

    def explain(result)
      kase, expected, actual = result.kase, result.expected, result.actual
      out = ["FAIL #{kase.id}"]
      out << "  #{kase.note}" if kase.note
      out << "  input: #{truncate(kase.input.b.inspect, 200)}"
      CONFIGS.each_key do |name|
        next if expected[name] == actual[name]

        [:output, *PROPERTIES].each do |part|
          a, b = expected[name][part], actual[name][part]
          next if a == b

          out << "  #{name}, #{part}:" << "    nokogiri #{truncate(a.inspect, 300)}" << "    lobzique #{truncate(b.inspect, 300)}"
          break
        end
      end
      out
    end

    private

    def sanitize(root, sanitizer, input, options)
      output = sanitizer.sanitize(input, options)
      return {output: output} if output.nil?

      {
        output: output.b,
        twice: sanitizer.sanitize(output, options).b,
        reparsed: root::HTML5::DocumentFragment.parse(output).to_html.b
      }
    rescue => e
      {output: "raises #{e.class.name.sub(ROOT_NAME, "")}: #{e.message}"}
    end

    def tally_violations(root, results)
      results.each_value do |result|
        PROPERTIES.each do |property|
          @violations[root][property] += 1 if result.key?(property) && result[property] != result[:output]
        end
      end
    end
  end
end
