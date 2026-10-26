# frozen_string_literal: true

# Checks out every target that has a `suite` into inventory/raw/src (Rails
# components share src/rails) and bundles it into inventory/raw/bundle with
# the Ruby running this script, or the target's `ruby`. Run it and
# run_suites.rb with the same Ruby. Checkouts that already exist are only
# bundled.
#
#   ruby script/inventory/setup.rb [NAME...]

require "fileutils"
require "yaml"

root = File.expand_path("../..", __dir__)
src = File.join(root, "inventory/raw/src")
FileUtils.mkdir_p(src)
targets = YAML.safe_load_file(File.join(__dir__, "targets.yml"))
only = ARGV

# Bundler groups skipped per checkout; Rails' db adapters and linters aren't
# needed by the suites in targets.yml.
WITHOUT = {"rails" => "db:rubocop:mdl:doc:job:cable"}

def run(*cmd, chdir:)
  system(*cmd, chdir: chdir, exception: true)
end

checkouts = %w[tier1 tier2].flat_map { |t| targets.fetch(t).to_a }.filter_map do |name, t|
  next unless t["suite"]
  next unless only.empty? || only.include?(name)
  source = t.fetch("source")
  [source["path"] ? "rails" : name, source, t["ruby"]]
end.uniq(&:first)

checkouts.each do |dir_name, source, ruby|
  dir = File.join(src, dir_name)
  unless Dir.exist?(dir)
    run("git", "init", "-q", dir, chdir: src)
    run("git", "fetch", "-q", "--depth", "1", "https://github.com/#{source["repo"]}.git", source["ref"], chdir: dir)
    run("git", "checkout", "-q", "FETCH_HEAD", chdir: dir)
  end
  run("bundle", "config", "set", "--local", "path", "../../bundle", chdir: dir)
  run("bundle", "config", "set", "--local", "without", WITHOUT[dir_name], chdir: dir) if WITHOUT[dir_name]
  puts "#{dir_name}: bundling with Ruby #{ruby || RUBY_VERSION}"
  run(*(["mise", "exec", "ruby@#{ruby}", "--"] if ruby), "bundle", "install", "--quiet", chdir: dir)
end
