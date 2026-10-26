# frozen_string_literal: true

# Statically scans a gem's Ruby sources for Nokogiri use.
#
#   ruby script/inventory/scan.rb API_JSON GEM VERSION ROOT OUT_JSON
#
# API_JSON comes from api_surface.rb. Per file and line it records:
#
#   constants    Nokogiri::* references and the role each plays (superclass,
#                include, rescue, when, is_a?, receiver, argument, ...)
#   const_calls  calls whose receiver is a Nokogiri constant, e.g.
#                Nokogiri::HTML5.fragment(...)
#   reopens      class/module bodies inside the Nokogiri namespace, with the
#                methods they define
#   subclasses   classes whose superclass is a Nokogiri class
#   calls        calls of method names Nokogiri defines and core Ruby does not;
#                the receiver type is unknown, so these are candidates only.
#                Names in STRONG are kept everywhere; the rest (children,
#                content, parse, ...) only in files that reference Nokogiri.
#   queries      string literals passed to css/xpath/search-like methods
#   requires     require "nokogiri..." lines
#
# Files are only parsed, never loaded.

require "json"
require "prism"

module Scan
  # Unlike the recorder, the scanner skips the operator aliases (% / >), which
  # mostly match numeric and version comparisons here.
  QUERY_METHODS = %i[css at_css xpath at_xpath search at matches? ancestors].freeze
  CLASS_CHECKS = %i[is_a? kind_of? instance_of?].freeze
  MIXIN_CALLS = %i[include extend prepend].freeze
  STRONG = %w[add_child add_class add_namespace add_namespace_definition add_next_sibling
    add_previous_sibling append_class at_css at_xpath attribute_nodes attribute_with_ns cdata?
    children= collect_namespaces comment? create_cdata create_comment create_element
    create_text_node css css_path decorate! element_children elem? encode_special_chars
    first_element_child get_attribute has_attribute? inner_html inner_html= inner_text inner_xml
    kwattr_add kwattr_append kwattr_remove kwattr_values last_element_child meta_encoding
    namespace_definitions namespaced_key? next_element next_sibling node_name node_name= node_type
    outer_xml prepend_child previous_element previous_sibling processing_instruction? remove_attr
    remove_attribute remove_class remove_namespaces! set_attribute slop! swap text? to_xhtml
    traverse unlink write_html_to write_xhtml_to write_xml_to xpath].to_h { |n| [n, true] }.freeze

  def self.origin(rel)
    case rel
    when %r{\A(test|spec|features)/}, /_(test|spec)\.rb\z/ then "test"
    when %r{\A(lib|app)/} then "lib"
    else "other"
    end
  end

  class Visitor < Prism::Visitor
    attr_reader :out

    def initialize(api, file, origin)
      super()
      @top = top_level_names(api)
      @distinctive = distinctive_names(api)
      @file = file
      @origin = origin
      @nesting = []
      @roles = {}
      @out = Hash.new { |h, k| h[k] = [] }
      @weak_calls = []
    end

    # Weak candidates count only when the file references Nokogiri.
    def finish
      @out[:calls].concat(@weak_calls) if @out.key?(:constants) || @out.key?(:requires)
      self
    end

    def visit_class_node(node)
      name = qualified(node.constant_path)
      if (parent = node.superclass) && (pname = nokogiri_const(parent))
        @roles[parent.object_id] = "superclass"
        emit(:subclasses, node, child: name, parent: pname)
      end
      enter(node, name, "class") { super }
    end

    def visit_module_node(node)
      enter(node, qualified(node.constant_path), "module") { super }
    end

    def visit_call_node(node)
      recv_const = node.receiver && nokogiri_const(node.receiver)
      args = node.arguments&.arguments || []
      if MIXIN_CALLS.include?(node.name) && node.receiver.nil?
        args.each { |a| @roles[a.object_id] = node.name.to_s }
      elsif CLASS_CHECKS.include?(node.name)
        args.each { |a| @roles[a.object_id] = node.name.to_s }
      else
        args.each { |a| @roles[a.object_id] ||= "argument" }
      end

      if recv_const
        @roles[node.receiver.object_id] = (node.name == :===) ? "===" : "receiver"
        emit(:const_calls, node, receiver: recv_const, method: node.name.to_s,
          args: args.map { |a| arg_shape(a) }, block: !node.block.nil?)
      elsif node.name == :Nokogiri && node.receiver.nil?
        emit(:const_calls, node, receiver: "Kernel", method: "Nokogiri",
          args: args.map { |a| arg_shape(a) }, block: !node.block.nil?)
      elsif @distinctive.include?(node.name.to_s)
        call = {file: @file, line: node.location.start_line, origin: @origin,
                method: node.name.to_s, receiver: receiver_kind(node), strong: STRONG.key?(node.name.to_s)}
        (call[:strong] ? @out[:calls] : @weak_calls) << call
      end

      q = args.first
      if QUERY_METHODS.include?(node.name) && (q.is_a?(Prism::StringNode) || q.is_a?(Prism::InterpolatedStringNode))
        emit(:queries, node, method: node.name.to_s, query: q.is_a?(Prism::StringNode) ? q.unescaped : q.slice)
      end

      if %i[require require_relative].include?(node.name) && node.receiver.nil? &&
          (s = args.first).is_a?(Prism::StringNode) && s.unescaped.match?(/nokogiri/i)
        emit(:requires, node, path: s.unescaped)
      end
      super
    end

    def visit_rescue_node(node)
      node.exceptions.each { |e| @roles[e.object_id] = "rescue" }
      super
    end

    def visit_when_node(node)
      node.conditions.each { |c| @roles[c.object_id] = "when" }
      super
    end

    def visit_in_node(node)
      @roles[node.pattern.object_id] = "pattern"
      super
    end

    def visit_defined_node(node)
      @roles[node.value.object_id] = "defined?"
      super
    end

    def visit_constant_write_node(node)
      @roles[node.value.object_id] = "assigned"
      super
    end

    def visit_constant_path_node(node)
      # The outermost path is the whole reference; don't descend into its parts.
      record_const(node)
    end

    def visit_constant_read_node(node)
      record_const(node)
    end

    private

    def top_level_names(api)
      api.fetch("modules").keys.filter_map { |n| n.split("::")[1] }.uniq +
        api.fetch("aliases").keys.filter_map { |n| n.split("::")[1] }
    end

    def distinctive_names(api)
      generic = api.fetch("generic_method_names").to_h { |n| [n, true] }
      api.fetch("modules").values.flat_map { |m| m["instance"] }.uniq.reject { |n| generic[n] }.to_h { |n| [n, true] }
    end

    def enter(node, name, kind)
      if name.start_with?("Nokogiri") && name != "Nokogiri"
        defs = (node.body&.child_nodes || []).flat_map { |c| c.is_a?(Prism::StatementsNode) ? c.body : [c] }
          .grep(Prism::DefNode).map { |d| (d.receiver ? "self." : "") + d.name.to_s }
        emit(:reopens, node, name: name, kind: kind, defs: defs)
      end
      @nesting.push(name)
      yield
    ensure
      @nesting.pop
    end

    def qualified(path)
      raw = path.slice.delete_prefix("::")
      outer = @nesting.last
      outer ? "#{outer}::#{raw}" : raw
    end

    # The Nokogiri constant a node names, resolving references made from
    # inside `module Nokogiri`; nil for anything else.
    def nokogiri_const(node)
      return nil unless node.is_a?(Prism::ConstantPathNode) || node.is_a?(Prism::ConstantReadNode)
      raw = node.slice.delete_prefix("::")
      return raw if raw == "Nokogiri" || raw.start_with?("Nokogiri::")
      if @nesting.any? { |n| n.start_with?("Nokogiri") } && @top.include?(raw.split("::").first)
        "Nokogiri::#{raw}"
      end
    end

    def record_const(node)
      name = nokogiri_const(node)
      return unless name
      emit(:constants, node, name: name, role: @roles.fetch(node.object_id, "reference"))
    end

    def receiver_kind(node)
      case node.receiver
      when nil then "implicit"
      when Prism::SelfNode then "self"
      else "explicit"
      end
    end

    def arg_shape(node)
      case node
      when Prism::StringNode, Prism::InterpolatedStringNode then "String"
      when Prism::SymbolNode then ":#{node.unescaped}"
      when Prism::IntegerNode then "Integer"
      when Prism::NilNode, Prism::TrueNode, Prism::FalseNode then node.slice
      when Prism::KeywordHashNode, Prism::HashNode
        "{" + node.elements.map { |e| e.respond_to?(:key) ? e.key.slice : "**" }.join(", ") + "}"
      when Prism::ConstantPathNode, Prism::ConstantReadNode then node.slice
      when Prism::BlockArgumentNode then "&block"
      when Prism::SplatNode then "*splat"
      else node.type.to_s.delete_suffix("_node")
      end
    end

    def emit(kind, node, **data)
      @out[kind] << {file: @file, line: node.location.start_line, origin: @origin, **data}
    end
  end

  def self.run(api_path, gem, version, root, out_path)
    api = JSON.parse(File.read(api_path))
    result = Hash.new { |h, k| h[k] = [] }
    errors = []
    files = Dir.glob("**/*.{rb,rake}", base: root).reject { |f| f.start_with?("vendor/", "node_modules/", "tmp/") }
    files.sort.each do |rel|
      parsed = Prism.parse_file(File.join(root, rel))
      errors << rel unless parsed.errors.empty?
      visitor = Visitor.new(api, rel, origin(rel))
      parsed.value.accept(visitor)
      visitor.finish.out.each { |k, v| result[k].concat(v) }
    end
    File.write(out_path, JSON.pretty_generate(
      gem: gem, version: version, files_scanned: files.size, parse_errors: errors, **result
    ))
  end
end

Scan.run(*ARGV) if $PROGRAM_NAME == __FILE__
