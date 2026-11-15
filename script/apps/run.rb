# frozen_string_literal: true

# Runs a public app's test suite as checked out (baseline), and then in each
# of the app's modes (plan.md decision 1):
#   seam    lobzique added to its Gemfile and both sanitizer vendors set to
#           Lobzique::Sanitizer in config/application.rb, as the README says
#           (plan.md phase 3);
#   facade  its nokogiri replaced with the facade gem (plan.md phase 5).
# Refs come from script/inventory/apps.tsv. The checkout goes into
# tmp/apps/src, gems into tmp/apps/bundle, logs and failing test names into
# tmp/apps/out. Uses the Ruby running this script, and compiles lobzique for
# it (run `mise run compile` afterwards if that isn't the repo's Ruby).
# Databases an app needs run in Docker containers named lobzique-apps-*.
#
#   ruby script/apps/run.rb [--mode seam|facade] [NAME...]

require "fileutils"
require "json"
require "optparse"
require_relative "../suite_runner"

APPS = {
  "campfire" => {
    repo: "basecamp/once-campfire",
    ruby: ">= 3.4.5",
    modes: %w[seam facade],
    setup: %w[bin/rails db:prepare],
    suite: %w[bin/rails test]
  },
  "fizzy" => {
    repo: "basecamp/fizzy",
    ruby: ">= 4.0.9",
    modes: %w[seam facade],
    setup: %w[bin/rails db:prepare],
    suite: %w[bin/rails test]
  },
  "loomio" => {
    repo: "loomio/loomio",
    ruby: ">= 4.0.9",
    modes: %w[seam],
    services: %w[postgres],
    env: {"TEST_DATABASE_URL" => "postgresql://postgres@localhost:5432/loomio_test"},
    setup: %w[bin/rails db:prepare],
    suite: %w[bin/rails test]
  },
  # This app calls Nokogiri directly and sets no sanitizer vendor, so seam mode
  # changes nothing.
  "lobsters" => {
    repo: "lobsters/lobsters",
    ruby: ">= 4.0.9",
    modes: %w[facade],
    runner: :rspec,
    copy: {".github/workflows/check/." => "config", "config/database.yml.sample" => "config/database.yml"},
    # The suite leaves rows that schema:load trips on.
    setup: %w[bin/rails db:drop db:create db:schema:load db:migrate],
    # css_widely_available_spec downloads browser data when it loads.
    suite: %w[bundle exec rspec --exclude-pattern spec/css_widely_available_spec.rb]
  }
}.freeze

# Started with Docker. Left running for the next run.
SERVICES = {
  "postgres" => {
    run: %w[-p 5432:5432 -e POSTGRES_HOST_AUTH_METHOD=trust postgres:18],
    ready: %w[pg_isready -U postgres -h localhost]
  }
}.freeze

VENDORS = <<~RUBY
  require "lobzique/sanitizer"
RUBY
CONFIG = <<~RUBY
  config.action_view.sanitizer_vendor = Lobzique::Sanitizer
  config.action_text.sanitizer_vendor = Lobzique::Sanitizer
RUBY
# Action Text sets its sanitizer when Action View loads.
SEAM_CHECK = <<~RUBY
  ActionView::Base
  vendors = [ActionView::Helpers::SanitizeHelper.sanitizer_vendor, ActionText::ContentHelper.sanitizer.class]
  expected = [Lobzique::Sanitizer, Lobzique::Sanitizer::SafeListSanitizer]
  abort "sanitizer vendors are \#{vendors.inspect}" unless vendors == expected
RUBY
FACADE_CHECK = <<~RUBY
  path = Gem.loaded_specs.fetch("nokogiri").full_gem_path
  abort "nokogiri is \#{path}" unless Nokogiri.equal?(Lobzique) && File.realpath(path) == %s
RUBY

root = File.expand_path("../..", __dir__)
tmp = File.join(root, "tmp/apps")
refs = File.readlines(File.join(root, "script/inventory/apps.tsv"), chomp: true).to_h { _1.split("\t") }
# The SSRF guards of apps resolve hosts. See resolv_offline.rb.
env = {
  "LANG" => "C.UTF-8",
  "RAILS_ENV" => "test",
  "RUBYOPT" => "-r#{File.join(__dir__, "resolv_offline.rb")}"
}

def use_seams(dir, root)
  File.write(File.join(dir, "Gemfile"), %(\ngem "lobzique", path: "#{root}"\n), mode: "a")
  application = File.join(dir, "config/application.rb")
  source = File.read(application)
  source.sub!(/^require "rails\/all"\n/) { _1 + VENDORS } or raise "no require \"rails/all\" in #{application}"
  source.sub!(/^(\s*)config\.load_defaults .*\n/) { _1 + CONFIG.gsub(/^/, $1) } or raise "no load_defaults in #{application}"
  File.write(application, source)
end

# Only a bare declaration is replaced. A declaration with options would run
# the facade under other conditions than the baseline.
def use_facade(dir, root)
  gemfile = File.join(dir, "Gemfile")
  source = File.read(gemfile).gsub(/^[ \t]*gem ["']nokogiri["'][ \t]*(?:#.*)?$/, "")
  raise "nokogiri has options in #{gemfile}" if source.match?(/^\s*gem\W+nokogiri["']/)
  File.write(gemfile, source + %(\ngem "nokogiri", path: "#{root}"\n))
end

def start_service(name)
  service = SERVICES.fetch(name)
  container = "lobzique-apps-#{name}"
  unless system("docker", "start", container, out: File::NULL, err: File::NULL)
    system("docker", "run", "-d", "--name", container, *service[:run], out: File::NULL, exception: true)
  end
  60.times do
    return if system("docker", "exec", container, *service[:ready], out: File::NULL, err: File::NULL)
    sleep 1
  end
  abort "#{name} isn't ready"
end

# Both return {summary:, failed:}, with no summary for a suite that did
# not finish.
def minitest_result(log)
  failed = SuiteRunner.read(log).scan(/^(\S+#test_.+?)(?: \[[^\]]+\])?:$/).flatten.uniq
  {summary: SuiteRunner.minitest_summary(log), failed:}
end

# Descriptions repeat within a file. Ids do not. A file that fails to load
# leaves its examples out of the report, so that run did not finish either.
def rspec_result(json)
  report = JSON.parse(File.read(json))
  failed = report["examples"].select { _1["status"] == "failed" }.map { "#{_1["id"]} #{_1["full_description"]}" }
  loaded = report.dig("summary", "errors_outside_of_examples_count").zero?
  {summary: (report["summary_line"] if loaded), failed:}
rescue Errno::ENOENT, JSON::ParserError
  {summary: nil, failed: []}
end

only = nil
OptionParser.new { |o| o.on("--mode MODE", %w[seam facade]) { only = _1 } }.parse!
names = ARGV.empty? ? APPS.keys : ARGV
names, skipped = names.partition { only.nil? || APPS.fetch(_1)[:modes].include?(only) }
warn "no #{only} mode: #{skipped.join(", ")}" if skipped.any?
exit if names.empty?
# Left by apps that are no longer listed.
FileUtils.rm_rf(Dir.glob(File.join(tmp, "out/*")).reject { APPS.key?(File.basename(_1)) })
# Bundler does not build the extension of a path gem. lib/ holds the build
# of whichever Ruby ran last. A build for the ABI of another Ruby loads
# and misbehaves.
SuiteRunner.run(env.except("RAILS_ENV"), "bundle", "install", "--quiet", chdir: root)
SuiteRunner.run(env.except("RAILS_ENV"), "bundle", "exec", "rake", "compile", chdir: root)
results = names.map do |name|
  app = APPS.fetch(name)
  unless Gem::Requirement.new(app[:ruby]).satisfied_by?(Gem::Version.new(RUBY_VERSION))
    abort "#{name} needs Ruby #{app[:ruby]}, this is #{RUBY_VERSION}"
  end
  app.fetch(:services, []).each { start_service(_1) }
  app_env = env.merge(app.fetch(:env, {}))
  dir = File.join(tmp, "src", name)
  out = File.join(tmp, "out", name)
  FileUtils.mkdir_p(out)
  modes = app[:modes].select { only.nil? || _1 == only }
  # Left by modes that are no longer listed.
  FileUtils.rm_f(Dir.children(out).reject { ["baseline", *app[:modes]].include?(File.basename(_1, ".*")) }.map { File.join(out, _1) })
  ["baseline", *modes].to_h do |mode|
    SuiteRunner.checkout(app_env, app[:repo], refs.fetch(app[:repo]), dir)
    app.fetch(:copy, {}).each { |from, to| FileUtils.cp_r(File.join(dir, from), File.join(dir, to)) }
    use_seams(dir, root) if mode == "seam"
    use_facade(dir, root) if mode == "facade"
    SuiteRunner.run(app_env, "bundle", "config", "set", "--local", "path", File.join(tmp, "bundle"), chdir: dir)
    SuiteRunner.run(app_env, "bundle", "install", "--quiet", chdir: dir)
    SuiteRunner.run(app_env, *app[:setup], chdir: dir)
    SuiteRunner.run(app_env, "bin/rails", "runner", SEAM_CHECK, chdir: dir) if mode == "seam"
    SuiteRunner.run(app_env, "bin/rails", "runner", format(FACADE_CHECK, File.realpath("facade", root).dump), chdir: dir) if mode == "facade"
    log = File.join(out, "#{mode}.log")
    json = File.join(out, "#{mode}.json")
    FileUtils.rm_f(Dir.glob(File.join(out, "#{mode}.*")))
    rspec = app[:runner] == :rspec
    suite = app[:suite]
    suite += ["--format", "progress", "--format", "json", "--out", json] if rspec
    status = SuiteRunner.run_suite(app_env, suite, chdir: dir, log:)
    result = rspec ? rspec_result(json) : minitest_result(log)
    File.write(File.join(out, "#{mode}.failures"), result[:failed].sort.map { "#{_1}\n" }.join)
    [mode, {**result, status:, log:}]
  end
end

puts "Failing test names are in tmp/apps/out."
puts
puts "| app | mode | result | new failures | fixed |"
puts "|---|---|---|---|---|"
names.zip(results).each do |name, by_mode|
  base = by_mode.fetch("baseline")
  puts "| #{name} | baseline | #{SuiteRunner.summary_cell(base)} | | |"
  by_mode.except("baseline").each do |mode, result|
    new_failures = SuiteRunner.count(result[:failed] - base[:failed], base, result)
    fixed = SuiteRunner.count(base[:failed] - result[:failed], base, result)
    puts "| #{name} | #{mode} | #{SuiteRunner.summary_cell(result)} | #{new_failures} | #{fixed} |"
  end
end
