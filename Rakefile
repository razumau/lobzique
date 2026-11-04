# frozen_string_literal: true

require "bundler/gem_tasks"
require "minitest/test_task"

test_task = Minitest::TestTask.create do |t|
  # test/fixtures/ holds upstream test files kept as data, not tests to run
  t.test_globs = FileList["test/**/{test_*,*_test}.rb"].exclude("test/fixtures/**/*")
end

require "standard/rake"

require "rb_sys/extensiontask"

task build: :compile

GEMSPEC = Gem::Specification.load("lobzique.gemspec")

extension = RbSys::ExtensionTask.new("lobzique", GEMSPEC) do |ext|
  ext.lib_dir = "lib/lobzique"
end

# Rake's default CLEAN globs **/*~ and **/*.bak, which match upstream files kept as data
CLEAN.exclude("test/fixtures/**/*", "inventory/raw/**/*")

task default: %i[compile test standard]

# make copies the profile's library over this only when the library is newer,
# so after a build with the other profile, a compile keeps the old binary
built_binary = File.join(extension.tmp_dir, extension.platform, extension.name, RUBY_VERSION, extension.binary(extension.platform))

namespace :test do
  desc "Run the tests under GC.stress, on a dev build with rb-sys's gc-stress feature"
  task :gc_stress do
    ENV["RB_SYS_CARGO_PROFILE"] = "dev"
    ENV["RB_SYS_EXTRA_CARGO_ARGS"] = "--features gc-stress"
    ENV["LOBZIQUE_GC_STRESS"] = "1"
    ENV["LOBZIQUE_GC_STRESS_BUILD"] = "1"
    rm_f built_binary
    Rake::Task["compile"].invoke
    # Every stressed allocation runs a full GC, so the Rails libraries the
    # sanitizer tests load would slow every other test several times over
    sanitizer, rest = test_task.test_globs.partition { _1.start_with?("test/sanitizer/") }
    [rest, sanitizer].each { ruby test_task.make_test_cmd(path: _1), verbose: test_task.verbose }
  ensure
    rm_f built_binary
  end
end

desc "Compare lobzique with Nokogiri::HTML5 (DIFFOPTS='--corpus crawl' etc., see script/differential)"
task differential: :compile do
  ruby "-Ilib", "script/differential", *ENV.fetch("DIFFOPTS", "").split
end

desc "Report how much of the in-scope Nokogiri API (inventory/contract.yml) is implemented"
task progress: :compile do
  ruby "-Ilib", "script/progress.rb"
end

namespace :progress do
  desc "Like progress, and list the entries not implemented yet"
  task missing: :compile do
    ruby "-Ilib", "script/progress.rb", "--missing"
  end
end
