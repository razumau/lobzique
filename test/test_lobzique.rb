# frozen_string_literal: true

require "test_helper"

class TestLobzique < Minitest::Test
  def test_that_it_has_a_version_number
    refute_nil ::Lobzique::VERSION
  end

  def test_hello_world
    assert_equal "Hello earth, from Rust!", Lobzique.hello("world")
  end
end
