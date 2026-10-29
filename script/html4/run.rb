# frozen_string_literal: true

# Runs the suites of gems that parse with Nokogiri's HTML4 names three times:
# on libxml2 (baseline), and through shim.rb in each HTML4_MODE. This is the
# measurement behind plan.md decision 3. Checkouts come from
# script/inventory/targets.yml, go into tmp/html4/src, and are bundled into
# tmp/html4/bundle with the Nokogiri version in our Gemfile.lock and the Ruby
# running this script. Logs and failing test names go to tmp/html4/out.
#
#   ruby script/html4/run.rb [NAME...]

require "fileutils"
require "json"
require "yaml"

SUITES = %w[
  roadie reverse_markdown html2text inky-rb premailer html-pipeline mechanize
  rails-dom-testing loofah rails-html-sanitizer
].freeze
MODES = %w[baseline html5-tree html5].freeze

root = File.expand_path("../..", __dir__)
tmp = File.join(root, "tmp/html4")
shim = File.join(__dir__, "shim.rb")
targets = YAML.safe_load_file(File.join(root, "script/inventory/targets.yml"))
targets = %w[tier1 tier2].map { |tier| targets.fetch(tier) }.reduce(:merge)
nokogiri = File.read(File.join(root, "Gemfile.lock"))[/^    nokogiri \(([\d.]+)/, 1]
# Keep checkouts' own .ruby-version files from switching Rubies under shims.
path = {"PATH" => "#{RbConfig::CONFIG["bindir"]}:#{ENV["PATH"]}"}

def run(env, *cmd, chdir:)
  system(env, *cmd, chdir: chdir, exception: true)
end

# Pins the oracle's Nokogiri over whatever the checkout's Gemfile asks for.
def pin_nokogiri(dir, version)
  gemfile = File.join(dir, "Gemfile")
  lines = File.readlines(gemfile).grep_v(/^\s*gem\s+["']nokogiri["']/)
  File.write(gemfile, lines.join + %(\ngem "nokogiri", "#{version}"\n))
end

def setup(name, target, src, bundle, nokogiri, env)
  source = target.fetch("source")
  dir = File.join(src, name)
  unless Dir.exist?(dir)
    run(env, "git", "init", "-q", dir, chdir: src)
    run(env, "git", "fetch", "-q", "--depth", "1", "https://github.com/#{source["repo"]}.git", source["ref"], chdir: dir)
    run(env, "git", "checkout", "-q", "FETCH_HEAD", chdir: dir)
  end
  run(env, "git", "checkout", "-q", "--", "Gemfile", chdir: dir)
  pin_nokogiri(dir, nokogiri)
  run(env, "bundle", "config", "set", "--local", "path", bundle, chdir: dir)
  run(env, "bundle", "install", "--quiet", chdir: dir)
  dir
end

def failures(log, json)
  if File.exist?(json)
    JSON.parse(File.read(json))["examples"].select { |e| e["status"] == "failed" }.map { |e| e["full_description"] }
  else
    File.read(log).scan(/^\s*\d+\) (?:Failure|Error):\n(.+?)(?: \[[^\]]+\])?:$/).flatten
  end
end

def summary(log, json)
  return JSON.parse(File.read(json))["summary_line"] if File.exist?(json)

  File.read(log).scan(/^\d+ runs, .*$/).last || "no summary, see #{log}"
end

names = ARGV.empty? ? SUITES : ARGV
src = File.join(tmp, "src")
FileUtils.mkdir_p(src)
dirs = names.to_h { |name| [name, setup(name, targets.fetch(name), src, File.join(tmp, "bundle"), nokogiri, path)] }

results = names.map do |name|
  Thread.new do
    out = File.join(tmp, "out", name)
    FileUtils.mkdir_p(out)
    MODES.to_h do |mode|
      log = File.join(out, "#{mode}.log")
      json = File.join(out, "#{mode}.json")
      FileUtils.rm_f(json)
      suite = targets.fetch(name).fetch("suite")
      suite += " --format json --out #{json}" if suite.include?("rspec")
      shimmed = mode != "baseline"
      env = path.merge(
        "HTML4_MODE" => (mode if shimmed),
        "RUBYOPT" => ("-rbundler/setup -r#{shim}" if shimmed)
      )
      system(env, suite, chdir: dirs.fetch(name), out: log, err: [:child, :out])
      failed = failures(log, json).sort
      File.write(File.join(out, "#{mode}.failures"), failed.map { "#{_1}\n" }.join)
      [mode, {summary: summary(log, json), failed: failed.to_set}]
    end
  end
end.map(&:value)

puts "Nokogiri #{nokogiri}. New failures are counted against baseline; failing test names are in tmp/html4/out."
puts
puts "| suite | baseline | html5-tree: new failures | html5: new failures | only in html5 |"
puts "|---|---|---|---|---|"
names.zip(results).each do |name, by_mode|
  base, tree, html5 = by_mode.values_at(*MODES).map { _1[:failed] }
  only_html5 = html5 - base - tree
  puts "| #{name} | #{by_mode["baseline"][:summary]} | #{(tree - base).size} | #{(html5 - base).size} | #{only_html5.size} |"
end
