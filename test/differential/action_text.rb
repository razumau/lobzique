# frozen_string_literal: true

require_relative "harness"
require_relative "../action_text/action_text_helper"

module Differential
  # The action_text mode (plan.md phase 4): each case is stored rich text,
  # read with Action Text's document classes set to Nokogiri's and then to
  # lobzique's (lobzique/action_text). Compared: what Action Text stores
  # (to_html), shows the editor (to_trix_html, and what saving that back
  # stores), and indexes or emails (to_plain_text), and the attachments and
  # links it finds.
  class ActionTextHarness < Harness
    MODE = "action_text"

    def observe(root, kase)
      ActionTextHelper.use(root)
      content = ActionText::Content.new(kase.input)
      trix = content.to_trix_html
      {
        html: content.to_html.b,
        trix: trix.b,
        trix_saved: ActionText::Content.new(trix).to_html.b,
        plain_text: content.to_plain_text.b,
        attachments: content.attachments.map { _1.to_html.b },
        galleries: content.attachment_galleries.size,
        links: content.links
      }
    rescue => e
      {error: "#{e.class.name.sub(ROOT_NAME, "")}: #{e.message}"}
    end

    def explain(result)
      kase, expected, actual = result.kase, result.expected, result.actual
      out = ["FAIL #{kase.id}"]
      out << "  #{kase.note}" if kase.note
      out << "  input: #{truncate(kase.input.b.inspect, 200)}"
      (expected.keys | actual.keys).each do |part|
        a, b = expected[part], actual[part]
        next if a == b

        out.concat((a.is_a?(String) && b.is_a?(String)) ? byte_difference(part, a, b) :
          ["  #{part}:", "    nokogiri #{truncate(a.inspect, 300)}", "    lobzique #{truncate(b.inspect, 300)}"])
      end
      out
    end
  end
end
