# frozen_string_literal: true

require "stringio"
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

    # Run on every case: the selectors runtime callers in the inventory use
    # (inventory/contract.yml), and the parts of the dialect they rely on
    SELECTORS = [
      "*", "p", "a[href]", "img", "pre", "form", "input", "option", "title", "svg", "math",
      "[data-trix-attachment]", "action-text-attachment[presentation=gallery]",
      "div:has(action-text-attachment[presentation=gallery] + action-text-attachment[presentation=gallery])",
      "h1, h2, h3, h4, h5, h6", "*[@style]", "*[@id]", "a[@href^='#']", "a, area",
      "link[@rel='stylesheet']:not([@data-premailer='ignore']), style:not([@data-premailer='ignore'])",
      "div p", "div > p", "p + p", "p ~ p", "body *", "table tr td", "li:nth-child(2n+1)",
      "p:first", "td:last-child", "a:not([href])", ":root", "p:empty", "[class~=x]", "[lang|=en]",
      "svg|svg", "* > text()", "p:contains('a')"
    ].freeze

    # Run on every case but the selector tests: the XPath runtime callers in
    # the inventory evaluate (inventory/summary.md), and an axis each
    XPATHS = [
      "/html/body", "./body", "./form", "//p[br[following-sibling::br]]", ".//text()", "//comment()", "li",
      "preceding::text()[1]", "html", "html/head", "html/head/meta", '//head/meta[@http-equiv="Set-Cookie"]',
      ".//a", ".//script", "//img/@onerror", "//*:svg", "//svg:svg", "/node()", ".//*[@id][1]",
      "descendant::*[2]/ancestor::*", "descendant::*[3]/preceding::node()[1]", "descendant::*[3]/following::*[1]",
      "descendant::*[1]/following-sibling::node()", "descendant::*[2]/preceding-sibling::*",
      "descendant-or-self::node()[@lang]/@*", "//*[@class = 'x']", "..faulty_xpath"
    ].freeze

    # Errors that only the translators raise differently: Nokogiri's
    # visitor fails with Ruby errors where lobzique raises CSS::SyntaxError
    # (ARCHITECTURE.md, departures). Only the oracle's are renamed, so the
    # same error from lobzique stays a failure.
    SELECTOR_ERRORS = {"Racc::ParseError" => "CSS::SyntaxError", "NoMethodError" => "CSS::SyntaxError",
                       "ZeroDivisionError" => "CSS::SyntaxError", "ArgumentError" => "CSS::SyntaxError"}.freeze

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
    # encoding) and XML, the tree and what read-only methods answer about
    # it, or the error raised.
    def observe(root, kase)
      node = if kase.kind == :document
        root::HTML5.parse(kase.input)
      else
        root::HTML5::DocumentFragment.parse(kase.input, context: kase.context || "body")
      end
      html = node.to_html
      {html: html.b, encoding: html.encoding.name, xml: xml(node), tree: dump(root, node), queries: queries(node),
       selected: select(root, node, kase.selectors || SELECTORS) + evaluate(node, kase.selectors ? [] : XPATHS)}
    rescue => e
      {error: "#{e.class.name.sub(ROOT_NAME, "")}: #{e.message}"}
    end

    XML_OUTPUTS = ["to_xml", "to_xhtml", "to_xhtml in US-ASCII", "first element's to_xhtml",
      "write_xhtml_to with no encoding"].freeze

    # libxml2's output, as XML_OUTPUTS names it. The XHTML of the first
    # element takes the XHTML rules even where a doctype sends the document
    # through libxml2's XML ones.
    def xml(top)
      element = top.children.find(&:element?)
      [top.to_xml, top.to_xhtml, top.to_xhtml(encoding: "US-ASCII"), element&.to_xhtml.to_s,
        StringIO.new.tap { |io| top.write_xhtml_to(io) }.string].map(&:b)
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

    # Each node's path, and the namespaces in scope and language of each
    # element, in document order and without recursion.
    def queries(top)
      lines = []
      stack = top.children.to_a.reverse
      until stack.empty?
        node = stack.pop
        line = node.path
        line += " #{node.namespaces.keys.join(",")} lang=#{node.lang.inspect}" if node.element?
        lines << line
        node.children.to_a.reverse_each { |child| stack << child }
      end
      lines
    end

    # What each selector finds from the top node and from its first element,
    # as node paths, or the class of the error it raises.
    def select(root, top, selectors)
      element = top.children.find(&:element?)
      selectors.flat_map do |selector|
        [top, element].compact.map do |from|
          found = from.css(selector).map(&:path).join(" ")
          "#{selector} from #{from.name}: #{found}"
        rescue => e
          name = e.class.name.sub(ROOT_NAME, "")
          name = SELECTOR_ERRORS.fetch(name, name) if root.equal?(@oracle)
          "#{selector} from #{from.name}: raises #{name}"
        end
      end
    end

    # What each XPath finds from the top node and from its first element, as
    # node paths, or the error it raises.
    def evaluate(top, xpaths)
      element = top.children.find(&:element?)
      xpaths.flat_map do |xpath|
        [top, element].compact.map do |from|
          "#{xpath} from #{from.name}: #{from.xpath(xpath).map(&:path).join(" ")}"
        rescue => e
          code = e.code if e.respond_to?(:code)
          "#{xpath} from #{from.name}: raises #{[e.class.name.sub(ROOT_NAME, ""), code].compact.join(" ")}"
        end
      end
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
      if expected[:tree] == actual[:tree] && expected[:queries] != actual[:queries]
        index = expected[:queries].zip(actual[:queries]).index { |a, b| a != b }
        out << "  node #{index + 1} answers differently:"
        out << "    nokogiri #{expected[:queries][index]}" << "    lobzique #{actual[:queries][index]}"
      end
      if expected[:tree] == actual[:tree] && expected[:selected] != actual[:selected]
        expected[:selected].zip(actual[:selected]).reject { |a, b| a == b }.first(3).each do |a, b|
          out << "  selects differently:" << "    nokogiri #{truncate(a, 300)}" << "    lobzique #{truncate(b, 300)}"
        end
      end
      if expected[:encoding] != actual[:encoding]
        out << "  encoding: nokogiri #{expected[:encoding]}, lobzique #{actual[:encoding]}"
      end
      out.concat(byte_difference("to_html", expected[:html], actual[:html]))
      XML_OUTPUTS.zip(expected[:xml], actual[:xml]).each do |output, a, b|
        out.concat(byte_difference(output, a, b))
      end
      out
    end

    def byte_difference(output, a, b)
      return [] if a == b

      index = (0...[a.bytesize, b.bytesize].min).find { |i| a.getbyte(i) != b.getbyte(i) } || [a.bytesize, b.bytesize].min
      from = [index - 40, 0].max
      ["  #{output} differs at byte #{index}:",
        "    nokogiri #{a.byteslice(from, 100).inspect}", "    lobzique #{b.byteslice(from, 100).inspect}"]
    end

    def truncate(string, length) = (string.length > length) ? "#{string[0, length]}..." : string
  end
end
