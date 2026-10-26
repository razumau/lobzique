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

task default: %i[compile test standard]

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
