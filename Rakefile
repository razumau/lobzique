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
    # sanitizer, Action Text and rails-dom-testing tests load would slow
    # every other test several times over
    rails, rest = test_task.test_globs.partition { _1.start_with?("test/sanitizer/", "test/action_text/", "test/rails_dom_testing/") }
    [rest, rails].each { ruby test_task.make_test_cmd(path: _1), verbose: test_task.verbose }
  ensure
    rm_f built_binary
  end
end

# cargo-fuzz's instrumentation, so libFuzzer sees the extension's coverage
RUBY_FUZZ_RUSTFLAGS = %w[
  -Zsanitizer=address -Cpasses=sancov-module -Cllvm-args=-sanitizer-coverage-level=4
  -Cllvm-args=-sanitizer-coverage-inline-8bit-counters -Cllvm-args=-sanitizer-coverage-pc-table
  -Cllvm-args=-sanitizer-coverage-trace-compares -Cdebug-assertions -Coverflow-checks
  -Cdebuginfo=line-tables-only
].freeze

namespace :fuzz do
  desc "Fuzz the Ruby API with Ruzzy under ASan (fuzz/ruby/api.rb; Linux, nightly Rust, " \
    "the fuzz Gemfile group). FUZZOPTS go to libFuzzer"
  task :ruby do
    begin
      require "ruzzy"
    rescue LoadError
      abort "fuzz:ruby needs Ruzzy: `bundle config set --local with fuzz`, then bundle install with clang (README)"
    end
    ENV["RUSTUP_TOOLCHAIN"] = "nightly"
    # An explicit target keeps the instrumentation out of build scripts and proc macros
    ENV["CARGO_BUILD_TARGET"] = `rustc -vV`[/^host: (.+)$/, 1]
    ENV["RB_SYS_EXTRA_CARGO_ARGS"] = "-Zbuild-std"
    ENV["RUSTFLAGS"] = RUBY_FUZZ_RUSTFLAGS.join(" ")
    Rake::Task["compile"].invoke

    corpus, artifacts = "fuzz/corpus/ruby_api", "fuzz/artifacts/ruby_api/"
    mkdir_p [corpus, artifacts]
    # The extension's ASan needs a runtime in the process, and Ruby's
    # stack overflow handling its own signal stack
    env = {
      "LD_PRELOAD" => Ruzzy::ASAN_PATH,
      "ASAN_OPTIONS" => "allocator_may_return_null=1:detect_leaks=0:use_sigaltstack=0"
    }
    # Read-only seeds after the corpus, as the Rust targets have them (fuzz.yml)
    seeds = %w[test/fixtures/wpt-parsing/tree-construction test/fixtures/html5lib-tests/tokenizer]
    sh env, "fuzz/ruby/trace.rb", corpus, *seeds, "-dict=fuzz/dom.dict", "-max_len=4096", "-timeout=10",
      "-artifact_prefix=#{artifacts}", *ENV.fetch("FUZZOPTS", "").split
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
