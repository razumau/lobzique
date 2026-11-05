# frozen_string_literal: true

# Runs Rails components' own suites (plan.md phase 4): as checked out
# (baseline), and on lobzique: Lobzique::Sanitizer as the sanitizer vendors,
# lobzique/action_text and lobzique/rails_dom_testing required, as the README
# says, and rails-dom-testing on :html5. Rails is checked out at the tag of
# the actiontext in our Gemfile.lock, into tmp/rails/src; gems go into
# tmp/rails/bundle, logs and failing test names into tmp/rails/out.
#
#   ruby script/rails/run.rb [SUITE...]

require "fileutils"
require_relative "../suite_runner"

# app: the dummy app's config/application.rb, which loads its defaults.
# Suites without one run on rails-dom-testing's :html4 and
# Rails::HTML4::Sanitizer, so they run once more on Nokogiri with what
# load_defaults 7.1 and later set (html5), and lobzique is compared with
# that.
SUITES = {
  "actiontext" => {helper: "test/test_helper.rb", app: "test/dummy/config/application.rb"},
  "actionview" => {helper: "test/abstract_unit.rb"},
  "actionpack" => {helper: "test/abstract_unit.rb"}
}.freeze
MODES = %w[baseline html5 lobzique].freeze

# Database adapters, cloud storage, job and cable backends, linters and docs
# need services or native libraries these suites don't use.
WITHOUT = "db:job:cable:storage:rubocop:mdl:doc"

APP_REQUIRES = <<~RUBY
  require "lobzique/sanitizer"
  require "lobzique/action_text"
  require "lobzique/rails_dom_testing"
RUBY
APP_CONFIG = <<~RUBY
  config.action_view.sanitizer_vendor = Lobzique::Sanitizer
  config.action_text.sanitizer_vendor = Lobzique::Sanitizer
RUBY
# Appended to the helper of a suite without an app
HTML5_DEFAULTS = <<~RUBY
  Rails::Dom::Testing.default_html_version = :html5
  ActionView::Helpers::SanitizeHelper.sanitizer_vendor = %s
RUBY
NO_APP_LOBZIQUE = <<~RUBY
  require "lobzique/sanitizer"
  require "lobzique/rails_dom_testing"
  #{HTML5_DEFAULTS % "Lobzique::Sanitizer"}
RUBY
# Appended to the suite's helper, once the app, if any, has booted
CHECK = <<~RUBY
  parsers = [Rails::Dom::Testing.html_document, ActionView::Helpers::SanitizeHelper.sanitizer_vendor]
  abort "rails-dom-testing and Action View parse with \#{parsers.inspect}" unless parsers == [Lobzique::HTML5::Document, Lobzique::Sanitizer]
RUBY
ACTION_TEXT_CHECK = <<~RUBY
  ActionView::Base
  parsers = [ActionText.html_document_class, ActionText::ContentHelper.sanitizer.class]
  abort "Action Text parses with \#{parsers.inspect}" unless parsers == [Lobzique::HTML5::Document, Lobzique::Sanitizer::SafeListSanitizer]
RUBY

root = File.expand_path("../..", __dir__)
tmp = File.join(root, "tmp/rails")
dir = File.join(tmp, "src/rails")
lock = File.read(File.join(root, "Gemfile.lock"))
ref = "v" + lock[/^    actiontext \(([\d.]+)\)/, 1]
# Every mode gets the sanitizer gems lobzique is tested with: the tag's lock
# may predate the versions Lobzique::Sanitizer accepts.
PINS = %w[loofah rails-html-sanitizer rails-dom-testing].map { %(gem "#{_1}", "#{lock[/^    #{_1} \(([\d.]+)\)/, 1]}"\n) }.join
env = {"LANG" => "C.UTF-8", "BUNDLE_WITHOUT" => WITHOUT}

def modes(options) = options[:app] ? MODES - ["html5"] : MODES

# Rails' Gemfile evaluates a .Gemfile beside it, so its own stays as it is.
def configure(mode, dir, suite, options, root)
  helper = File.join(dir, suite, options[:helper])
  case mode
  when "html5"
    File.write(helper, "\n" + HTML5_DEFAULTS % "Rails::HTML5::Sanitizer", mode: "a")
  when "lobzique"
    File.write(File.join(dir, ".Gemfile"), %(gem "lobzique", path: "#{root}"\n), mode: "a")
    if options[:app]
      application = File.join(dir, suite, options[:app])
      source = File.read(application)
      source.sub!(/^require "rails\/all"\n/) { _1 + APP_REQUIRES } or raise "no require \"rails/all\" in #{application}"
      source.sub!(/^(\s*)config\.load_defaults .*\n/) { _1 + APP_CONFIG.gsub(/^/, $1) } or raise "no load_defaults in #{application}"
      File.write(application, source)
      File.write(helper, "\n" + CHECK + ACTION_TEXT_CHECK, mode: "a")
    else
      File.write(helper, "\n" + NO_APP_LOBZIQUE + CHECK, mode: "a")
    end
  end
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
  modes(options).to_h do |mode|
    SuiteRunner.checkout(env, "rails/rails", "refs/tags/#{ref}", dir)
    File.write(File.join(dir, ".Gemfile"), PINS)
    configure(mode, dir, name, options, root)
    SuiteRunner.run(env, "bundle", "config", "set", "--local", "path", File.join(tmp, "bundle"), chdir: dir)
    SuiteRunner.run(env, "bundle", "install", "--quiet", chdir: dir)
    log = File.join(out, "#{mode}.log")
    status = SuiteRunner.run_suite(env, %w[bundle exec bin/test], chdir: File.join(dir, name), log:)
    failed = failures(log).sort
    File.write(File.join(out, "#{mode}.failures"), failed.map { "#{_1}\n" }.join)
    [mode, {summary: SuiteRunner.minitest_summary(log), status:, log:, failed: failed.to_set}]
  end
end

puts "Rails #{ref}. lobzique is compared with html5 where it ran, else with baseline."
puts "Failing test names are in tmp/rails/out."
puts
puts "| suite | baseline | html5 | lobzique | new failures | fixed |"
puts "|---|---|---|---|---|---|"
names.zip(results).each do |name, by_mode|
  reference = by_mode["html5"] || by_mode["baseline"]
  lobzique = by_mode["lobzique"]
  new_failures = SuiteRunner.count(lobzique[:failed] - reference[:failed], reference, lobzique)
  fixed = SuiteRunner.count(reference[:failed] - lobzique[:failed], reference, lobzique)
  cells = MODES.map { by_mode[_1] ? SuiteRunner.summary_cell(by_mode[_1]) : "–" }
  puts "| #{name} | #{cells.join(" | ")} | #{new_failures} | #{fixed} |"
end
