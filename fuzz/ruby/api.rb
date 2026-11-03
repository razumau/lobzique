# frozen_string_literal: true

# Fuzzes lobzique through its Ruby API, so the extension's bindings, the
# object model and the ported Ruby layer run on fuzzed input, under ASan
# with GC and compaction in between (README, "Safety checks"). Run it with
# `rake fuzz:ruby`, which builds the extension for it.
#
# The input is a byte of options, markup, a NUL, then operations of four
# bytes each, as in fuzz/fuzz_targets/dom.rs. The markup is parsed into a
# sanitizer document and into a fragment, and slices of it give the names,
# text, markup and queries the operations use. The operations edit, copy,
# search, scrub and serialize nodes from a pool, drop nodes from it,
# collect garbage and compact, and run XPath functions that edit the tree.
#
# Besides crashes, ASan reports and panics (which Ruby gets as `fatal`), it
# fails on:
# - an exception outside EXPECTED that Nokogiri doesn't raise when it runs
#   the same operations from the same state;
# - sanitized output that changes when sanitized again or parsed again,
#   where Rails::HTML5's doesn't (plan.md, "Sanitizer properties"), and a
#   sanitizer that raises where Rails::HTML5's doesn't.
#
# Nokogiri runs only to judge such an input, in a child process: it can
# loop forever or crash on the trees the operations make.

require "ruzzy"
require "stringio"
require "nokogiri"
require "lobzique"
require "lobzique/sanitizer"
require_relative "../../test/differential/sanitize"

module RubyApiFuzz
  CONFIGS = Differential::SanitizeHarness::CONFIGS.values
  SELECTORS = Differential::Harness::SELECTORS
  XPATHS = Differential::Harness::XPATHS
  SELECTOR_ERRORS = Differential::Harness::SELECTOR_ERRORS
  ROOT_NAME = Differential::Harness::ROOT_NAME
  POOL_LIMIT = 256
  # Deeper than 64 operations can nest
  ANCESTRY_LIMIT = 10_000
  # Below libFuzzer's -timeout (rake fuzz:ruby)
  ORACLE_SECONDS = 3
  ENCODINGS = %w[UTF-8 ISO-8859-1 US-ASCII UTF-16LE Shift_JIS].freeze
  SCRUBBERS = %i[strip prune escape whitewash unprintable targetblank noopener noreferrer].freeze

  # Operations, by Run#operate's numbers
  REPARENTING = [0, 1, 2, 3, 4, 5, 6, 9, 10].freeze
  SELECTING = [17, 19, 20].freeze

  # Raised for bad arguments, as Nokogiri raises them, so they aren't
  # replayed on Nokogiri
  EXPECTED = %w[
    ArgumentError TypeError RangeError EncodingError Encoding::CompatibilityError
    Encoding::UndefinedConversionError Encoding::InvalidByteSequenceError Encoding::ConverterNotFoundError
    XML::SyntaxError XML::XPath::SyntaxError CSS::SyntaxError
  ].freeze

  class Failure < StandardError; end

  # What one implementation needs, by its root module
  Implementation = Data.define(:root, :document, :fragment, :vendor)

  LOBZIQUE = Implementation.new(
    root: Lobzique, document: Lobzique::Sanitizer::Document,
    fragment: Lobzique::Sanitizer::DocumentFragment, vendor: Lobzique::Sanitizer
  )
  NOKOGIRI = Implementation.new(
    root: Nokogiri, document: Loofah::HTML5::Document,
    fragment: Loofah::HTML5::DocumentFragment, vendor: Rails::HTML5::Sanitizer
  )

  # XPath functions the fuzzed queries can call. The first call to edit
  # the tree does, as a handler may (ARCHITECTURE.md, "XPath and CSS
  # handlers"); only one does, since lobzique evaluates again on the edited
  # tree, and an edit that adds a node on every call (reparented text is a
  # copy) never lets it finish.
  class Handler
    def initialize(run)
      @run = run
    end

    def f(nodes, *)
      edit { @run.pool_node(nodes.length)&.unlink }
      nodes
    end

    def g(nodes, *)
      GC.start
      edit { nodes.first&.add_next_sibling("<i>g</i>") }
      nodes.to_a.length.to_s
    end

    def h(nodes, *)
      edit { nodes.each { |node| node.parent = @run.pool_node(node.name.length) if node.respond_to?(:parent=) } }
      true
    rescue ArgumentError, RuntimeError
      false
    end

    private

    def edit
      return if @edited

      @edited = true
      yield
    end
  end

  # The operations on one implementation
  class Run
    attr_reader :log

    def initialize(implementation, options, markup, ops, states: false)
      @impl = implementation
      @options = options
      @markup = markup
      @ops = ops
      @states = states
      @log = []
      @pool = []
      @handler = Handler.new(self)
    end

    # Parses, runs the operations and serializes everything. Returns, for
    # each of these steps, the exception it raised or nil, and with
    # `states:` what the trees and the pool were before it.
    def call
      outcomes = []
      states = []
      steps.each do |step, operation|
        states << state if @states
        @log << operation
        outcomes << attempt { step.call }
        break if outcomes.length == 1 && outcomes[0]
      end
      [outcomes, states]
    end

    def pool_node(byte)
      @pool.empty? ? nil : @pool[byte % @pool.length]
    end

    private

    def steps
      [
        [-> { parse }, [:parse]],
        *@ops.each_slice(4).map { |op| [-> { operate(*op) }, op] },
        [-> { [*@documents, @fragment].each { |top| serialize(top) } }, [:serialize]]
      ]
    end

    def parse
      parse_options = (@options & 0x80 == 0) ? {} : {max_errors: 10}
      @documents = [@impl.document.parse(@markup, **parse_options), @impl.root::HTML5::Document.new]
      @fragment = @impl.fragment.parse(@markup)
      [@documents[0], @fragment, @documents[1]].each { |top| collect(top) }
    end

    # What a step can change that later steps see: every document the pool
    # reaches, and where each node of the pool is. Node types and names
    # stand for nodes, as class names differ between Loofah's documents and
    # lobzique's.
    def state
      return nil unless @documents

      documents = [*@documents, *@pool.map(&:document)].uniq(&:__id__)
      nodes = @pool.map do |node|
        parent = node.parent unless node.document?
        [node.node_type, node.name.b, documents.index { |document| document.equal?(node.document) }, parent&.node_type]
      end
      [html(@fragment), *documents.map { |document| html(document) }, *nodes]
    end

    def html(top)
      top.to_html.b
    rescue => e
      e.class.name.sub(ROOT_NAME, "")
    end

    def attempt
      yield
      nil
    rescue => e
      e
    end

    def operate(kind, x, y, flags)
      a = pool_node(x)
      b = pool_node(y)
      # A node or markup, for the methods that take either
      arg = (flags & 1 == 0) ? b : text(y, flags)
      @log[-1] = [kind % 32, a&.class&.name, b&.class&.name, arg.is_a?(String) ? arg : nil]
      return if [a, b].any? { |node| own_ancestor?(node) }

      case kind % 32
      when 0 then keep(a.add_child(arg))
      when 1 then keep(a.prepend_child(arg))
      when 2 then keep(a.add_previous_sibling(arg))
      when 3 then keep(a.add_next_sibling(arg))
      when 4 then keep(a.replace(arg))
      when 5 then keep(a.swap(arg))
      when 6 then a.parent = b
      when 7 then a.unlink
      when 8 then a.inner_html = text(y, flags)
      when 9 then a.children = arg
      when 10 then keep(a.wrap(arg))
      when 11 then keep(a.dup(flags >> 6))
      when 12 then a[text(x, y)] = text(y, flags)
      when 13 then a.remove_attribute(text(y, flags))
      when 14 then (flags & 2 == 0) ? a.add_class(text(y, flags)) : a.remove_class(text(y, flags))
      when 15 then a.name = text(y, flags)
      when 16 then a.content = text(y, flags)
      when 17 then keep(a.css(selector(y, flags), @handler))
      when 18 then keep(a.xpath(xpath(y, flags), @handler))
      when 19 then a.matches?(selector(y, flags))
      when 20 then keep(a.ancestors(selector(y, flags)))
      when 21 then keep(a.fragment(text(y, flags)))
      when 22 then keep(a.parse(text(y, flags)))
      when 23 then read(a, flags)
      when 24 then a.scrub!(SCRUBBERS[flags % SCRUBBERS.length])
      when 25 then keep(document(flags).create_element(text(x, y), text(y, flags)))
      when 26 then keep(create_character_data(document(flags), flags))
      when 27 then document(flags).root = a
      when 28 then @documents[flags & 1] = keep(document(flags).dup(flags >> 6))
      when 29 then forget(x, y)
      when 30 then GC.start
      else compact(flags)
      end
    end

    # A node moved to another document is copied, and its object points at
    # the copy; the original keeps the object as its node's, so a node left
    # under the original can be its own ancestor (as in Nokogiri), and
    # walking up never ends
    def own_ancestor?(node)
      ancestor = node
      ANCESTRY_LIMIT.times do
        return false unless ancestor.respond_to?(:parent) && !ancestor.document? && (ancestor = ancestor.parent)
      end
      true
    end

    # Up to 63 bytes of the markup, from where `at` points
    def text(at, length)
      start = @markup.bytesize * at / 256
      @markup.byteslice(start, length % 64)
    end

    def selector(at, flags) = (flags & 2 == 0) ? SELECTORS[at % SELECTORS.length] : text(at, flags)

    def xpath(at, flags)
      return text(at, flags) if flags & 2 != 0

      expression = XPATHS[at % XPATHS.length]
      (flags & 4 == 0) ? expression : "#{expression}[#{%w[f g h][at % 3]}(.)]"
    end

    def document(flags) = @documents[flags & 1]

    def create_character_data(document, flags)
      string = text(flags, flags)
      case flags % 3
      when 0 then document.create_text_node(string)
      when 1 then document.create_comment(string)
      else document.create_cdata(string)
      end
    end

    def read(node, flags)
      case flags % 12
      when 0 then node.to_html
      when 1 then node.to_xml
      when 2 then node.to_xhtml
      when 3 then node.inner_html
      when 4 then node.text
      when 5 then node.path
      when 6 then node.css_path
      when 7 then node.inspect
      when 8 then node.write_to(StringIO.new, encoding: ENCODINGS[flags % ENCODINGS.length])
      when 9 then node.attribute_nodes.each { |attribute| keep(attribute) }
      when 10 then node.traverse { |descendant| descendant.name }
      else node.namespaces
      end
    end

    def serialize(node)
      node.to_html
      node.to_xml
    end

    def keep(result)
      case result
      when @impl.root::XML::NodeSet then result.first(8).each { |node| keep(node) }
      when @impl.root::XML::Node then @pool << result if @pool.length < POOL_LIMIT
      end
      result
    end

    def collect(top)
      keep(top)
      top.traverse do |node|
        keep(node)
        node.attribute_nodes.each { |attribute| keep(attribute) } if node.respond_to?(:attribute_nodes)
      end
    end

    # Drops nodes from the pool, so the only references left to them may be
    # from the tree
    def forget(x, y)
      (y % 8).times { @pool.delete_at(x % @pool.length) unless @pool.length <= 1 }
    end

    def compact(flags)
      if flags & 2 == 0
        GC.compact
      else
        GC.verify_compaction_references(toward: :empty)
      end
    rescue NotImplementedError
      GC.start
    end
  end

  class << self
    def test_one_input(data)
      options = data.getbyte(0) or return
      markup, ops = data.byteslice(1..).split("\0", 2)
      markup ||= +""
      markup.force_encoding(Encoding::UTF_8) if options & 0x40 != 0
      ops = (ops || "").bytes.first(256)
      ops = ops.first(ops.length / 4 * 4)

      check_sanitizer(options, markup)
      check_api(options, markup, ops)
      0
    end

    private

    def check_api(options, markup, ops)
      outcomes, = Run.new(LOBZIQUE, options, markup, ops).call
      return if outcomes.all? { |error| error.nil? || EXPECTED.include?(name(error)) }

      # Again, with the states to compare
      run = Run.new(LOBZIQUE, options, markup, ops, states: true)
      outcomes, states = run.call
      expected_outcomes, expected_states = oracle do
        outcomes, states = Run.new(NOKOGIRI, options, markup, ops, states: true).call
        [outcomes.map { |error| error && [name(error), error.message] }, states]
      end
      return unless expected_outcomes

      # Trees that differ (parsed differently, as known.yml lists, or
      # changed differently) can't be compared from there on
      comparable = outcomes.each_index.take_while { |index| states[index] == expected_states[index] }
      index = comparable.find do |step|
        error = outcomes[step]
        expected_name, = expected_outcomes[step]
        error && !EXPECTED.include?(name(error)) && !(expected_name && same_error?(expected_name, error, run.log[step]))
      end
      return if index.nil? || departure?(run.log[index], outcomes[index])

      error = outcomes[index]
      expected_name, expected_message = expected_outcomes[index]
      raise Failure, <<~MESSAGE
        step #{index} #{run.log[index].inspect} raised #{error.class}: #{error.message}
        Nokogiri: #{expected_name ? "#{expected_name}: #{expected_message}" : "no error"}
        steps: #{run.log.inspect}
        #{error.backtrace.first(12).join("\n")}
      MESSAGE
    end

    def same_error?(expected_name, actual, (kind, *))
      names = [expected_name]
      names << SELECTOR_ERRORS[expected_name] if SELECTING.include?(kind)
      names.include?(name(actual))
    end

    # Where lobzique refuses on purpose what Nokogiri does (ARCHITECTURE.md,
    # "Departures from Nokogiri")
    def departure?((kind, a, b), error)
      # "attribute values": an Attr has no children, and nothing is linked
      # next to one or in its place, where its element may be a cycle away
      REPARENTING.include?(kind) && [a, b].any? { |name| name&.end_with?("::Attr") } &&
        error.instance_of?(RuntimeError) &&
        (error.message == "Could not reparent node" || error.message.start_with?("cycle detected"))
    end

    def name(error) = error.class.name.sub(ROOT_NAME, "")

    # What the block returns, run in a child process. Nil if the child
    # fails or takes longer than ORACLE_SECONDS, so the input isn't judged.
    def oracle
      reader, writer = IO.pipe
      pid = fork do
        reader.close
        $stderr.reopen(File::NULL)
        writer.write(Marshal.dump(yield))
        exit!(0)
      rescue Exception # rubocop:disable Lint/RescueException
        exit!(1)
      end
      writer.close
      Marshal.load(reader.read) if reader.wait_readable(ORACLE_SECONDS) # rubocop:disable Security/MarshalLoad
    rescue ArgumentError, TypeError
      nil
    ensure
      reader.close
      Process.kill(:KILL, pid)
      Process.wait(pid)
    end

    # One of the differential's configurations, picked by the options byte
    def check_sanitizer(options, markup)
      config = CONFIGS[options % CONFIGS.length]
      broken = properties(LOBZIQUE, markup, *config)
      return if broken.empty?

      expected_parsed, expected = oracle do
        [parsed(NOKOGIRI, markup), properties(NOKOGIRI, markup, *config).transform_values(&:first)]
      end
      # Input parsed differently (known.yml) gives different output, whose
      # properties don't compare
      return unless expected && expected_parsed == parsed(LOBZIQUE, markup)

      broken.each do |property, (before, after)|
        next if expected.key?(property) && (property != :raises || expected[property] == before)

        message = if property == :raises
          "raises #{before}: #{after}; Rails::HTML5 #{expected[:raises] ? "raises #{expected[:raises]}" : "doesn't"}"
        else
          "#{property} changes #{before.inspect} to #{after.inspect}; Rails::HTML5 doesn't"
        end
        raise Failure, "#{config.inspect}: #{message}"
      end
    end

    # The properties the sanitized output breaks, each with what it was and
    # what it became, or the exception sanitizing raised
    def properties(impl, markup, sanitizer, arguments, options)
      instance = impl.vendor.public_send(sanitizer).new(**arguments)
      output = instance.sanitize(markup, options)
      return {} if output.nil?

      {
        twice: instance.sanitize(output, options),
        reparsed: impl.root::HTML5::DocumentFragment.parse(output).to_html
      }.filter_map { |property, again| [property, [output, again]] if again.b != output.b }.to_h
    rescue => e
      {raises: [name(e), e.message]}
    end

    def parsed(impl, markup)
      impl.root::HTML5::DocumentFragment.parse(markup).to_html.b
    rescue => e
      name(e)
    end
  end
end

# Ruby traps these, and libFuzzer only installs its handlers for signals
# nothing handles: with them, -timeout works inside native code, and a
# SIGTERM ends the run cleanly
%w[ALRM TERM].each { |signal| Signal.trap(signal, "SYSTEM_DEFAULT") }

Ruzzy.fuzz(->(data) { RubyApiFuzz.test_one_input(data) })
