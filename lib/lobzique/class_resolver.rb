# frozen_string_literal: true

# Adapted from Nokogiri 1.19.4, lib/nokogiri/class_resolver.rb (MIT, see LICENSE-NOKOGIRI.md).

module Lobzique
  # Finds the class with a given name in the namespace of the receiver's
  # class or of its nearest ancestor class, so a subclass such as
  # `Loofah::HTML5::Document` gets `Loofah::HTML5::DocumentFragment`.
  module ClassResolver
    VALID_NAMESPACES = Set.new(["HTML", "HTML4", "HTML5", "XML", "SAX"])

    def related_class(class_name)
      klass = nil
      inspecting = self.class

      while inspecting
        namespace_path = inspecting.name.split("::")[0..-2]
        inspecting = inspecting.superclass

        next unless VALID_NAMESPACES.include?(namespace_path.last)

        related_class_name = (namespace_path << class_name).join("::")
        klass = begin
          Object.const_get(related_class_name)
        rescue NameError
          nil
        end
        break if klass
      end
      klass
    end
  end
end
