# frozen_string_literal: true

# Runs Rails components' own suites twice (plan.md phase 4): as checked out
# (baseline), and with their dummy app on lobzique: Lobzique::Sanitizer as
# both sanitizer vendors and lobzique/action_text required, as the README
# says. Rails is checked out at the tag of the actiontext in our
# Gemfile.lock, into tmp/rails/src; gems go into tmp/rails/bundle, logs and
# failing test names into tmp/rails/out.
#
#   ruby script/rails/run.rb [SUITE...]

require "fileutils"
require_relative "../suite_runner"

SUITES = {
  "actiontext" => {app: "test/dummy/config/application.rb", helper: "test/test_helper.rb"}
}.freeze
MODES = %w[baseline lobzique].freeze

# Database adapters, cloud storage, job and cable backends, linters and docs
# need services or native libraries these suites don't use.
WITHOUT = "db:job:cable:storage:rubocop:mdl:doc"

REQUIRES = <<~RUBY
  require "lobzique/sanitizer"
  require "lobzique/action_text"
RUBY
CONFIG = <<~RUBY
  config.action_view.sanitizer_vendor = Lobzique::Sanitizer
  config.action_text.sanitizer_vendor = Lobzique::Sanitizer
RUBY
# Appended to the suite's test helper, once the app has booted
CHECK = <<~RUBY
  ActionView::Base
  parsers = [ActionText.html_document_class, ActionText::ContentHelper.sanitizer.class]
  abort "Action Text parses with \#{parsers.inspect}" unless parsers == [Lobzique::HTML5::Document, Lobzique::Sanitizer::SafeListSanitizer]
RUBY

root = File.expand_path("../..", __dir__)
tmp = File.join(root, "tmp/rails")
dir = File.join(tmp, "src/rails")
lock = File.read(File.join(root, "Gemfile.lock"))
ref = "v" + lock[/^    actiontext \(([\d.]+)\)/, 1]
# Both modes get the sanitizer gems lobzique is tested with: the tag's lock
# may predate the versions Lobzique::Sanitizer accepts.
PINS = %w[loofah rails-html-sanitizer].map { %(gem "#{_1}", "#{lock[/^    #{_1} \(([\d.]+)\)/, 1]}"\n) }.join
env = {"LANG" => "C.UTF-8", "BUNDLE_WITHOUT" => WITHOUT}

# Rails' Gemfile evaluates a .Gemfile beside it, so its own stays as it is.
def use_lobzique(dir, suite, options, root)
  File.write(File.join(dir, ".Gemfile"), %(gem "lobzique", path: "#{root}"\n), mode: "a")
  application = File.join(dir, suite, options[:app])
  source = File.read(application)
  source.sub!(/^require "rails\/all"\n/) { _1 + REQUIRES } or raise "no require \"rails/all\" in #{application}"
  source.sub!(/^(\s*)config\.load_defaults .*\n/) { _1 + CONFIG.gsub(/^/, $1) } or raise "no load_defaults in #{application}"
  File.write(application, source)
  File.write(File.join(dir, suite, options[:helper]), "\n" + CHECK, mode: "a")
end

def failures(log)
  SuiteRunner.read(log).scan(/^(\S+#test_.+?)(?: \[[^\]]+\])?:$/).flatten
end

names = ARGV.empty? ? SUITES.keys : ARGV
# A path gem's extension isn't built by Bundler; lib/ holds whichever Ruby's
# build ran last.
SuiteRunner.run(env.except("BUNDLE_WITHOUT"), "bundle", "exec", "rake", "compile", chdir: root)
results = names.map do |name|
  options = SUITES.fetch(name)
  out = File.join(tmp, "out", name)
  FileUtils.mkdir_p(out)
  MODES.to_h do |mode|
    SuiteRunner.checkout(env, "rails/rails", "refs/tags/#{ref}", dir)
    File.write(File.join(dir, ".Gemfile"), PINS)
    use_lobzique(dir, name, options, root) if mode == "lobzique"
    SuiteRunner.run(env, "bundle", "config", "set", "--local", "path", File.join(tmp, "bundle"), chdir: dir)
    SuiteRunner.run(env, "bundle", "install", "--quiet", chdir: dir)
    log = File.join(out, "#{mode}.log")
    status = SuiteRunner.run_suite(env, %w[bundle exec bin/test], chdir: File.join(dir, name), log:)
    failed = failures(log).sort
    File.write(File.join(out, "#{mode}.failures"), failed.map { "#{_1}\n" }.join)
    [mode, {summary: SuiteRunner.minitest_summary(log), status:, log:, failed: failed.to_set}]
  end
end

puts "Rails #{ref}. Failing test names are in tmp/rails/out."
puts
puts "| suite | baseline | lobzique | new failures | fixed |"
puts "|---|---|---|---|---|"
names.zip(results).each do |name, by_mode|
  base, lobzique = by_mode.values_at(*MODES)
  new_failures = SuiteRunner.count(lobzique[:failed] - base[:failed], base, lobzique)
  fixed = SuiteRunner.count(base[:failed] - lobzique[:failed], base, lobzique)
  puts "| #{name} | #{SuiteRunner.summary_cell(base)} | #{SuiteRunner.summary_cell(lobzique)} | #{new_failures} | #{fixed} |"
end
