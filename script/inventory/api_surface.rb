# frozen_string_literal: true

# Dumps Nokogiri's public surface (modules, superclasses, methods, constants)
# as JSON. Run inside a bundle that has nokogiri:
#
#   BUNDLE_GEMFILE=... ruby -rbundler/setup script/inventory/api_surface.rb OUT.json

require "nokogiri"
require "json"

out = ARGV.fetch(0)
seen = {}
queue = [Nokogiri]
until queue.empty?
  mod = queue.shift
  next if seen.key?(mod)
  seen[mod] = true
  mod.constants(false).each do |c|
    next if mod.autoload?(c)
    value = mod.const_get(c, false)
    queue << value if value.is_a?(Module) && value.name&.start_with?("Nokogiri")
  end
end

own = ->(m) { m.public_instance_methods(false).map(&:to_s).sort }
modules = seen.keys.sort_by(&:name).to_h do |mod|
  [mod.name, {
    kind: mod.is_a?(Class) ? "class" : "module",
    superclass: mod.is_a?(Class) ? mod.superclass&.name : nil,
    includes: mod.included_modules.map(&:name).compact.select { |n| n.start_with?("Nokogiri") } -
      ((mod.is_a?(Class) && mod.superclass) ? mod.superclass.included_modules.map(&:name) : []),
    instance: own.call(mod),
    singleton: own.call(mod.singleton_class),
    constants: mod.constants(false).map(&:to_s).reject { |c| mod.const_get(c, false).is_a?(Module) }.sort
  }]
end
aliases = {}
seen.keys.each do |mod|
  mod.constants(false).each do |c|
    value = mod.const_get(c, false)
    full = "#{mod.name}::#{c}"
    aliases[full] = value.name if value.is_a?(Module) && value.name != full && value.name&.start_with?("Nokogiri")
  end
end

generic = [Object, Kernel, BasicObject, Module, Class, Array, Hash, String, Symbol, Integer,
  Enumerable, Comparable, Struct, Exception, StandardError].flat_map do |m|
  m.public_instance_methods.map(&:to_s)
end.uniq.sort

File.write(out, JSON.pretty_generate(
  nokogiri: Nokogiri::VERSION, modules: modules, aliases: aliases, generic_method_names: generic
))
warn "wrote #{modules.size} modules to #{out}"
