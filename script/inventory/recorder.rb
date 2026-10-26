# frozen_string_literal: true

# Records how code outside Nokogiri uses Nokogiri while a test suite runs.
#
#   INVENTORY_OUT=/abs/dir INVENTORY_LABEL=loofah \
#     RUBYOPT="-rbundler/setup -r/abs/script/inventory/recorder.rb" rake test
#
# Nokogiri is loaded right after bundler/setup and every method it defines is
# wrapped by a prepended module. A call is recorded only when its caller lives
# outside Nokogiri, keyed by call site, with argument shapes, block use, return classes
# and raised exceptions. Class checks (is_a?, ===, respond_to?), subclasses,
# mixins and methods other gems add to Nokogiri classes are recorded as well.
# Each process writes one JSON file to INVENTORY_OUT at exit.

module InventoryRecorder
  OUT_DIR = ENV.fetch("INVENTORY_OUT")
  LABEL = ENV.fetch("INVENTORY_LABEL", "unlabeled")
  SELF_PATH = File.expand_path(__FILE__)
  MAX_SAMPLES = 16
  SKIP = %i[method_missing respond_to_missing? inspect pretty_print pretty_print_inspect
    initialize_copy initialize_clone initialize_dup].freeze
  # Selector and XPath strings passed to these are kept verbatim.
  QUERY_METHODS = %i[css at_css xpath at_xpath search at % / > matches? ancestors].freeze
  MAX_QUERIES = 64
  CLASS_CHECKS = %i[is_a? kind_of? instance_of? respond_to?].freeze
  # respond_to? probes made by the VM for implicit conversions are not API use.
  CONVERSIONS = %i[to_ary to_str to_hash to_proc to_io to_a to_s to_int to_path to_sym to_hash].freeze

  @calls = {}
  @subclasses = []
  @lock = Mutex.new
  @installed = false
  @nokogiri_roots = []

  class << self
    def install
      return if @installed || !defined?(::Nokogiri::VERSION)
      @installed = true
      lib = $LOADED_FEATURES.find { |f| f.end_with?("/lib/nokogiri.rb") }
      # Racc drives Nokogiri's CSS parser, so its callbacks count as internal.
      roots = [File.expand_path("..", File.dirname(lib))]
      roots << Gem.loaded_specs["racc"].full_gem_path if Gem.loaded_specs["racc"]
      @nokogiri_roots = roots.flat_map { |r| [r, File.realpath(r)] }.uniq.map { |r| "#{r}/" }
      @modules = nokogiri_modules
      @baseline_ancestors = @modules.to_h { |m| [m, m.ancestors] }
      @modules.each { |m| wrap_module(m) }
    end

    def nokogiri_modules
      seen = {}
      queue = [::Nokogiri]
      until queue.empty?
        m = queue.shift
        next if seen[m]
        seen[m] = true
        m.constants(false).each do |c|
          next if m.autoload?(c)
          v = begin
            m.const_get(c, false)
          rescue Exception # rubocop:disable Lint/RescueException
            next
          end
          queue << v if v.is_a?(Module) && v.name&.start_with?("Nokogiri")
        end
      end
      seen.keys
    end

    def nokogiri_path?(path)
      path && @nokogiri_roots.any? { |r| path.start_with?(r) }
    end

    def nokogiri_defined?(mod, name, singleton)
      um = singleton ? mod.singleton_class.instance_method(name) : mod.instance_method(name)
      loc = um.source_location
      loc.nil? || nokogiri_path?(loc[0])
    end

    def root_class?(mod)
      mod.is_a?(Class) && !mod.superclass&.name&.start_with?("Nokogiri")
    end

    def wrap_module(mod)
      owner = mod.name
      inst = (mod.public_instance_methods(false) + mod.protected_instance_methods(false) +
        mod.private_instance_methods(false)).uniq - SKIP
      inst.select! { |n| nokogiri_defined?(mod, n, false) }
      extra = []
      extra.concat(Enumerable.public_instance_methods - inst) if mod.is_a?(Class) && mod.include?(Enumerable)
      extra.concat(CLASS_CHECKS) if root_class?(mod)
      wrapper = Module.new
      inst.each { |n| define_wrapper(wrapper, mod, n, owner, "instance") }
      extra.each { |n| define_wrapper(wrapper, mod, n, owner, "instance", inherited: true) }
      mod.prepend(wrapper) unless inst.empty? && extra.empty?

      sing = (mod.singleton_class.public_instance_methods(false) - SKIP)
      sing.select! { |n| nokogiri_defined?(mod, n, true) }
      sextra = []
      if root_class?(mod)
        sextra << :new unless sing.include?(:new)
        sextra << :===
      end
      swrapper = Module.new
      sing.each { |n| define_wrapper(swrapper, mod.singleton_class, n, owner, "singleton") }
      sextra.each { |n| define_wrapper(swrapper, mod.singleton_class, n, owner, "singleton", inherited: true) }
      define_inherited_hook(swrapper) if root_class?(mod)
      mod.singleton_class.prepend(swrapper)
    end

    def define_wrapper(wrapper, target, name, owner, kind, inherited: false)
      visibility =
        if target.private_method_defined?(name) then :private
        elsif target.protected_method_defined?(name) then :protected
        else :public
        end
      rec = self
      wrapper.send(:define_method, name) do |*args, &blk|
        loc = rec.external_caller(name)
        return super(*args, &blk) unless loc
        if name == :respond_to? && CONVERSIONS.include?(args.first)
          return super(*args, &blk)
        end
        site = rec.note_call(loc, owner, name, kind, inherited, self, args, blk)
        begin
          result = super(*args, &blk)
        rescue Exception => e # rubocop:disable Lint/RescueException
          rec.note(site, :raises, e.class.name)
          raise
        end
        rec.note(site, :returns, rec.shape(result, value: false))
        result
      end
      wrapper.send(:ruby2_keywords, name)
      wrapper.send(visibility, name)
    end

    def define_inherited_hook(swrapper)
      rec = self
      swrapper.send(:define_method, :inherited) do |sub|
        loc = caller_locations(1, 1).first
        rec.note_subclass(self, sub, loc) unless rec.nokogiri_path?(loc&.absolute_path || loc&.path)
        super(sub)
      end
    end

    # The first caller frame outside Ruby's <internal:*> files, or nil when
    # that frame is Nokogiri itself (or a construction-time initialize call).
    def external_caller(name)
      locs = caller_locations(2, 6) || []
      loc = locs.find { |l| !(l.path || "").start_with?("<internal:") }
      return nil unless loc
      path = loc.absolute_path || loc.path
      return nil if nokogiri_path?(path) || path == SELF_PATH
      return nil if name == :initialize && loc.label.to_s.end_with?("new")
      loc
    end

    def note_call(loc, owner, name, kind, inherited, recv, args, blk)
      key = [loc.absolute_path || loc.path, loc.lineno, owner, name.to_s, kind]
      args_shape = args.map { |a| shape(a) }.join(", ")
      recv_name = (kind == "singleton") ? (recv.name || "(anonymous)") : class_name(recv.class)
      @lock.synchronize do
        site = (@calls[key] ||= {count: 0, inherited: inherited, receivers: [], args: [], blocks: [], returns: [], raises: [], queries: []})
        site[:count] += 1
        add(site[:receivers], recv_name)
        add(site[:args], args_shape)
        add(site[:blocks], !blk.nil?)
        if QUERY_METHODS.include?(name)
          args.grep(String).each { |q| add(site[:queries], q[0, 300], MAX_QUERIES) }
        end
        site
      end
    end

    def note(site, field, value)
      @lock.synchronize { add(site[field], value) }
    end

    def note_subclass(parent, sub, loc)
      @lock.synchronize do
        @subclasses << {parent: parent.name, child: sub.name, path: loc&.absolute_path || loc&.path, line: loc&.lineno}
      end
    end

    def add(list, value, max = MAX_SAMPLES)
      list << value if list.size < max && !list.include?(value)
    end

    def class_name(klass)
      c = klass
      c = c.superclass while c && c.name.nil?
      (c == klass) ? c.name : "#{c&.name}(anonymous subclass)"
    end

    def shape(v, value: true)
      case v
      when nil, true, false then v.inspect
      when Symbol then value ? ":#{v}" : "Symbol"
      when Hash
        keys = v.keys.map { |k| k.is_a?(Symbol) ? "#{k}:" : class_name(k.class) }.join(", ")
        kw = Hash.ruby2_keywords_hash?(v) ? "**" : ""
        "#{kw}{#{keys}}"
      when Module then value ? (v.name || "(anonymous module)") : "Module"
      else class_name(v.class)
      end
    end

    # ------------------------------------------------------------------ dump

    def gem_roots
      @gem_roots ||= Gem.loaded_specs.values.flat_map do |s|
        dirs = [s.full_gem_path]
        dirs << File.realpath(s.full_gem_path) if File.exist?(s.full_gem_path)
        dirs.uniq.map { |d| ["#{d}/", s.name, s.version.to_s] }
      end.sort_by { |d, _, _| -d.size }
    end

    def locate(path)
      return {gem: nil, file: path} unless path
      dir, name, version = gem_roots.find { |d, _, _| path.start_with?(d) }
      return {gem: nil, file: path} unless dir
      {gem: name, version: version, file: path.delete_prefix(dir)}
    end

    # Modules mixed into a Nokogiri class or module (not into Object) after
    # Nokogiri loaded.
    def foreign_ancestors(mod)
      (mod.ancestors - @baseline_ancestors.fetch(mod) - Object.ancestors).reject { |a| a.name.nil? }.map(&:name)
    end

    def own_method_location(target, name)
      um = target.instance_method(name)
      um = um.super_method while um && um.owner != target
      um&.source_location
    end

    def patches(mod)
      [[mod, "instance"], [mod.singleton_class, "singleton"]].flat_map do |target, kind|
        names = target.public_instance_methods(false) + target.private_instance_methods(false)
        names.uniq.filter_map do |n|
          loc = own_method_location(target, n)
          next unless loc && !nokogiri_path?(loc[0]) && loc[0] != SELF_PATH
          {target: mod.name, method: n.to_s, kind: kind, **locate(loc[0]), line: loc[1]}
        end
      end
    end

    # json and fileutils are loaded only here so that the recorder never
    # activates a default gem ahead of Bundler.
    def dump
      return unless @installed
      require "fileutils"
      require "json"
      calls = @calls.map do |(path, line, owner, method, kind), s|
        {**locate(path), line: line, owner: owner, method: method, kind: kind, **s}
      end
      data = {
        label: LABEL, pid: Process.pid, ruby: RUBY_VERSION, nokogiri: ::Nokogiri::VERSION,
        calls: calls,
        subclasses: @subclasses.map { |s| s.merge(locate(s[:path])).except(:path) },
        mixins: @modules.filter_map { |m| (f = foreign_ancestors(m)).empty? ? nil : {target: m.name, mixins: f} },
        patches: @modules.flat_map { |m| patches(m) }
      }
      FileUtils.mkdir_p(OUT_DIR)
      File.write(File.join(OUT_DIR, "#{LABEL}-#{Process.pid}.json"), JSON.pretty_generate(data))
    end
  end
end

# Load Nokogiri up front, but only inside a bundle so the bundled version wins:
# run with RUBYOPT="-rbundler/setup -r/abs/recorder.rb".
if defined?(::Bundler) && ENV["BUNDLE_GEMFILE"]
  begin
    require "nokogiri"
    InventoryRecorder.install
  rescue LoadError
    nil
  end
end
at_exit { InventoryRecorder.dump }
