# frozen_string_literal: true

# Reports how much of the API contract lobzique implements: every entry that
# inventory/contract.yml scopes `in` (see inventory/scope.yml), looked up under
# `Lobzique::` instead of `Nokogiri::`, since the core mirrors Nokogiri's tree.
#
# An entry counts once it is defined, not once it behaves like Nokogiri; the
# test suites track behavior. Methods that only Object/Kernel/BasicObject
# provide (`#==`, `#dup`, `#initialize`, `#respond_to?`, ...) don't count.
# `.new` counts when the class defines its own `new` or `#initialize`.
#
#   ruby -Ilib script/progress.rb [--missing]

require "yaml"
require "lobzique"

module Progress
  CONTRACT = File.expand_path("../inventory/contract.yml", __dir__)
  SCOPE = "in"

  module_function

  def run(show_missing: false)
    entries = YAML.safe_load_file(CONTRACT).fetch("entries").select { |e| e["scope"] == SCOPE }
    results = entries.map { |e| [e["api"], implemented?(e)] }
    groups = entries.zip(results).group_by { |e, _| group(e) }.transform_values { |rows| rows.map(&:last) }

    width = groups.keys.map(&:length).max
    groups.sort.each do |name, rows|
      done = rows.count(&:last)
      puts format("%-#{width}s  %3d/%-3d %s", name, done, rows.size, percent(done, rows.size))
    end
    done = results.count(&:last)
    puts format("%-#{width}s  %3d/%-3d %s", "total", done, results.size, percent(done, results.size))

    return unless show_missing
    puts "\nmissing:"
    results.reject(&:last).each { |api, _| puts "  #{api}" }
  end

  def percent(done, total) = format("%5.1f%%", 100.0 * done / total)

  # The class or module an entry belongs to.
  def group(entry)
    api = entry.fetch("api")
    case entry.fetch("kind")
    when "instance", "singleton" then api[/\A[^#.]+/]
    when "constant" then api.sub(/::\w+\z/, "")
    else api
    end
  end

  def implemented?(entry)
    api = entry.fetch("api").sub(/\ANokogiri\b/, "Lobzique")
    case entry.fetch("kind")
    when "module" then resolve(api).instance_of?(Module)
    when "class" then resolve(api).is_a?(Class)
    when "constant" then !resolve(api).nil?
    when "instance"
      owner, name = api.split("#", 2)
      own_instance_method?(resolve(owner), name)
    when "singleton"
      owner, name = api.split(".", 2)
      mod = resolve(owner)
      return false unless mod
      own_method?(mod.singleton_class, name, Class) ||
        (name == "new" && own_instance_method?(mod, "initialize"))
    end
  end

  def own_instance_method?(mod, name) = mod.is_a?(Module) && own_method?(mod, name, Object)

  # True when `mod` defines `name` (public or private) itself or through a
  # module other than the ones every `base` already has.
  def own_method?(mod, name, base)
    return false unless mod.method_defined?(name) || mod.private_method_defined?(name)
    !base.ancestors.include?(mod.instance_method(name).owner)
  end

  def resolve(path)
    Object.const_get(path)
  rescue NameError
    nil
  end
end

Progress.run(show_missing: ARGV.include?("--missing")) if $PROGRAM_NAME == __FILE__
