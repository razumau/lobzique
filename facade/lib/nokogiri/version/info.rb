# frozen_string_literal: true

# Adapted from Nokogiri 1.19.7, lib/nokogiri/version/info.rb (MIT, see LICENSE-NOKOGIRI.md).

require "singleton"

module Nokogiri
  class VersionInfo # :nodoc:
    include Singleton

    # lobzique: JRuby branches dropped (ARCHITECTURE.md, port rule)
    def jruby?
      nil
    end

    def windows?
      ::RUBY_PLATFORM =~ /mingw|mswin/
    end

    def ruby_minor
      Gem::Version.new(::RUBY_VERSION).segments[0..1].join(".")
    end

    def engine
      defined?(::RUBY_ENGINE) ? ::RUBY_ENGINE : "mri"
    end

    def loaded_libxml_version
      Gem::Version.new(Nokogiri::LIBXML_LOADED_VERSION
        .scan(/^(\d+)(\d\d)(\d\d)(?!\d)/).first
        .collect(&:to_i)
        .join("."))
    end

    def compiled_libxml_version
      Gem::Version.new(Nokogiri::LIBXML_COMPILED_VERSION)
    end

    def libxml2?
      defined?(Nokogiri::LIBXML_COMPILED_VERSION)
    end

    def libxml2_has_iconv?
      defined?(Nokogiri::LIBXML_ICONV_ENABLED) && Nokogiri::LIBXML_ICONV_ENABLED
    end

    def libxml2_has_zlib?
      defined?(Nokogiri::LIBXML_ZLIB_ENABLED) && Nokogiri::LIBXML_ZLIB_ENABLED
    end

    def libxml2_has_http?
      defined?(Nokogiri::LIBXML_HTTP_ENABLED) && Nokogiri::LIBXML_HTTP_ENABLED
    end

    # lobzique: no libxslt (XSLT is out of scope), so none of its versions,
    # loaded_libxslt_version, compiled_libxslt_version or libxslt_has_datetime?

    def libxml2_using_packaged?
      libxml2? && Nokogiri::PACKAGED_LIBRARIES
    end

    def libxml2_using_system?
      libxml2? && !libxml2_using_packaged?
    end

    def libxml2_precompiled?
      libxml2_using_packaged? && Nokogiri::PRECOMPILED_LIBRARIES
    end

    def warnings
      warnings = []

      if libxml2?
        if compiled_libxml_version != loaded_libxml_version
          warnings << "Nokogiri was built against libxml version #{compiled_libxml_version}, but has dynamically loaded #{loaded_libxml_version}"
        end
      end

      warnings
    end

    # lobzique: no cppflags or ldflags, as there are no headers to build
    # against, and no libxslt section
    def to_hash
      {}.tap do |vi|
        vi["warnings"] = []
        vi["nokogiri"] = {}.tap do |nokogiri|
          nokogiri["version"] = Nokogiri::VERSION
        end
        vi["ruby"] = {}.tap do |ruby|
          ruby["version"] = ::RUBY_VERSION
          ruby["platform"] = ::RUBY_PLATFORM
          ruby["gem_platform"] = ::Gem::Platform.local.to_s
          ruby["description"] = ::RUBY_DESCRIPTION
          ruby["engine"] = engine
        end

        if libxml2?
          vi["libxml"] = {}.tap do |libxml|
            if libxml2_using_packaged?
              libxml["source"] = "packaged"
              libxml["precompiled"] = libxml2_precompiled?
              libxml["patches"] = Nokogiri::LIBXML2_PATCHES
            else
              libxml["source"] = "system"
            end
            libxml["memory_management"] = Nokogiri::LIBXML_MEMORY_MANAGEMENT
            libxml["iconv_enabled"] = libxml2_has_iconv?
            libxml["zlib_enabled"] = libxml2_has_zlib?
            libxml["http_enabled"] = libxml2_has_http?
            libxml["compiled"] = compiled_libxml_version.to_s
            libxml["loaded"] = loaded_libxml_version.to_s
          end

          vi["warnings"] = warnings
        end

        # lobzique: in place of other_libraries
        if (lobzique = Gem.loaded_specs["lobzique"])
          vi["lobzique"] = {"version" => lobzique.version.to_s}
        end
      end
    end

    def to_markdown
      require "yaml"
      "# Nokogiri (#{Nokogiri::VERSION})\n" +
        YAML.dump(to_hash).each_line.map { |line| "    #{line}" }.join
    end

    instance.warnings.each do |warning|
      warn "WARNING: #{warning}"
    end
  end

  # :nodoc:
  def self.uses_libxml?(requirement = nil)
    return false unless VersionInfo.instance.libxml2?
    return true unless requirement

    Gem::Requirement.new(requirement).satisfied_by?(VersionInfo.instance.loaded_libxml_version)
  end

  # :nodoc:
  def self.uses_gumbo?
    uses_libxml? # TODO: replace with Gumbo functionality
  end

  # :nodoc:
  def self.jruby?
    VersionInfo.instance.jruby?
  end

  # :nodoc:
  def self.libxml2_patches
    if VersionInfo.instance.libxml2_using_packaged?
      Nokogiri::VERSION_INFO["libxml"]["patches"]
    else
      []
    end
  end

  require_relative "../extension"

  # Detailed version info about Nokogiri and the installed extension dependencies.
  VERSION_INFO = VersionInfo.instance.to_hash
end
