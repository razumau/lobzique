# frozen_string_literal: true

# Runs a public app's test suite twice: as checked out (baseline), and with
# lobzique added to its Gemfile and both sanitizer vendors set to
# Lobzique::Sanitizer in config/application.rb, as the README says (plan.md
# phase 3). Refs come from script/inventory/apps.tsv. The checkout goes into
# tmp/apps/src, gems into tmp/apps/bundle, logs and failing test names into
# tmp/apps/out. Uses the Ruby running this script, and compiles lobzique for
# it (run `mise run compile` afterwards if that isn't the repo's Ruby).
#
#   ruby script/apps/run.rb [NAME...]

require "fileutils"
require_relative "../suite_runner"

APPS = {
  "campfire" => {
    repo: "basecamp/once-campfire",
    ruby: ">= 3.4.5",
    setup: %w[bin/rails db:prepare],
    suite: %w[bin/rails test]
  }
}.freeze
MODES = %w[baseline lobzique].freeze

VENDORS = <<~RUBY
  require "lobzique/sanitizer"
RUBY
CONFIG = <<~RUBY
  config.action_view.sanitizer_vendor = Lobzique::Sanitizer
  config.action_text.sanitizer_vendor = Lobzique::Sanitizer
RUBY
# Action Text sets its sanitizer once Action View loads.
CHECK = <<~RUBY
  ActionView::Base
  vendors = [ActionView::Helpers::SanitizeHelper.sanitizer_vendor, ActionText::ContentHelper.sanitizer.class]
  expected = [Lobzique::Sanitizer, Lobzique::Sanitizer::SafeListSanitizer]
  abort "sanitizer vendors are \#{vendors.inspect}" unless vendors == expected
RUBY

root = File.expand_path("../..", __dir__)
tmp = File.join(root, "tmp/apps")
refs = File.readlines(File.join(root, "script/inventory/apps.tsv"), chomp: true).to_h { _1.split("\t") }
env = {
  "LANG" => "C.UTF-8",
  "RAILS_ENV" => "test",
  "RUBYOPT" => "-r#{File.join(__dir__, "resolv_offline.rb")}"
}

def use_lobzique(dir, root)
  File.write(File.join(dir, "Gemfile"), %(\ngem "lobzique", path: "#{root}"\n), mode: "a")
  application = File.join(dir, "config/application.rb")
  source = File.read(application)
  source.sub!(/^require "rails\/all"\n/) { _1 + VENDORS } or raise "no require \"rails/all\" in #{application}"
  source.sub!(/^(\s*)config\.load_defaults .*\n/) { _1 + CONFIG.gsub(/^/, $1) } or raise "no load_defaults in #{application}"
  File.write(application, source)
end

def failures(log)
  SuiteRunner.read(log).scan(/^(\S+#test_.+?)(?: \[[^\]]+\])?:$/).flatten
end

names = ARGV.empty? ? APPS.keys : ARGV
# A path gem's extension isn't built by Bundler; lib/ holds whichever Ruby's
# build ran last, and one built for another Ruby's ABI loads and misbehaves.
SuiteRunner.run(env.except("RAILS_ENV"), "bundle", "install", "--quiet", chdir: root)
SuiteRunner.run(env.except("RAILS_ENV"), "bundle", "exec", "rake", "compile", chdir: root)
results = names.map do |name|
  app = APPS.fetch(name)
  unless Gem::Requirement.new(app[:ruby]).satisfied_by?(Gem::Version.new(RUBY_VERSION))
    abort "#{name} needs Ruby #{app[:ruby]}, this is #{RUBY_VERSION}"
  end
  dir = File.join(tmp, "src", name)
  out = File.join(tmp, "out", name)
  FileUtils.mkdir_p(out)
  MODES.to_h do |mode|
    SuiteRunner.checkout(env, app[:repo], refs.fetch(app[:repo]), dir)
    use_lobzique(dir, root) if mode == "lobzique"
    SuiteRunner.run(env, "bundle", "config", "set", "--local", "path", File.join(tmp, "bundle"), chdir: dir)
    SuiteRunner.run(env, "bundle", "install", "--quiet", chdir: dir)
    SuiteRunner.run(env, *app[:setup], chdir: dir)
    SuiteRunner.run(env, "bin/rails", "runner", CHECK, chdir: dir) if mode == "lobzique"
    log = File.join(out, "#{mode}.log")
    status = SuiteRunner.run_suite(env, app[:suite], chdir: dir, log:)
    failed = failures(log).sort
    File.write(File.join(out, "#{mode}.failures"), failed.map { "#{_1}\n" }.join)
    [mode, {summary: SuiteRunner.minitest_summary(log), status:, log:, failed: failed.to_set}]
  end
end

puts "Failing test names are in tmp/apps/out."
puts
puts "| app | baseline | lobzique | new failures | fixed |"
puts "|---|---|---|---|---|"
names.zip(results).each do |name, by_mode|
  base, lobzique = by_mode.values_at(*MODES)
  new_failures = SuiteRunner.count(lobzique[:failed] - base[:failed], base, lobzique)
  fixed = SuiteRunner.count(base[:failed] - lobzique[:failed], base, lobzique)
  puts "| #{name} | #{SuiteRunner.summary_cell(base)} | #{SuiteRunner.summary_cell(lobzique)} | #{new_failures} | #{fixed} |"
end
