# Open: invalid bytes in HTML4 input

plan.md, phase 5, step 2. The HTML4 names parse with the HTML5 parser but
handle encodings as libxml2 does (decision 3). One case libxml2 handles in a
way the HTML5 parser can't take: bytes that are invalid in the input's
encoding.

## What libxml2 2.13.9 does (Nokogiri 1.19.7)

When no encoding is given, detected or declared, libxml2 reads the whole
input as ISO-8859-1 from the first non-ASCII byte, so no byte is invalid.
The cases below have an encoding:

| input | text node | attribute value | parse |
|---|---|---|---|
| UTF-8 given, `<p>caf\xE9 x</p><p>y</p>` | `"caf\xE9 x"`: the raw byte, so the Ruby string is invalid UTF-8 | `"café"`: the byte read as ISO-8859-1 | goes on; one error, `Invalid bytes in character encoding` |
| UTF-8 given, `<p>a\x80b</p>` | `"a\x80b"` | (same rule) | goes on; one error |
| Shift_JIS given, `<p>\x93\xFA \x81</p><p>after</p>` | `"日 "` | | stops at the invalid byte: the rest of the input, `after` included, is lost; one fatal error |

In the source, `htmlCurrentChar` returns an invalid UTF-8 byte's value as
the character, which is ISO-8859-1. Text keeps the byte raw because it is
copied with a length of 1. Encodings that go through iconv stop the parser
(`xmlHaltParser`) on a conversion error.

To the serializer, raw bytes in text go out as they came in, and the
document doesn't survive a round trip through valid UTF-8 tools.

On the Common Crawl sample (`script/differential --mode html4 --corpus
crawl`), 18 of 3107 pages have such bytes: windows-1251, Shift_JIS, EUC-KR
and others declared, with a stray byte that isn't, and one GBK page that
declares UTF-8. On the iconv ones libxml2 keeps only the text before the
byte: of the non-ASCII characters, 91 of 1692 on a windows-1251 page, 9 of
522 on an EUC-KR one, 7565 of 12706 on a Shift_JIS one.

## Options for lobzique

html5ever takes only valid UTF-8, so the raw byte can't be kept.

1. **ISO-8859-1 per invalid byte** (`\xE9` → `é`). What libxml2 does
   for attribute values, and what its code means. Decision 3's wording
   ("invalid bytes, which libxml2 reads as Latin-1") points here. In text it
   differs from Nokogiri anyway: Nokogiri writes back `\xE9`, lobzique would
   write `é` as UTF-8.
2. **U+FFFD per invalid sequence**: the HTML standard's decoder, and what
   lobzique's HTML5 names already do with invalid UTF-8, so both name sets
   would agree. Loses the byte's value.
3. **Stop at the invalid byte** for iconv encodings, as libxml2 does, and
   one of the above for UTF-8. Faithful for Shift_JIS and the like, but a
   page losing its tail to one bad byte is rarely what a caller wants.

Either way, the error goes in `document.errors`.

## Now

lobzique uses option 2 until this is decided. The choice is
`HTML4::EncodingReader::INVALID_BYTES` in
`lib/lobzique/html4/encoding_reader.rb`.
