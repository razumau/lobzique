# frozen_string_literal: true

require "yaml"
require_relative "corpus"

module Differential
  # Parses each corpus case with Nokogiri and with Lobzique and compares the
  # serialized HTML and a tree dump (plan.md decision 6). Both sides run the
  # same code against their root module, since Lobzique mirrors Nokogiri's
  # class tree.
  class Harness
    KNOWN = File.join(__dir__, "known.yml")
    ROOT_NAME = /\A(Nokogiri|Lobzique)::/

    Result = Data.define(:kase, :expected, :actual, :known)

    attr_reader :counts, :failures, :stale

    def initialize(implementation:, oracle: Nokogiri, known: self.class.load_known)
      @oracle = oracle
      @implementation = implementation
      @known = known
      @counts = Hash.new { |h, k| h[k] = Hash.new(0) }
      @failures = []
      @stale = []
    end

    def self.load_known
      return [] unless File.exist?(KNOWN)

      (YAML.safe_load_file(KNOWN) || []).map do |entry|
        {pattern: Regexp.new(entry.fetch("match")), reason: entry.fetch("reason")}
      end
    end

    def run(cases)
      cases.each do |kase|
        expected = observe(@oracle, kase)
        actual = @implementation ? observe(@implementation, kase) : expected
        known = @known.find { |k| k[:pattern].match?(kase.id) }
        tally = @counts[kase.source]
        if expected == actual
          tally[:pass] += 1
          @stale << [kase, known] if known
        elsif known
          tally[:known] += 1
        else
          tally[:fail] += 1
          @failures << Result.new(kase: kase, expected: expected, actual: actual, known: known)
        end
      end
      self
    end

    def ok? = @failures.empty?

    # What a caller can see of one parse: the serialized HTML (with its
    # encoding) and the tree, or the error raised.
    def observe(root, kase)
      node = if kase.kind == :document
        root::HTML5.parse(kase.input)
      else
        root::HTML5::DocumentFragment.parse(kase.input, context: kase.context || "body")
      end
      html = node.to_html
      {html: html.b, encoding: html.encoding.name, tree: dump(root, node)}
    rescue => e
      {error: "#{e.class.name.sub(ROOT_NAME, "")}: #{e.message}"}
    end

    # html5lib-style tree dump, in document order and without recursion, so a
    # tree at the depth limit can't overflow the stack.
    def dump(root, top)
      lines = []
      stack = top.children.to_a.reverse.map { |n| [n, 0] }
      until stack.empty?
        node, depth = stack.pop
        indent = "| " + "  " * depth
        lines << indent + describe(root, node)
        if node.element?
          node.attribute_nodes.each do |attr|
            name = [attr.namespace&.prefix, attr.name].compact.join(" ")
            lines << "#{indent}  #{name}=#{attr.value.inspect}"
          end
        end
        node.children.to_a.reverse_each { |child| stack << [child, depth + 1] }
      end
      lines
    end

    def describe(root, node)
      if node.element?
        "<#{[node.namespace&.href, node.name].compact.join(" ")}>"
      elsif node.text? || node.cdata?
        node.content.inspect
      elsif node.comment?
        "<!-- #{node.content} -->"
      elsif node.type == root::XML::Node::DTD_NODE
        "<!DOCTYPE #{node.name}>"
      else
        "type=#{node.type} #{node.name}"
      end
    end

    def report(io = $stdout, show: 10)
      width = [@counts.keys.map(&:length).max.to_i, 6].max
      io.puts format("%-#{width}s %7s %7s %7s", "source", "pass", "known", "fail")
      totals = Hash.new(0)
      @counts.sort.each do |source, tally|
        io.puts format("%-#{width}s %7d %7d %7d", source, tally[:pass], tally[:known], tally[:fail])
        tally.each { |k, v| totals[k] += v }
      end
      io.puts format("%-#{width}s %7d %7d %7d", "total", totals[:pass], totals[:known], totals[:fail])

      @failures.first(show).each { |result| io.puts "", *explain(result) }
      io.puts "", "... #{@failures.size - show} more failures" if @failures.size > show
      @stale.each { |kase, known| io.puts "", "#{kase.id} matches now; drop it from known.yml (#{known[:pattern].source})" }
    end

    def explain(result)
      kase, expected, actual = result.kase, result.expected, result.actual
      out = ["FAIL #{kase.id} (#{[kase.kind, kase.context].compact.join(" in ")})"]
      out << "  #{kase.note}" if kase.note
      out << "  input: #{truncate(kase.input.b.inspect, 200)}"
      if expected[:error] || actual[:error]
        out << "  nokogiri: #{expected[:error] || "no error"}" << "  lobzique: #{actual[:error] || "no error"}"
        return out
      end
      if expected[:tree] != actual[:tree]
        index = expected[:tree].zip(actual[:tree]).index { |a, b| a != b } || [expected[:tree].size, actual[:tree].size].min
        out << "  tree differs at line #{index + 1}:"
        out.concat(expected[:tree][[index - 2, 0].max, 5].map { |l| "    nokogiri #{l}" })
        out.concat(actual[:tree][[index - 2, 0].max, 5].map { |l| "    lobzique #{l}" })
      end
      if expected[:encoding] != actual[:encoding]
        out << "  encoding: nokogiri #{expected[:encoding]}, lobzique #{actual[:encoding]}"
      end
      if expected[:html] != actual[:html]
        a, b = expected[:html], actual[:html]
        index = (0...[a.bytesize, b.bytesize].min).find { |i| a.getbyte(i) != b.getbyte(i) } || [a.bytesize, b.bytesize].min
        from = [index - 40, 0].max
        out << "  to_html differs at byte #{index}:"
        out << "    nokogiri #{a.byteslice(from, 100).inspect}" << "    lobzique #{b.byteslice(from, 100).inspect}"
      end
      out
    end

    def truncate(string, length) = (string.length > length) ? "#{string[0, length]}..." : string
  end
end
