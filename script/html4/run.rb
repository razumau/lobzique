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
require_relative "../suite_runner"

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

# Pins the oracle's Nokogiri over whatever the checkout's Gemfile asks for.
def pin_nokogiri(dir, version)
  gemfile = File.join(dir, "Gemfile")
  lines = File.readlines(gemfile).grep_v(/^\s*gem\s+["']nokogiri["']/)
  File.write(gemfile, lines.join + %(\ngem "nokogiri", "#{version}"\n))
end

def setup(name, target, src, bundle, nokogiri)
  source = target.fetch("source")
  dir = File.join(src, name)
  SuiteRunner.checkout({}, source["repo"], source["ref"], dir)
  pin_nokogiri(dir, nokogiri)
  SuiteRunner.run({}, "bundle", "config", "set", "--local", "path", bundle, chdir: dir)
  SuiteRunner.run({}, "bundle", "install", "--quiet", chdir: dir)
  dir
end

def failures(log, json)
  if File.exist?(json)
    JSON.parse(File.read(json))["examples"].select { |e| e["status"] == "failed" }.map { |e| e["full_description"] }
  else
    SuiteRunner.read(log).scan(/^\s*\d+\) (?:Failure|Error):\n(.+?)(?: \[[^\]]+\])?:$/).flatten
  end
end

def summary(log, json)
  return JSON.parse(File.read(json))["summary_line"] if File.exist?(json)

  SuiteRunner.minitest_summary(log)
end

names = ARGV.empty? ? SUITES : ARGV
src = File.join(tmp, "src")
dirs = names.to_h { |name| [name, setup(name, targets.fetch(name), src, File.join(tmp, "bundle"), nokogiri)] }

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
      env = {
        "HTML4_MODE" => (mode if shimmed),
        "RUBYOPT" => ("-rbundler/setup -r#{shim}" if shimmed)
      }
      status = SuiteRunner.run_suite(env, suite, chdir: dirs.fetch(name), log:)
      failed = failures(log, json).sort
      File.write(File.join(out, "#{mode}.failures"), failed.map { "#{_1}\n" }.join)
      [mode, {summary: summary(log, json), status:, log:, failed: failed.to_set}]
    end
  end
end.map(&:value)

puts "Nokogiri #{nokogiri}. New failures are counted against baseline; failing test names are in tmp/html4/out."
puts
puts "| suite | baseline | html5-tree: new failures | html5: new failures | only in html5 |"
puts "|---|---|---|---|---|"
names.zip(results).each do |name, by_mode|
  results = by_mode.values_at(*MODES)
  base, tree, html5 = results.map { _1[:failed] }
  cells = [
    SuiteRunner.summary_cell(results.first),
    SuiteRunner.count(tree - base, results[0], results[1]),
    SuiteRunner.count(html5 - base, results[0], results[2]),
    SuiteRunner.count(html5 - base - tree, *results)
  ]
  puts "| #{name} | #{cells.join(" | ")} |"
end
