# frozen_string_literal: true

# Analyzes public Rails apps' Gemfile/Gemfile.lock pairs: which gems depend
# directly on nokogiri or loofah, and whether each is reachable from the
# app's runtime groups or only from development/test groups.
#
#   ruby script/inventory/apps.rb APPS_TSV LOCKFILE_DIR OUT_JSON
#
# APPS_TSV lines are "owner/repo<TAB>sha"; LOCKFILE_DIR holds owner__repo.lock
# and owner__repo.gemfile. Lockfiles are read with Bundler's parser and
# Gemfiles with Prism; neither is evaluated.

require "bundler"
require "json"
require "prism"

module Apps
  TARGETS = %w[nokogiri loofah].freeze
  # Gems declared only in these groups don't ship to production.
  NON_RUNTIME = %w[test development ci lint linters rubocop danger coverage debug doc docs tools
    benchmark benchmarks profile guard metrics test_tools system_tests e2e
    development_test test_development cucumber].freeze

  # Maps each gem declared in a Gemfile to the groups it is declared in.
  class GemfileGroups < Prism::Visitor
    attr_reader :groups

    def initialize
      super
      @stack = []
      @groups = Hash.new { |h, k| h[k] = [] }
    end

    def visit_call_node(node)
      args = node.arguments&.arguments || []
      case node.name
      when :group
        names = args.filter_map { |a| a.unescaped.to_s if a.is_a?(Prism::SymbolNode) || a.is_a?(Prism::StringNode) }
        @stack.push(names)
        super
        @stack.pop
        return
      when :gem
        if (name = args.first).is_a?(Prism::StringNode)
          @groups[name.unescaped].concat(@stack.flatten.empty? ? ["default"] : @stack.flatten)
          @groups[name.unescaped].concat(option_groups(args))
          @groups[name.unescaped].uniq!
        end
      end
      super
    end

    private

    def option_groups(args)
      hash = args.find { |a| a.is_a?(Prism::KeywordHashNode) }
      return [] unless hash
      hash.elements.flat_map do |e|
        next [] unless e.respond_to?(:key) && %w[group groups].include?(e.key.unescaped.to_s)
        v = e.value
        items = v.is_a?(Prism::ArrayNode) ? v.elements : [v]
        items.filter_map { |i| i.unescaped.to_s if i.respond_to?(:unescaped) }
      end
    end
  end

  def self.runtime_groups?(groups)
    groups.empty? || groups.any? { |g| !NON_RUNTIME.include?(g) }
  end

  def self.analyze(repo, sha, lock_text, gemfile_text)
    parser = Bundler::LockfileParser.new(lock_text)
    specs = parser.specs.to_h { |s| [s.name, s] }
    deps = specs.transform_values { |s| s.dependencies.map(&:name).select { |d| specs.key?(d) } }

    visitor = GemfileGroups.new
    Prism.parse(gemfile_text).value.accept(visitor)
    declared = visitor.groups
    top = parser.dependencies.keys
    # Top-level gems the Gemfile scan could not place (eval_gemfile, loops) count as runtime.
    top_groups = top.to_h { |n| [n, declared.fetch(n, ["unknown"])] }

    reach = lambda do |roots|
      seen = {}
      queue = roots.dup
      until queue.empty?
        n = queue.shift
        next if seen[n] || !specs.key?(n)
        seen[n] = true
        queue.concat(deps.fetch(n, []))
      end
      seen
    end
    runtime_roots = top.select { |n| runtime_groups?(top_groups[n] - ["unknown"]) }
    runtime = reach.call(runtime_roots)

    dependents = TARGETS.to_h do |t|
      [t, specs.keys.select { |n| deps[n].include?(t) }.sort.map do |n|
        {gem: n, version: specs[n].version.to_s, runtime: runtime.key?(n),
         declared_in: top_groups[n]}
      end]
    end

    {
      repo: repo, sha: sha,
      rails: specs["rails"]&.version&.to_s || specs["railties"]&.version&.to_s,
      nokogiri: specs["nokogiri"]&.version&.to_s,
      loofah: specs["loofah"]&.version&.to_s,
      rails_html_sanitizer: specs["rails-html-sanitizer"]&.version&.to_s,
      rails_dom_testing: specs["rails-dom-testing"]&.version&.to_s,
      gem_count: specs.size,
      gems: specs.transform_values { |s| {version: s.version.to_s, runtime: runtime.key?(s.name)} },
      dependents: dependents
    }
  end

  def self.run(tsv, dir, out)
    apps = File.readlines(tsv, chomp: true).reject(&:empty?).map do |line|
      repo, sha = line.split("\t")
      base = File.join(dir, repo.sub("/", "__"))
      analyze(repo, sha, File.read("#{base}.lock"), File.read("#{base}.gemfile"))
    end

    summary = Hash.new { |h, k| h[k] = {apps: [], runtime_apps: [], via: []} }
    apps.each do |app|
      app[:dependents].each do |target, list|
        list.each do |d|
          s = summary[d[:gem]]
          s[:apps] |= [app[:repo]]
          s[:runtime_apps] |= [app[:repo]] if d[:runtime]
          s[:via] |= [target]
        end
      end
    end
    gem_frequency = Hash.new(0)
    apps.each { |a| a[:gems].each_key { |g| gem_frequency[g] += 1 } }

    File.write(out, JSON.pretty_generate(
      apps: apps,
      dependents: summary.sort_by { |g, s| [-s[:apps].size, g] }.to_h,
      gem_frequency: gem_frequency.sort_by { |g, c| [-c, g] }.to_h
    ))
  end
end

Apps.run(*ARGV) if $PROGRAM_NAME == __FILE__
