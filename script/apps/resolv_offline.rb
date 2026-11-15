# frozen_string_literal: true

# The SSRF guard of Campfire (surfguard) resolves hosts before WebMock sees
# the request, so its Opengraph tests need DNS. This file answers
# from /etc/hosts, and otherwise with one public address. Both runs then behave
# the same without a network. Tests that stub Resolv.getaddresses with mocha
# still replace it.

require "resolv"

def Resolv.getaddresses(name)
  hosts = Resolv::Hosts.new.getaddresses(name)
  hosts.empty? ? ["93.184.215.14"] : hosts
end
