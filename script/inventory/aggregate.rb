# frozen_string_literal: true

# Builds the API contract from the raw inventory data:
#
#   inventory/raw/nokogiri-api.json   Nokogiri's surface (api_surface.rb)
#   inventory/raw/traces/*.json       runtime calls (recorder.rb)
#   inventory/raw/scans/*.json        static findings (scan.rb)
#   inventory/raw/rank/apps.json      public app corpus (apps.rb)
#   script/inventory/targets.yml      which gems count, and how they're used
#   inventory/scope.yml               hand-made scope decisions
#
# and writes inventory/contract.yml (one entry per API element) and
# inventory/summary.md (tables for review).
#
#   ruby script/inventory/aggregate.rb

require "json"
require "yaml"

module Aggregate
  ROOT = File.expand_path("../..", __dir__)
  RAW = File.join(ROOT, "inventory/raw")
  MAX_SITES = 6
  CLASS_CHECKS = %w[is_a? kind_of? instance_of?].freeze
  # Owners preferred when a name-only match fits several Nokogiri modules.
  PREFERRED_OWNERS = %w[Nokogiri::XML::Searchable Nokogiri::XML::Node Nokogiri::XML::NodeSet
    Nokogiri::XML::Document Nokogiri::XML::DocumentFragment Nokogiri::XML::Attr
    Nokogiri::XML::Element Nokogiri::XML::Text].freeze
  # Name-only matches are attributed to these classes alone; names that only
  # other Nokogiri classes define (CSS::Tokenizer#action, ...) are dropped.
  DOM_OWNERS = /\ANokogiri::(XML::(Node|NodeSet|Document|DocumentFragment|Attr|Element|Text|CharacterData|Comment|
    CDATA|Searchable|Namespace)|HTML4::Document|HTML5::(Document|DocumentFragment|Node))\z/x
  XML_ONLY = /\ANokogiri::(XSLT|XML::(SAX|Schema|RelaxNG|Reader|DTD|ElementDecl|EntityDecl|AttributeDecl|
    ElementContent|XPathContext|XPath\b|EntityReference|Notation|Builder)|HTML4::SAX|HTML4::EncodingReader)/x
  STUBS = /\A(Nokogiri::VersionInfo|Nokogiri::VERSION|Nokogiri::VERSION_INFO|Nokogiri::LIBXML|Nokogiri::LIBXSLT|
    Nokogiri\.(jruby\?|uses_libxml\?|uses_gumbo\?|libxml2_patches|install_default_aliases))/x

  class Builder
    def initialize
      @api = JSON.parse(File.read(File.join(RAW, "nokogiri-api.json")))
      @modules = @api.fetch("modules")
      @aliases = @api.fetch("aliases")
      @targets = YAML.safe_load_file(File.join(ROOT, "script/inventory/targets.yml"))
      @tiers = %w[tier1 tier2].to_h { |t| [t, @targets.fetch(t, nil) || {}] }
      @gems = @tiers.values.reduce({}, :merge)
      @scope = load_scope
      @entries = {}
      @queries = []
      @extensions = []
      @ignored = Hash.new(0)
    end

    def run
      Dir[File.join(RAW, "traces/*.json")].sort.each { |f| add_trace(JSON.parse(File.read(f))) }
      Dir[File.join(RAW, "scans/*.json")].sort.each { |f| add_scan(JSON.parse(File.read(f))) }
      @entries.each_value { |e| finalize(e) }
      write_contract
      write_summary
    end

    # ------------------------------------------------------------ helpers

    def load_scope
      path = File.join(ROOT, "inventory/scope.yml")
      return [] unless File.exist?(path)
      (YAML.safe_load_file(path) || {}).fetch("rules", []).map do |r|
        r.merge("re" => Regexp.new(r.fetch("match")))
      end
    end

    def target?(gem) = @gems.key?(gem)

    def code?(file) = file.start_with?("lib/", "app/")

    def usage(gem, file)
      t = @gems.fetch(gem)
      return "test" if t["usage"] == "test"
      hit = (t["test_paths"] || []).any? do |g|
        g.end_with?("/**") ? file.start_with?(g.delete_suffix("**")) : File.fnmatch?(g, file, File::FNM_PATHNAME)
      end
      hit ? "test" : "runtime"
    end

    def tier(gem) = @tiers.find { |_, gems| gems.key?(gem) }&.first

    def domain(gem) = @gems.fetch(gem)["domain"]

    def canonical(const)
      @aliases.each do |from, to|
        return const.sub(from, to) if const == from || const.start_with?("#{from}::")
      end
      const
    end

    def entry(key, kind, owner, name)
      @entries[key] ||= {"api" => key, "kind" => kind, "owner" => owner, "name" => name,
                         "used_by" => {}, "receivers" => [], "args" => [], "blocks" => [],
                         "returns" => [], "raises" => [], "roles" => [], "spelled_as" => []}
    end

    def use(entry, gem, version, file, line, evidence)
      u = (entry["used_by"][gem] ||= {"version" => version, "usage" => [], "evidence" => [], "sites" => []})
      u["usage"] |= [usage(gem, file)]
      u["evidence"] |= [evidence]
      site = "#{file}:#{line}"
      u["sites"] << site if u["sites"].size < MAX_SITES && !u["sites"].include?(site)
    end

    def merge(entry, field, values)
      entry[field] |= Array(values)
    end

    def resolve_singleton(const, meth)
      name = canonical(const)
      while name && (m = @modules[name])
        return name if m["singleton"].include?(meth)
        name = m["superclass"]
      end
      nil
    end

    def instance_owners(meth)
      owners = @modules.select { |name, m| name.match?(DOM_OWNERS) && m["instance"].include?(meth) }.keys
      owners.sort_by { |o| [PREFERRED_OWNERS.index(o) || PREFERRED_OWNERS.size, o] }
    end

    def constant_entry(name)
      canon = canonical(name)
      kind = @modules.key?(canon) ? @modules[canon]["kind"] : "constant"
      e = entry(canon, kind, canon.rpartition("::").first, canon.rpartition("::").last)
      e["spelled_as"] |= [name] if name != canon
      e
    end

    # ------------------------------------------------------------- traces

    def add_trace(trace)
      trace["calls"].each { |c| add_call(c) }
      trace["subclasses"].each do |s|
        next unless target?(s["gem"]) && code?(s["file"])
        add_extension("subclass", s["gem"], s["version"], s["file"], s["line"],
          "#{s["child"]} < #{s["parent"]}", s["parent"], "traced")
      end
      trace["patches"].each do |p|
        next unless target?(p["gem"])
        sep = (p["kind"] == "singleton") ? "." : "#"
        add_extension("patch", p["gem"], p["version"], p["file"], p["line"],
          "#{p["target"]}#{sep}#{p["method"]}", p["target"], "traced")
      end
      trace["mixins"].each do |m|
        m["mixins"].each do |mod|
          @extensions << {"type" => "mixin", "api" => "#{m["target"]} includes #{mod}", "target" => m["target"],
                          "gem" => target?(trace["label"]) ? trace["label"] : nil, "evidence" => ["traced"],
                          "label" => trace["label"]}
        end
      end
    end

    def add_call(c)
      gem = c["gem"]
      unless gem && target?(gem) && code?(c["file"])
        @ignored[(gem && target?(gem)) ? "own tests and scripts" : "outside target gems (#{gem || "app"})"] += 1
        return
      end
      if CLASS_CHECKS.include?(c["method"])
        c["args"].each do |a|
          next unless a.start_with?("Nokogiri")
          e = constant_entry(a)
          merge(e, "roles", c["method"])
          use(e, gem, c["version"], c["file"], c["line"], "traced")
        end
        return
      end
      if c["method"] == "===" && c["kind"] == "singleton"
        c["receivers"].each do |r|
          e = constant_entry(r)
          merge(e, "roles", "=== (case/rescue)")
          use(e, gem, c["version"], c["file"], c["line"], "traced")
        end
        return
      end
      owners =
        if c["method"] == "new" && c["kind"] == "singleton"
          c["receivers"].map do |r|
            next r if r.start_with?("Nokogiri")
            r.include?("HTML5") ? c["owner"].sub("HTML4", "HTML5") : c["owner"]
          end.uniq
        elsif c["owner"].start_with?("Nokogiri::HTML4::Document")
          # HTML5::Document(Fragment) inherits these; file HTML5 receivers under HTML5.
          c["receivers"].map { |r| r.include?("HTML5") ? c["owner"].sub("HTML4", "HTML5") : c["owner"] }.uniq
        else
          [c["owner"]]
        end
      owners.each do |owner|
        sep = (c["kind"] == "singleton") ? "." : "#"
        e = entry("#{owner}#{sep}#{c["method"]}", c["kind"], owner, c["method"])
        e["inherited_from"] = c["owner"] if owner != c["owner"] && !(c["method"] == "new" && c["kind"] == "singleton")
        use(e, gem, c["version"], c["file"], c["line"], "traced")
        %w[receivers args blocks returns raises].each { |f| merge(e, f, c[f]) }
        c["queries"].each { |q| add_query(gem, c["version"], c["file"], c["line"], c["method"], q, "traced") }
      end
    end

    # -------------------------------------------------------------- scans

    def add_scan(scan)
      gem = scan["gem"]
      return unless target?(gem)
      version = scan["version"]
      lib = ->(r) { r["origin"] == "lib" }
      %w[constants const_calls calls queries subclasses reopens requires].each { |k| scan[k] ||= [] }

      scan["constants"].select(&lib).each do |c|
        e = constant_entry(c["name"])
        merge(e, "roles", c["role"])
        use(e, gem, version, c["file"], c["line"], "static")
      end
      scan["const_calls"].select(&lib).each do |c|
        if c["receiver"] == "Kernel"
          e = entry("Kernel#Nokogiri", "instance", "Kernel", "Nokogiri")
        elsif (owner = resolve_singleton(c["receiver"], c["method"]))
          e = entry("#{owner}.#{c["method"]}", "singleton", owner, c["method"])
          e["spelled_as"] |= ["#{c["receiver"]}.#{c["method"]}"] if canonical(c["receiver"]) != owner
        else
          # A method on a value constant (SaveOptions flags) or an unknown name.
          e = constant_entry(c["receiver"])
          merge(e, "roles", "receiver of #{c["method"]}")
        end
        merge(e, "args", [c["args"].join(", ")])
        merge(e, "blocks", [c["block"]])
        use(e, gem, version, c["file"], c["line"], "static")
      end
      scan["calls"].select(&lib).each { |c| add_candidate(gem, version, c) }
      scan["queries"].select(&lib).each do |q|
        add_query(gem, version, q["file"], q["line"], q["method"], q["query"], "static")
      end
      scan["subclasses"].select(&lib).each do |s|
        add_extension("subclass", gem, version, s["file"], s["line"], "#{s["child"]} < #{canonical(s["parent"])}",
          canonical(s["parent"]), "static")
      end
      scan["reopens"].select(&lib).each do |r|
        add_extension("reopen", gem, version, r["file"], r["line"],
          "#{r["kind"]} #{r["name"]} (#{r["defs"].empty? ? "no defs" : r["defs"].join(", ")})", r["name"], "static")
      end
      scan["requires"].each { |r| (@requires ||= Hash.new { |h, k| h[k] = [] })[gem] |= [r["path"]] }
    end

    # A call matched by method name only; the receiver's type is unknown.
    def add_candidate(gem, version, c)
      name = c["method"]
      traced = @entries.values.select do |e|
        e["kind"] == "instance" && e["name"] == name && e["used_by"].dig(gem, "evidence")&.include?("traced")
      end
      if traced.any?
        traced.each { |e| use(e, gem, version, c["file"], c["line"], "static") }
        return
      end
      owners = instance_owners(name)
      return if owners.empty?
      e = entry("#{owners.first}##{name}", "instance", owners.first, name)
      e["also_defined_on"] = (e["also_defined_on"] || []) | owners.drop(1) if owners.size > 1
      use(e, gem, version, c["file"], c["line"], "name match")
    end

    def add_query(gem, version, file, line, meth, query, evidence)
      lang =
        if %w[xpath at_xpath].include?(meth) then "xpath"
        elsif %w[css at_css matches?].include?(meth) then "css"
        elsif query.match?(%r{\A\s*(\.?/|\(|[a-z-]+::|@)}) || query.include?("//") then "xpath"
        else "css"
        end
      existing = @queries.find { |q| q["gem"] == gem && q["query"] == query && q["method"] == meth }
      site = "#{file}:#{line}"
      if existing
        existing["sites"] |= [site]
        existing["evidence"] |= [evidence]
      else
        @queries << {"gem" => gem, "usage" => usage(gem, file), "language" => lang, "method" => meth,
                     "query" => query, "evidence" => [evidence], "sites" => [site]}
      end
    end

    def add_extension(type, gem, version, file, line, api, target, evidence)
      existing = @extensions.find { |x| x["type"] == type && x["api"] == api && x["gem"] == gem }
      if existing
        existing["evidence"] |= [evidence]
        return
      end
      @extensions << {"type" => type, "api" => api, "target" => target, "gem" => gem, "version" => version,
                      "usage" => usage(gem, file), "site" => "#{file}:#{line}", "evidence" => [evidence]}
      e = constant_entry(target) if target.start_with?("Nokogiri")
      if e
        merge(e, "roles", type)
        use(e, gem, version, file, line, evidence)
      end
    end

    # ------------------------------------------------------------- scope

    def auto_scope(e)
      key = e["api"]
      return ["stub", "libxml2/platform introspection; answer statically"] if key.match?(STUBS)
      return ["out:xml", "XML-only feature"] if key.match?(XML_ONLY)
      if %w[xpath at_xpath].include?(e["name"])
        return ["review:xpath", "XPath; see the queries list for the expressions actually used"]
      end
      if key.start_with?("Nokogiri::HTML4") || key.match?(/\ANokogiri\.(HTML|HTML4)\z/)
        return ["review:html4", "HTML4 entry point; candidate for routing to the HTML5 parser"]
      end
      if key.match?(/\ANokogiri(\.XML|::XML::Document\.(parse|read_memory|read_io)|::XML::DocumentFragment\.parse)\z/)
        return ["review:xml", "XML parsing entry point"]
      end
      return ["review", "CSS-to-XPath internals"] if key.start_with?("Nokogiri::CSS")
      return ["review", "receiver type unknown"] if e["used_by"].values.all? { |u| u["evidence"] == ["name match"] }
      ["in", nil]
    end

    def finalize(e)
      scope, note = auto_scope(e)
      e["domains"] = e["used_by"].keys.map { |g| @gems.fetch(g)["domain"] }.uniq.sort
      if e["domains"] == ["xml"] && scope != "stub"
        scope = "out:xml"
        note = "only XML-domain gems use it"
      end
      if (rule = @scope.find { |r| r["re"].match?(e["api"]) })
        scope = rule.fetch("scope")
        note = rule["note"] || note
      end
      e["scope"] = scope
      e["note"] = note if note
      # Usage counts non-XML users when there are any, since XML users don't decide scope.
      users = e["used_by"].reject { |g, _| domain(g) == "xml" }
      users = e["used_by"] if users.empty?
      e["usage"] = users.values.flat_map { |u| u["usage"] }.include?("runtime") ? "runtime" : "test"
      e["evidence"] = e["used_by"].values.flat_map { |u| u["evidence"] }.uniq.sort
      e["tiers"] = e["used_by"].keys.map { |g| tier(g) }.uniq.sort
    end

    # ------------------------------------------------------------- output

    def sorted_entries
      @entries.values.sort_by { |e| [e["owner"].to_s, e["kind"], e["name"].to_s] }
    end

    def write_contract
      entries = sorted_entries.map do |e|
        out = e.slice("api", "kind", "scope", "note", "usage", "domains", "evidence", "tiers", "inherited_from", "spelled_as", "also_defined_on",
          "roles", "receivers", "args", "blocks", "returns", "raises")
        out.reject! { |_, v| v.nil? || (v.respond_to?(:empty?) && v.empty?) }
        out["used_by"] = e["used_by"].sort.to_h
        out
      end
      doc = {
        "generated_by" => "script/inventory/aggregate.rb",
        "nokogiri" => @api["nokogiri"],
        "entries" => entries,
        "queries" => @queries.sort_by { |q| [q["language"], q["usage"], q["gem"], q["query"]] },
        "extensions" => @extensions.sort_by { |x| [x["type"], x["target"].to_s, x["gem"].to_s] },
        "ignored_calls" => @ignored.sort.to_h,
        "requires" => (@requires || {}).sort.to_h
      }
      File.write(File.join(ROOT, "inventory/contract.yml"),
        "# Generated by script/inventory/aggregate.rb; edit inventory/scope.yml instead.\n" + doc.to_yaml(line_width: -1))
    end

    # Gems in one summary column: :runtime and :test are non-XML gems by usage.
    def gems_cell(e, column)
      e["used_by"].select do |g, u|
        case column
        when :xml then domain(g) == "xml"
        when :runtime then domain(g) != "xml" && u["usage"].include?("runtime")
        when :test then domain(g) != "xml" && !u["usage"].include?("runtime")
        end
      end.map do |g, u|
        mark = if u["evidence"].include?("traced")
          ""
        else
          ((u["evidence"] == ["name match"]) ? "?" : "*")
        end
        "#{g}#{mark}"
      end.join(", ")
    end

    def write_summary
      by_scope = sorted_entries.group_by { |e| e["scope"] }
      io = +""
      io << "# Nokogiri API inventory: summary\n\n"
      io << "Generated by `script/inventory/aggregate.rb` from Nokogiri #{@api["nokogiri"]}. "
      io << "Gem marks: none = seen at runtime by the recorder, `*` = static only (receiver known), "
      io << "`?` = method-name match only (receiver unknown). Usage counts HTML-domain gems when an entry has any.\n\n"
      io << "| scope | entries | runtime | test-only |\n|---|---:|---:|---:|\n"
      by_scope.sort.each do |scope, list|
        rt = list.count { |e| e["usage"] == "runtime" }
        io << "| #{scope} | #{list.size} | #{rt} | #{list.size - rt} |\n"
      end
      by_scope.sort.each do |scope, list|
        io << "\n## #{scope}\n\n| API | kind | HTML, runtime | HTML, test-time | XML gems | note |\n|---|---|---|---|---|---|\n"
        list.each do |e|
          io << "| `#{e["api"]}` | #{e["kind"]} | #{gems_cell(e, :runtime)} | #{gems_cell(e, :test)} | " \
            "#{gems_cell(e, :xml)} | #{e["note"]} |\n"
        end
      end
      xml_queries = @queries.select { |q| domain(q["gem"]) == "xml" }
      io << "\n## Selector and XPath strings from HTML-domain gems\n\n"
      io << "#{xml_queries.size} more from XML-domain gems are only in contract.yml.\n\n"
      io << "| language | usage | gem | query | sites |\n|---|---|---|---|---|\n"
      (@queries - xml_queries).sort_by { |q| [q["language"], q["usage"], q["gem"], q["query"]] }.each do |q|
        io << "| #{q["language"]} | #{q["usage"]} | #{q["gem"]} | `#{q["query"].gsub("|", "\\|").tr("\n", " ")}` | #{q["sites"].first(3).join(", ")} |\n"
      end
      io << "\n## Extension points in HTML-domain gems (subclasses, reopened classes, patches, mixins)\n\n"
      io << "| type | API | gem | usage | site |\n|---|---|---|---|---|\n"
      @extensions.sort_by { |x| [x["type"], x["target"].to_s, x["gem"].to_s] }.each do |x|
        next if x["gem"] && domain(x["gem"]) == "xml"
        io << "| #{x["type"]} | `#{x["api"]}` | #{x["gem"] || x["label"]} | #{x["usage"]} | #{x["site"]} |\n"
      end
      File.write(File.join(ROOT, "inventory/summary.md"), io)
    end
  end
end

Aggregate::Builder.new.run if $PROGRAM_NAME == __FILE__
