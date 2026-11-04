# frozen_string_literal: true

# Checkouts and suite runs shared by script/html4/run.rb and script/apps/run.rb.

require "fileutils"
require "rbconfig"

module SuiteRunner
  module_function

  # Drops the caller's Bundler setup (`bundle exec`, an exported BUNDLE_GEMFILE),
  # which would make a checkout resolve our Gemfile. The Ruby running this
  # script goes first in PATH, so checkouts' .ruby-version files don't switch
  # Rubies under shims.
  def env(vars)
    base = defined?(Bundler) ? Bundler.unbundled_env : ENV.to_h
    base.merge("PATH" => "#{RbConfig::CONFIG["bindir"]}:#{base["PATH"]}", **vars)
  end

  def run(vars, *cmd, chdir:)
    system(env(vars), *cmd, chdir:, unsetenv_others: true, exception: true)
  end

  # The ref is fetched into refs/pinned/<ref>, so a checkout whose ref changed
  # or whose fetch failed fetches again, and one that has it doesn't.
  def checkout(vars, repo, ref, dir)
    pinned = "refs/pinned/#{ref}"
    FileUtils.mkdir_p(dir)
    run(vars, "git", "init", "-q", chdir: dir)
    unless system("git", "rev-parse", "-q", "--verify", "#{pinned}^{commit}", chdir: dir, out: File::NULL)
      run(vars, "git", "fetch", "-q", "--depth", "1", "https://github.com/#{repo}.git", "+#{ref}:#{pinned}", chdir: dir)
    end
    run(vars, "git", "checkout", "-q", "-f", pinned, chdir: dir)
  end

  # Returns the exit status. Minitest and RSpec exit 1 when tests fail, so a
  # crash shows as a missing summary instead.
  def run_suite(vars, cmd, chdir:, log:)
    system(env(vars), *cmd, chdir:, out: log, err: [:child, :out], unsetenv_others: true)
    $?.exitstatus
  end

  def read(log)
    File.read(log, encoding: "UTF-8").scrub
  end

  def minitest_summary(log)
    read(log).scan(/^\d+ runs, .*$/).last
  end

  # result: {summary:, status:, log:}
  def summary_cell(result)
    result[:summary] || "no summary (exit #{result[:status]}), see #{result[:log]}"
  end

  # Failure lists of a suite that didn't finish are incomplete.
  def count(names, *results)
    (results.all? { _1[:summary] }) ? names.size : "?"
  end
end
