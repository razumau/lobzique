# frozen_string_literal: true

# The nokogiri facade (plan.md decision 1): lobzique under Nokogiri's name.
# Never pushed to RubyGems; apps take it from this repository with
# `gem "nokogiri", git:` or `path:`, and Bundler finds lobzique there too.
# Versions are read, not required: Bundler evaluates this in any bundle of the
# repository, the development one with real Nokogiri included.

version = ->(file) { File.read(File.expand_path(file, __dir__))[/VERSION = "(.+?)"/, 1] }

Gem::Specification.new do |spec|
  spec.name = "nokogiri"
  spec.version = version["lib/nokogiri/version/constant.rb"]
  spec.authors = ["Jury Razumau"]
  spec.email = ["mail@razumau.net"]

  spec.summary = "lobzique as Nokogiri, for apps that need Nokogiri only for HTML"
  spec.homepage = "https://github.com/razumau/lobzique"
  spec.license = "MIT"
  spec.required_ruby_version = ">= 3.3"
  spec.metadata["source_code_uri"] = "https://github.com/razumau/lobzique"
  spec.metadata["rubygems_mfa_required"] = "true"

  spec.files = Dir.glob(%w[lib/**/*.rb README.md], base: __dir__)
  spec.require_paths = ["lib"]
  spec.add_dependency "lobzique", version["../lib/lobzique/version.rb"]
end
