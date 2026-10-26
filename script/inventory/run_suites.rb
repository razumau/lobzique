# frozen_string_literal: true

# Runs each target's `suite` from targets.yml under the recorder, in its git
# checkout (inventory/raw/src/NAME, or src/rails/PATH for Rails components).
# Checkouts must already be bundled by setup.rb with the same Ruby; suites use
# the Ruby running this script unless the target sets `ruby`. Logs go to
# inventory/raw/logs.
#
#   ruby script/inventory/run_suites.rb [NAME...]

require "rbconfig"
require "shellwords"
require "yaml"

root = File.expand_path("../..", __dir__)
src = File.join(root, "inventory/raw/src")
logs = File.join(root, "inventory/raw/logs")
Dir.mkdir(logs) unless Dir.exist?(logs)
targets = YAML.safe_load_file(File.join(__dir__, "targets.yml"))
only = ARGV

%w[tier1 tier2].flat_map { |t| targets.fetch(t).to_a }.each do |name, t|
  next unless t["suite"]
  next unless only.empty? || only.include?(name)
  # Keep checkouts' own .ruby-version files from switching Rubies under shims.
  env = {"PATH" => "#{RbConfig::CONFIG["bindir"]}:#{ENV["PATH"]}"}
  mise = ["mise", "exec", "ruby@#{t["ruby"]}", "--"] if t["ruby"]
  dir =
    if (path = t.dig("source", "path"))
      env["BUNDLE_GEMFILE"] = File.join(src, "rails/Gemfile")
      File.join(src, "rails", path)
    else
      File.join(src, name)
    end
  log = File.join(logs, "#{name}.log")
  print "#{name}: "
  ok = system(env, File.join(__dir__, "trace"), name, dir, *mise, *Shellwords.split(t["suite"]),
    out: log, err: [:child, :out])
  result = File.readlines(log).grep(/\A\d+ (runs|tests|examples)/).last&.strip
  puts "#{ok ? "ok" : "exit #{$?.exitstatus}"} #{result}"
end
