use crate::model::Hit;
use serde::de::{self, Deserializer};

pub fn deserialize_hit<'de, D>(d: D) -> std::result::Result<Hit, D::Error>
where
    D: Deserializer<'de>,
{
    struct HitVisitor;

    impl<'de> de::Visitor<'de> for HitVisitor {
        type Value = Hit;

        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("\"nonempty\", \"status_ok\", or { greater = N }")
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<Hit, E> {
            match v {
                "nonempty" => Ok(Hit::Nonempty),
                "status_ok" => Ok(Hit::StatusOk),
                other => Err(E::custom(format!("unknown hit rule `{other}`"))),
            }
        }

        fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<Hit, A::Error> {
            let key: String = match map.next_key()? {
                Some(k) => k,
                None => return Err(de::Error::custom("empty hit table")),
            };
            if key != "greater" {
                return Err(de::Error::custom(format!("unknown hit rule `{key}`")));
            }
            let n: i64 = map.next_value()?;
            Ok(Hit::Greater(n))
        }
    }

    d.deserialize_any(HitVisitor)
}

pub fn truncate(text: &str, max: usize) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if out.chars().count() >= max {
            out.push('…');
            return out;
        }
        out.push(c);
    }
    out
}

/// Percent-encode one URL path/query component, keeping the RFC 3986
/// unreserved characters (`A-Z a-z 0-9 - _ . ~`).
pub fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Encode a slash-separated identifier as a URL path, preserving the `/`
/// separators so scoped npm packages still resolve.
pub fn percent_encode_path(s: &str) -> String {
    s.split('/')
        .map(percent_encode)
        .collect::<Vec<_>>()
        .join("/")
}
