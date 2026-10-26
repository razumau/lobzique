# frozen_string_literal: true

# Picks Tier 2: gems beyond Rails/Loofah whose Nokogiri use the contract
# should cover. Writes inventory/raw/rank/ranking.json and prints a
# `tier2:` block for script/inventory/targets.yml.
#
#   ruby script/inventory/rank.rb [LIMIT]
#
# Inputs (inventory/raw/rank/): gems-meta/*.json (RubyGems metadata for every
# runtime dependent of nokogiri/loofah), reverse-names.txt,
# loofah-reverse-names.txt, apps.json (apps.rb), undeclared.txt ("name version"
# of corpus gems that reference Nokogiri without depending on it).
#
# Selection, in order, until LIMIT (default 50):
#   1. direct dependents of nokogiri/loofah in at least 2 corpus apps
#   2. corpus gems in at least 2 apps that use Nokogiri without declaring it
#   3. remaining runtime dependents released since RECENT, by total downloads

require "json"
require "yaml"
require "time"

module Rank
  RAW = File.expand_path("../../inventory/raw/rank", __dir__)
  TARGETS = File.expand_path("targets.yml", __dir__)
  RECENT = Time.utc(2023, 1, 1)
  # Rails components and the gems Tier 1 already covers.
  EXCLUDE = %w[nokogiri loofah rails-html-sanitizer rails-dom-testing actionpack actiontext actionview
    actionmailer activesupport activestorage railties rails].freeze

  def self.meta(name)
    path = File.join(RAW, "gems-meta", "#{name}.json")
    return nil unless File.exist?(path)
    JSON.parse(File.read(path))
  rescue JSON::ParserError
    nil
  end

  def self.run(limit)
    apps = JSON.parse(File.read(File.join(RAW, "apps.json")))
    dependents = apps.fetch("dependents")
    frequency = apps.fetch("gem_frequency")
    corpus_versions = Hash.new { |h, k| h[k] = Hash.new(0) }
    apps["apps"].each { |a| a["gems"].each { |g, v| corpus_versions[g][v["version"]] += 1 } }
    runtime_apps = Hash.new(0)
    apps["apps"].each { |a| a["gems"].each { |g, v| runtime_apps[g] += 1 if v["runtime"] } }
    reverse = File.readlines(File.join(RAW, "reverse-names.txt"), chomp: true)
    loofah_reverse = File.readlines(File.join(RAW, "loofah-reverse-names.txt"), chomp: true)
    undeclared_path = File.join(RAW, "undeclared.txt")
    undeclared = File.exist?(undeclared_path) ? File.readlines(undeclared_path, chomp: true).map { |l| l.split.first } : []

    rows = (reverse | loofah_reverse | dependents.keys | undeclared).filter_map do |name|
      next if EXCLUDE.include?(name)
      m = meta(name)
      released = m && m["version_created_at"] && Time.parse(m["version_created_at"])
      in_corpus = corpus_versions.key?(name)
      {
        name: name,
        version: in_corpus ? corpus_versions[name].max_by { |v, n| [n, Gem::Version.new(v)] }.first : m&.dig("version"),
        latest: m&.dig("version"),
        downloads: m&.dig("downloads") || 0,
        released: released&.strftime("%Y-%m-%d"),
        apps: frequency.fetch(name, 0),
        runtime_apps: runtime_apps.fetch(name, 0),
        declares: [("nokogiri" if reverse.include?(name) || dependents.dig(name, "via")&.include?("nokogiri")),
          ("loofah" if loofah_reverse.include?(name) || dependents.dig(name, "via")&.include?("loofah"))].compact,
        undeclared: undeclared.include?(name),
        source_code_uri: m&.dig("source_code_uri") || m&.dig("homepage_uri")
      }
    end

    picked = []
    reasons = {}
    pick = lambda do |row, why|
      next if picked.size >= limit || reasons.key?(row[:name])
      picked << row
      reasons[row[:name]] = why
    end
    rows.select { |r| r[:declares].any? && r[:apps] >= 2 }.sort_by { |r| [-r[:apps], -r[:downloads]] }
      .each { |r| pick.call(r, "depends on #{r[:declares].join("/")} in #{r[:apps]}/#{apps["apps"].size} apps (#{r[:runtime_apps]} at runtime)") }
    rows.select { |r| r[:undeclared] && r[:apps] >= 2 }.sort_by { |r| [-r[:apps], -r[:downloads]] }
      .each { |r| pick.call(r, "uses Nokogiri without declaring it; in #{r[:apps]}/#{apps["apps"].size} apps") }
    rows.select { |r| r[:declares].any? && r[:released] && Time.parse(r[:released]) >= RECENT }
      .sort_by { |r| -r[:downloads] }
      .each { |r| pick.call(r, "#{(r[:downloads] / 1_000_000.0).round}M downloads, released #{r[:released]}") }

    File.write(File.join(RAW, "ranking.json"), JSON.pretty_generate(
      rows: rows.sort_by { |r| -r[:downloads] }, picked: picked.map { |r| r.merge(why: reasons[r[:name]]) }
    ))
    puts "tier2:"
    picked.each do |r|
      puts "  #{r[:name]}:"
      puts "    version: #{r[:version]}"
      puts "    usage: #{(r[:apps] > 0 && r[:runtime_apps] == 0) ? "test" : "runtime"}"
      puts "    why: #{reasons[r[:name]]}"
    end
  end
end

Rank.run(Integer(ARGV.fetch(0, "50"))) if $PROGRAM_NAME == __FILE__
