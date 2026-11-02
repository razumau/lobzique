# frozen_string_literal: true

require_relative "sanitizer_helper"

# The vendor as Rails reaches it. Adapted from the vendor cases of
# test/fixtures/rails-html-sanitizer/rails_api_test.rb, and from
# SanitizeHelperVendorTests in
# test/fixtures/rails/actionview/test/template/sanitize_helper_test.rb.
module VendorContract
  def test_vendor_classes
    assert_equal sanitizers::FullSanitizer, vendor.full_sanitizer
    assert_equal sanitizers::LinkSanitizer, vendor.link_sanitizer
    assert_equal sanitizers::SafeListSanitizer, vendor.safe_list_sanitizer
    assert_equal sanitizers::SafeListSanitizer, vendor.white_list_sanitizer
  end

  def test_sanitizers_are_rails_sanitizers
    [vendor.full_sanitizer, vendor.link_sanitizer, vendor.safe_list_sanitizer].each do |klass|
      assert_operator klass, :<, Rails::HTML::Sanitizer
    end
  end

  def test_allowed_uri
    assert vendor.safe_list_sanitizer.allowed_uri?("https://example.com")
    assert vendor.safe_list_sanitizer.allowed_uri?("http://example.com")
    assert vendor.safe_list_sanitizer.allowed_uri?("mailto:user@example.com")
    assert vendor.safe_list_sanitizer.allowed_uri?("/relative/path")
    refute vendor.safe_list_sanitizer.allowed_uri?("javascript:alert(1)")
  end

  def test_safe_list_sanitizer_takes_prune
    input = "<div><script>x</script><blink>y</blink></div>"
    assert_equal "<div>xy</div>", vendor.safe_list_sanitizer.new.sanitize(input)
    assert_equal "<div></div>", vendor.safe_list_sanitizer.new(prune: true).sanitize(input)
  end

  def test_nil_and_empty_input
    assert_nil vendor.full_sanitizer.new.sanitize(nil)
    assert_equal "", vendor.safe_list_sanitizer.new.sanitize("")
  end

  def test_output_is_utf8
    assert_equal Encoding::UTF_8, vendor.safe_list_sanitizer.new.sanitize("<p>Señor</p>").encoding
    assert_equal "<p>Señor</p>", vendor.safe_list_sanitizer.new.sanitize("<p>Se&ntilde;or</p>")
  end
end

module SanitizeHelperVendorContract
  def setup
    super
    @saved_vendor = ActionView::Helpers::SanitizeHelper.sanitizer_vendor
    ActionView::Helpers::SanitizeHelper.sanitizer_vendor = vendor

    @subject_class = Class.new do
      include ActionView::Helpers::SanitizeHelper
    end
    @subject = @subject_class.new
  end

  def teardown
    ActionView::Helpers::SanitizeHelper.sanitizer_vendor = @saved_vendor
    super
  end

  def test_full_sanitizer_returns_a_sanitizer
    sanitizer = @subject_class.full_sanitizer

    assert_equal(vendor.full_sanitizer, sanitizer.class)
    assert_respond_to(sanitizer, :sanitize)
  end

  def test_link_sanitizer_returns_a_sanitizer
    sanitizer = @subject_class.link_sanitizer

    assert_equal(vendor.link_sanitizer, sanitizer.class)
    assert_respond_to(sanitizer, :sanitize)
  end

  def test_safe_list_sanitizer_returns_a_sanitizer
    sanitizer = @subject_class.safe_list_sanitizer

    assert_equal(vendor.safe_list_sanitizer, sanitizer.class)
    assert_respond_to(sanitizer, :sanitize)
  end

  def test_full_sanitizer_is_settable
    @subject_class.full_sanitizer = :mock
    assert_same(:mock, @subject_class.full_sanitizer)
  end

  def test_link_sanitizer_is_settable
    @subject_class.link_sanitizer = :mock
    assert_same(:mock, @subject_class.link_sanitizer)
  end

  def test_safe_list_sanitizer_is_settable
    @subject_class.safe_list_sanitizer = :mock
    assert_same(:mock, @subject_class.safe_list_sanitizer)
  end

  def test_deprecated_methods_are_supported
    assert_respond_to(vendor, :white_list_sanitizer)
  end

  def test_sanitized_allowed_tags_returns_a_string_collection
    result = @subject_class.sanitized_allowed_tags

    assert_kind_of(Enumerable, result)
    assert_kind_of(String, result.first)
  end

  def test_sanitized_allowed_attributes_returns_a_string_collection
    result = @subject_class.sanitized_allowed_attributes

    assert_kind_of(Enumerable, result)
    assert_kind_of(String, result.first)
  end

  def test_sanitized_allowed_tags_is_settable
    saved_value = @subject_class.sanitized_allowed_tags
    @subject_class.sanitized_allowed_tags = ["a", "b"]

    assert_equal(["a", "b"], @subject_class.sanitized_allowed_tags)
  ensure
    @subject_class.sanitized_allowed_tags = saved_value
  end

  def test_sanitized_allowed_attributes_is_settable
    saved_value = @subject_class.sanitized_allowed_attributes
    @subject_class.sanitized_allowed_attributes = ["a", "b"]

    assert_equal(["a", "b"], @subject_class.sanitized_allowed_attributes)
  ensure
    @subject_class.sanitized_allowed_attributes = saved_value
  end

  def test_sanitize
    input = %(<div><script>alert(1);</script><a href="http://www.example.com/">Example</a> of a fragment</div>)
    assert_not_includes(@subject.sanitize(input), "script")
  end

  def test_sanitize_with_tags_option
    input = %(<b>bold</b> <i>italic</i> <div>hello</div>)

    assert_equal("<b>bold</b> <i>italic</i> hello", @subject.sanitize(input, tags: %w[b i]))
    assert_equal("bold italic <div>hello</div>", @subject.sanitize(input, tags: %w[div]))
  end

  def test_sanitize_with_attributes_option
    input = %(<div a="1" b="2" c="3">hello</div>)

    assert_equal(%(<div a="1" b="2">hello</div>), @subject.sanitize(input, attributes: %w[a b]))
    assert_equal(%(<div b="2" c="3">hello</div>), @subject.sanitize(input, attributes: %w[b c]))
  end

  def test_sanitize_with_loofah_scrubber_option
    input = %(<div>hello</div>)
    scrubber = Loofah::Scrubber.new do |node|
      node.content = "scrubbed"
    end

    assert_equal(%(<div>scrubbed</div>), @subject.sanitize(input, scrubber: scrubber))
  end

  def test_sanitize_with_loofah_scrubber_instances_that_build_text_nodes
    assert_equal("<div>a</div>&lt;foo&gt;b&lt;/foo&gt;",
      @subject.sanitize("<div>a</div><foo>b</foo>", scrubber: Loofah::Scrubbers::Escape.new))
    assert_equal("\na\n\nb\nc\n",
      @subject.sanitize("<p>a</p><div>b<br>c</div>", scrubber: Loofah::Scrubbers::NewlineBlockElements.new))
  end

  def test_sanitize_with_custom_scrubber_option
    scrubber = Class.new(Rails::HTML::PermitScrubber) do
      def initialize
        super
        self.tags = ["div"]
      end
    end.new

    input = %(<div>hello</div><p>world</p>)

    assert_equal(%(<div>hello</div>world), @subject.sanitize(input, scrubber: scrubber))
  end

  def test_sanitize_css
    input = %(width: 100%; background-image:url("http://www.example.com/"); height: 100%;)
    result = @subject.sanitize_css(input)

    assert_not_includes(result, "background-image")
    assert_includes(result, "width")
    assert_includes(result, "height")
  end

  def test_strip_tags
    input = %(<div><a href="http://www.example.com/">Example</a> of a fragment</div>)
    assert_equal("Example of a fragment", @subject.strip_tags(input))
  end

  def test_strip_links
    input = %(<div><a href="http://www.example.com/">Example</a> of a fragment</div>)
    assert_equal("<div>Example of a fragment</div>", @subject.strip_links(input))
  end

  # Upstream expects libxml2's tree for the HTML4 vendors and gumbo's for
  # Rails::HTML5; both vendors here are HTML5.
  def test_we_get_the_expected_HTML_parser
    input = %(<p>1<b>2<i>3</b>4</i>5</p>)
    scrubber = Loofah::Scrubber.new { |_| }

    assert_equal("<p>1<b>2<i>3</i></b><i>4</i>5</p>", @subject.sanitize(input, scrubber: scrubber))
  end
end

class TestVendorNokogiri < ActiveSupport::TestCase
  include UpstreamSuite::NoGCStress
  include VendorContract
  include SanitizeHelperVendorContract

  def vendor = Rails::HTML5::Sanitizer

  def sanitizers = Rails::HTML5
end

class TestVendorLobzique < ActiveSupport::TestCase
  include VendorContract
  include SanitizeHelperVendorContract

  def vendor = Lobzique::Sanitizer

  def sanitizers = Lobzique::Sanitizer
end
