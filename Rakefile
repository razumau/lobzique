# frozen_string_literal: true

require "bundler/gem_tasks"
require "minitest/test_task"

test_task = Minitest::TestTask.create do |t|
  # test/fixtures/ holds upstream test files kept as data, not tests to run;
  # test/facade/ runs in its own bundle (test:facade)
  t.test_globs = FileList["test/**/{test_*,*_test}.rb"].exclude("test/fixtures/**/*", "test/facade/**/*")
end

desc "Run the nokogiri facade's tests in its own bundle (test/facade/Gemfile), where nokogiri is lobzique"
task "test:facade" => :compile do
  Bundler.with_unbundled_env do
    env = {"BUNDLE_GEMFILE" => File.expand_path("test/facade/Gemfile", __dir__)}
    sh env, "bundle", "install", "--quiet"
    files = FileList["test/facade/test_*.rb"]
    sh env, "bundle", "exec", "ruby", "-e", "ARGV.each { require File.expand_path(_1) }", *files
  end
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
  desc "Run the tests under GC.stress on the extension in lib/, a process per file " \
    "(STRESS_JOBS at a time, default one per CPU)"
  task :stress do
    require "etc"
    require "open3"
    # Every stressed allocation runs a full GC, whose cost grows with the
    # heap: a file runs alone so it doesn't mark the Rails libraries another
    # loads, and without Bundler, whose code is half of a small heap. The
    # slow Rails files start first.
    rails, rest = test_task.test_globs.partition { _1.start_with?("test/sanitizer/", "test/action_text/", "test/rails_dom_testing/") }
    env = Bundler.unbundled_env.merge(
      "RUBYLIB" => $LOAD_PATH.join(File::PATH_SEPARATOR),
      "LOBZIQUE_GC_STRESS" => "1",
      "LOBZIQUE_GC_STRESS_BUILD" => ENV["LOBZIQUE_GC_STRESS_BUILD"]
    )
    queue = Queue.new.tap { |q| (rails + rest).each { q << _1 } }.close
    output = Mutex.new
    failed = []
    Array.new(Integer(ENV.fetch("STRESS_JOBS", Etc.nprocessors))) do
      Thread.new do
        while (file = queue.pop)
          started = Process.clock_gettime(Process::CLOCK_MONOTONIC)
          log, status = Open3.capture2e(env, "#{RbConfig.ruby} #{test_task.make_test_cmd(path: [file])}")
          output.synchronize do
            puts "== #{file} (#{(Process.clock_gettime(Process::CLOCK_MONOTONIC) - started).round}s)", log
            failed << file unless status.success?
          end
        end
      end
    end.each(&:join)
    abort "Failed under GC.stress: #{failed.join(", ")}" if failed.any?
  end

  desc "Run the tests under GC.stress, on a dev build with rb-sys's gc-stress feature"
  task :gc_stress do
    ENV["RB_SYS_CARGO_PROFILE"] = "dev"
    ENV["RB_SYS_EXTRA_CARGO_ARGS"] = "--features gc-stress"
    ENV["LOBZIQUE_GC_STRESS_BUILD"] = "1"
    rm_f built_binary
    Rake::Task["compile"].invoke
    Rake::Task["test:stress"].invoke
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
