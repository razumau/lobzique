# frozen_string_literal: true

# Campfire's SSRF guard (surfguard) resolves hosts before WebMock sees the
# request, so its Opengraph tests need DNS. This answers from /etc/hosts and
# otherwise with one public address, so both runs behave the same without a
# network. Tests that stub Resolv.getaddresses with mocha still replace it.

require "resolv"

def Resolv.getaddresses(name)
  hosts = Resolv::Hosts.new.getaddresses(name)
  hosts.empty? ? ["93.184.215.14"] : hosts
end
