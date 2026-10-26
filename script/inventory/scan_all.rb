# frozen_string_literal: true

# Runs scan.rb over every target in targets.yml. Gems with a git `source` are
# scanned from inventory/raw/src/NAME (plus `path`); the rest from the
# unpacked package in inventory/raw/unpacked/NAME-VERSION.
#
#   ruby script/inventory/scan_all.rb

require "yaml"

root = File.expand_path("../..", __dir__)
raw = File.join(root, "inventory/raw")
targets = YAML.safe_load_file(File.join(__dir__, "targets.yml"))
Dir.mkdir(File.join(raw, "scans")) unless Dir.exist?(File.join(raw, "scans"))

%w[tier1 tier2].each do |tier|
  targets.fetch(tier).each do |name, t|
    source = t["source"]
    dir =
      if source && tier == "tier1"
        File.join(raw, "src", source["path"] ? "rails/#{source["path"]}" : name)
      else
        File.join(raw, "unpacked", "#{name}-#{t["version"]}")
      end
    abort "missing sources for #{name}: #{dir}" unless Dir.exist?(dir)
    system(RbConfig.ruby, File.join(__dir__, "scan.rb"), File.join(raw, "nokogiri-api.json"),
      name, t["version"].to_s, dir, File.join(raw, "scans", "#{name}.json"), exception: true)
  end
end
