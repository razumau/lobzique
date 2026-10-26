# frozen_string_literal: true

require "bundler/gem_tasks"
require "minitest/test_task"

Minitest::TestTask.create do |t|
  # test/fixtures/ holds upstream test files kept as data, not tests to run
  t.test_globs = FileList["test/**/{test_*,*_test}.rb"].exclude("test/fixtures/**/*")
end

require "standard/rake"

require "rb_sys/extensiontask"

task build: :compile

GEMSPEC = Gem::Specification.load("lobzique.gemspec")

RbSys::ExtensionTask.new("lobzique", GEMSPEC) do |ext|
  ext.lib_dir = "lib/lobzique"
end

# Rake's default CLEAN globs **/*~ and **/*.bak, which match upstream files kept as data
CLEAN.exclude("test/fixtures/**/*", "inventory/raw/**/*")

task default: %i[compile test standard]

namespace :test do
  desc "Run the tests under GC.stress, on a dev build with rb-sys's gc-stress feature"
  task :gc_stress do
    ENV["RB_SYS_CARGO_PROFILE"] = "dev"
    ENV["RB_SYS_EXTRA_CARGO_ARGS"] = "--features gc-stress"
    ENV["LOBZIQUE_GC_STRESS"] = "1"
    Rake::Task["compile"].invoke
    Rake::Task["test"].invoke
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
