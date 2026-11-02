# frozen_string_literal: true

# Adapted from Loofah 2.25.3, lib/loofah/concerns.rb (MIT).

module Lobzique
  module Sanitizer
    module ScrubBehavior
      module Node # :nodoc:
        def scrub!(scrubber)
          scrubber = ScrubBehavior.resolve_scrubber(scrubber)
          # lobzique: Loofah's case names Nokogiri's classes, so lobzique
          # documents and fragments would fall through to the else branch
          case self
          when XML::Document
            scrubber.traverse(root) if root
          when XML::DocumentFragment
            # lobzique: traversed here, not by children.scrub!, so the
            # fragment is escaped once
            children.each { |node| scrubber.traverse(node) }
          else
            scrubber.traverse(self)
          end
          # lobzique: escape_raw_text_in_select.rb
          EscapeRawTextInSelect.call(self)
          self
        end
      end

      module NodeSet # :nodoc:
        def scrub!(scrubber)
          each { |node| node.scrub!(scrubber) }
          self
        end
      end

      class << self
        def resolve_scrubber(scrubber) # :nodoc:
          # lobzique: the Loofah scrubbers that build Nokogiri nodes, by name
          # or by instance, resolve to ports (scrubbers.rb)
          klass = Scrubbers::MAP[scrubber] || Loofah::Scrubbers::MAP[scrubber] || Scrubbers::PORTS[scrubber.class]
          scrubber = klass.new if klass
          unless scrubber.is_a?(Loofah::Scrubber)
            raise Loofah::ScrubberNotFound, "not a Scrubber or a scrubber name: #{scrubber.inspect}"
          end

          scrubber
        end
      end
    end

    # lobzique: TextBehavior (text, to_text) isn't ported yet; Rails'
    # sanitizers don't call it.

    module DocumentDecorator # :nodoc:
      def initialize(*args, &block)
        super
        decorators(XML::Node) << ScrubBehavior::Node
        decorators(XML::NodeSet) << ScrubBehavior::NodeSet
      end
    end

    module HtmlDocumentBehavior # :nodoc:
      # lobzique: ClassMethods#parse, which removes comments before the html
      # element, isn't ported yet; Rails' sanitizers parse only fragments.

      def serialize_root
        at_xpath("/html/body")
      end
    end

    module HtmlFragmentBehavior # :nodoc:
      module ClassMethods
        def parse(tags, encoding = nil)
          doc = document_klass.new

          encoding ||= tags.respond_to?(:encoding) ? tags.encoding.name : "UTF-8"
          doc.encoding = encoding

          new(doc, tags)
        end

        def document_klass
          # lobzique: one class, HTML5 only
          @document_klass ||= if self == Sanitizer::DocumentFragment
            Sanitizer::Document
          else
            raise ArgumentError, "unexpected class: #{self}"
          end
        end
      end

      class << self
        def included(base)
          base.extend(ClassMethods)
        end
      end

      def to_s
        serialize_root.children.to_s
      end

      alias_method :serialize, :to_s

      def serialize_root
        at_xpath("./body") || self
      end
    end
  end
end
