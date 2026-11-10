# frozen_string_literal: true

# lobzique's own: Nokogiri's entry points to what lobzique leaves out
# (plan.md, "Out"). Absent, they fail with NoMethodError or an uninitialized
# constant, and on the facade nothing before that says Nokogiri isn't there:
# Bundler, VERSION and uses_libxml? all answer as Nokogiri. These raise
# NotImplementedError, and the constants NameError, saying so.

module Nokogiri
  module Facade
    module Unsupported
      def self.message(name, feature)
        "#{name} isn't supported: the nokogiri facade is lobzique, which has #{feature}. " \
          "For it, use the nokogiri gem with lobzique's seams (https://github.com/razumau/lobzique#readme)"
      end

      def self.stub_methods(feature, mod, *names)
        names.each do |name|
          # self is the receiver, so a subclass names itself
          mod.define_singleton_method(name) do |*|
            raise NotImplementedError, Unsupported.message("#{self}.#{name}", feature)
          end
        end
      end

      # const_missing rather than placeholder classes: defined? stays false
      # and NameError stays the error.
      def self.stub_constants(feature, mod, *names)
        hook = Module.new do
          define_method(:const_missing) do |name|
            return super(name) unless names.include?(name)

            raise NameError.new(Unsupported.message("#{self}::#{name}", feature), name, receiver: self)
          end
        end
        mod.singleton_class.prepend(hook)
      end

      xml = "no XML parser"
      stub_methods(xml, Nokogiri, :XML)
      stub_methods(xml, XML, :parse, :fragment, :Reader, :Schema, :RelaxNG)
      stub_methods(xml, XML::Document, :parse, :read_io, :read_memory)
      stub_methods(xml, XML::DocumentFragment, :parse)
      stub_constants(xml, XML,
        :Reader, :Schema, :RelaxNG, :ProcessingInstruction, :EntityReference,
        :AttributeDecl, :ElementDecl, :EntityDecl, :ElementContent, :Notation)

      builder = "no Builder"
      stub_constants(builder, XML, :Builder)
      stub_constants(builder, HTML4, :Builder)
      stub_constants(builder, HTML5, :Builder)

      sax = "no SAX parser"
      stub_constants(sax, XML, :SAX)
      stub_constants(sax, HTML4, :SAX)

      xslt = "no XSLT"
      stub_methods(xslt, Nokogiri, :XSLT)
      stub_constants(xslt, Nokogiri, :XSLT)

      css_to_xpath = "no CSS-to-XPath translation (it compiles CSS natively)"
      stub_methods(css_to_xpath, CSS, :xpath_for, :parse)
      stub_constants(css_to_xpath, CSS, :Parser, :Tokenizer, :Node, :XPathVisitor, :SelectorCache)

      slop = "no Slop decorator"
      stub_methods(slop, Nokogiri, :Slop)
      stub_constants(slop, Nokogiri, :Decorators)

      html4_tables = "no libxml2 HTML4 element and entity tables"
      stub_constants(html4_tables, HTML4, :ElementDescription, :EntityDescription, :EntityLookup, :NamedCharacters)
    end
  end
end
