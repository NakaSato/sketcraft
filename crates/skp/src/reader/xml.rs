//! The few `std::regex` searches `core.cpp` runs over `material.xml`, written out by hand with
//! the same leftmost-match results.

use super::is_space;

/// `\w` in the C locale.
fn is_word(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// `<(?:[A-Za-z_][\w.-]*:)?NAME\b([^>]*)>` (case-insensitive): the attribute text of the first
/// such tag. Without `closed` the final `>` is not required (the `image` pattern).
pub(super) fn find_tag<'a>(xml: &'a [u8], name: &[u8], closed: bool) -> Option<&'a [u8]> {
    let rest = |at: usize| -> Option<&'a [u8]> {
        let word = xml.get(at..at.checked_add(name.len())?)?;
        if !word.eq_ignore_ascii_case(name) {
            return None;
        }
        let start = at + name.len();
        if xml.get(start).is_some_and(|&c| is_word(c)) {
            return None;
        }
        let tail = xml.get(start..)?;
        match tail.iter().position(|&c| c == b'>') {
            Some(end) => tail.get(..end),
            None if closed => None,
            None => Some(tail),
        }
    };
    xml.iter().enumerate().filter(|(_, c)| **c == b'<').find_map(|(i, _)| {
        // The optional namespace prefix: its name cannot contain ':', so the run of name
        // characters must be followed by ':' directly.
        let prefixed = xml.get(i + 1).filter(|c| c.is_ascii_alphabetic() || **c == b'_').and_then(|_| {
            let run = xml.get(i + 2..)?.iter().take_while(|&&c| is_word(c) || c == b'.' || c == b'-').count();
            (xml.get(i + 2 + run) == Some(&b':')).then(|| rest(i + 3 + run)).flatten()
        });
        prefixed.or_else(|| rest(i + 1))
    })
}

/// `attr`: `(?:^|\s)KEY\s*=\s*["']([^"']*)["']` (case-insensitive), entity-decoded; empty
/// when absent.
pub(super) fn attr(s: &[u8], key: &[u8]) -> Vec<u8> {
    for k in 0..s.len() {
        if k > 0 && !s.get(k - 1).is_some_and(|&c| is_space(c)) {
            continue;
        }
        let Some(word) = s.get(k..k + key.len()) else {
            break;
        };
        if !word.eq_ignore_ascii_case(key) {
            continue;
        }
        let mut p = k + key.len();
        while s.get(p).is_some_and(|&c| is_space(c)) {
            p += 1;
        }
        if s.get(p) != Some(&b'=') {
            continue;
        }
        p += 1;
        while s.get(p).is_some_and(|&c| is_space(c)) {
            p += 1;
        }
        if !matches!(s.get(p), Some(b'"' | b'\'')) {
            continue;
        }
        let Some(value) = s.get(p + 1..) else {
            continue;
        };
        if let Some(end) = value.iter().position(|&c| c == b'"' || c == b'\'') {
            return decode_entities(value.get(..end).unwrap_or(&[]));
        }
    }
    Vec::new()
}

/// `decode_xml_entities`: the five named entities and numeric references, in one pass.
pub(super) fn decode_entities(value: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(value.len());
    let mut i = 0;
    while let Some(&c) = value.get(i) {
        if c == b'&'
            && let Some((len, decoded)) = entity(value.get(i + 1..).unwrap_or(&[]))
        {
            match decoded {
                Some(bytes) => out.extend_from_slice(&bytes),
                // A reference too large for `stoul` stays as written.
                None => out.extend_from_slice(value.get(i..i + 1 + len).unwrap_or(&[])),
            }
            i += 1 + len;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// One entity after `&`: its length including `;` and its bytes (`None` if `stoul` throws).
fn entity(s: &[u8]) -> Option<(usize, Option<Vec<u8>>)> {
    for (name, c) in [(&b"lt"[..], b'<'), (b"gt", b'>'), (b"amp", b'&'), (b"apos", b'\''), (b"quot", b'"')] {
        if s.starts_with(name) && s.get(name.len()) == Some(&b';') {
            return Some((name.len() + 1, Some(vec![c])));
        }
    }
    if s.first() != Some(&b'#') {
        return None;
    }
    let decimal = s.get(1..)?.iter().take_while(|c| c.is_ascii_digit()).count();
    if decimal > 0 && s.get(1 + decimal) == Some(&b';') {
        let digits = s.get(1..1 + decimal)?;
        return Some((decimal + 2, parse_radix(digits, 10).map(utf8)));
    }
    if matches!(s.get(1), Some(b'x' | b'X')) {
        let hex = s.get(2..)?.iter().take_while(|c| c.is_ascii_hexdigit()).count();
        if hex > 0 && s.get(2 + hex) == Some(&b';') {
            let digits = s.get(2..2 + hex)?;
            return Some((hex + 3, parse_radix(digits, 16).map(utf8)));
        }
    }
    None
}

/// `stoul` of plain digits; `None` on overflow of `unsigned long` (64 bits).
fn parse_radix(digits: &[u8], radix: u32) -> Option<u64> {
    digits.iter().try_fold(0u64, |v, &c| v.checked_mul(u64::from(radix))?.checked_add(u64::from(char::from(c).to_digit(radix)?)))
}

/// The UTF-8 encoder in `decode_xml_entities`, including its `char` truncation past U+10FFFF.
fn utf8(cp: u64) -> Vec<u8> {
    let byte = |v: u64| v as u8;
    if cp <= 0x7f {
        vec![byte(cp)]
    } else if cp <= 0x7ff {
        vec![byte(0xc0 | (cp >> 6)), byte(0x80 | (cp & 0x3f))]
    } else if cp <= 0xffff {
        vec![byte(0xe0 | (cp >> 12)), byte(0x80 | ((cp >> 6) & 0x3f)), byte(0x80 | (cp & 0x3f))]
    } else {
        vec![byte(0xf0 | (cp >> 18)), byte(0x80 | ((cp >> 12) & 0x3f)), byte(0x80 | ((cp >> 6) & 0x3f)), byte(0x80 | (cp & 0x3f))]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_and_attributes_match_the_regexes() {
        let xml =
            br#"<?xml?><mat:materials/><materialx/><material name="A &amp;lt; B" colorRed = '12' colorGreen="x"><texture textureFilename="t.png">"#;
        let a = find_tag(xml, b"material", true).unwrap();
        assert_eq!(attr(a, b"name"), b"A &lt; B");
        assert_eq!(attr(a, b"COLORRED"), b"12");
        assert_eq!(attr(a, b"red"), b"");
        assert_eq!(find_tag(b"<x:material:y>", b"material", true), Some(&b":y"[..]));
        assert_eq!(find_tag(b"<x:materials>", b"material", true), None);
        assert_eq!(find_tag(b"<material:y>", b"material", true), Some(&b":y"[..]));
        assert_eq!(find_tag(b"<image path='a'", b"image", false), Some(&b" path='a'"[..]));
        assert_eq!(decode_entities(b"&#65;&#x42;&#99999999999999999999;&bogus;&"), b"AB&#99999999999999999999;&bogus;&");
    }
}
