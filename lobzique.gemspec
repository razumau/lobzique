# frozen_string_literal: true

require_relative "lib/lobzique/version"

Gem::Specification.new do |spec|
  spec.name = "lobzique"
  spec.version = Lobzique::VERSION
  spec.authors = ["Jury Razumau"]
  spec.email = ["mail@razumau.net"]

  spec.summary = "Rust-based Nokogiri replacement"
  spec.homepage = "https://github.com/razumau/lobzique"
  spec.license = "MIT"
  spec.required_ruby_version = ">= 3.3"
  spec.metadata["allowed_push_host"] = "https://rubygems.org"
  spec.metadata["homepage_uri"] = spec.homepage
  spec.metadata["source_code_uri"] = "https://github.com/razumau/lobzique"
  spec.metadata["changelog_uri"] = "https://github.com/razumau/lobzique/blob/main/CHANGELOG.md"
  spec.metadata["rubygems_mfa_required"] = "true"

  spec.files = Dir.glob(%w[
    lib/**/*.rb
    ext/**/*.{rb,rs,toml}
    crates/**/*.{rs,toml}
    Cargo.toml Cargo.lock
    README.md CHANGELOG.md LICENSE.txt
  ], base: __dir__)
  spec.executables = spec.files.grep(%r{\Aexe/}) { |f| File.basename(f) }
  spec.require_paths = ["lib"]
  spec.extensions = ["ext/lobzique/extconf.rb"]
  spec.add_dependency "rb_sys", "~> 0.9.128"
end
