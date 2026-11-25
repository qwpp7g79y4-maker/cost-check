//! JSON that refuses a repeated key.
//!
//! `serde_json::Value` keeps the last of two equal keys and says nothing. An
//! export with `"total_net"` twice has no single meaning, so it is refused
//! instead of read as whichever value came last.

use std::fmt;

use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};

use crate::Fault;

/// Parse `bytes` as JSON. A syntax error gives `unreadable`; a repeated key
/// in one object gives its own message. The message does not echo the key.
pub fn parse(bytes: &[u8], unreadable: &str) -> Result<Value, Fault> {
    match serde_json::from_slice::<Strict>(bytes) {
        Ok(Strict(value)) => Ok(value),
        Err(err) if err.to_string().starts_with(DUPLICATE) => Err(Fault::new(
            "de JSON heeft een dubbele sleutel in één object",
        )),
        Err(_) => Err(Fault::new(unreadable)),
    }
}

const DUPLICATE: &str = "duplicate key";

struct Strict(Value);

impl<'de> Deserialize<'de> for Strict {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(StrictVisitor)
    }
}

struct StrictVisitor;

impl<'de> Visitor<'de> for StrictVisitor {
    type Value = Strict;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("any JSON value")
    }

    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Strict, E> {
        Ok(Strict(Value::Bool(v)))
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Strict, E> {
        Ok(Strict(Value::Number(v.into())))
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Strict, E> {
        Ok(Strict(Value::Number(v.into())))
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Strict, E> {
        Number::from_f64(v)
            .map(|number| Strict(Value::Number(number)))
            .ok_or_else(|| E::custom("not a finite number"))
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Strict, E> {
        Ok(Strict(Value::String(v.to_string())))
    }

    fn visit_string<E: de::Error>(self, v: String) -> Result<Strict, E> {
        Ok(Strict(Value::String(v)))
    }

    fn visit_none<E: de::Error>(self) -> Result<Strict, E> {
        Ok(Strict(Value::Null))
    }

    fn visit_unit<E: de::Error>(self) -> Result<Strict, E> {
        Ok(Strict(Value::Null))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Strict, A::Error> {
        let mut items = Vec::new();
        while let Some(Strict(item)) = seq.next_element()? {
            items.push(item);
        }
        Ok(Strict(Value::Array(items)))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Strict, A::Error> {
        let mut out = Map::new();
        while let Some((key, Strict(value))) = map.next_entry::<String, Strict>()? {
            if out.contains_key(&key) {
                return Err(de::Error::custom(DUPLICATE));
            }
            out.insert(key, value);
        }
        Ok(Strict(Value::Object(out)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_normal_document_reads_like_serde_json() {
        let text = br#"{"a":[1,-2,3.5,true,null,"x"],"b":{"c":{}}}"#;
        let ours = parse(text, "kapot").unwrap();
        let theirs: Value = serde_json::from_slice(text).unwrap();
        assert_eq!(ours, theirs);
    }

    #[test]
    fn a_repeated_key_is_refused() {
        let err = parse(br#"{"total_net":"1.00","total_net":"999.00"}"#, "kapot").unwrap_err();
        assert!(
            err.message().contains("dubbele sleutel"),
            "{}",
            err.message()
        );
        assert!(!err.message().contains("total_net"));
    }

    #[test]
    fn a_repeated_key_deep_inside_is_refused() {
        let err = parse(br#"{"a":[{"b":1,"b":2}]}"#, "kapot").unwrap_err();
        assert!(err.message().contains("dubbele sleutel"));
    }

    #[test]
    fn the_same_key_in_two_objects_is_fine() {
        assert!(parse(
            br#"{"a":{"k":1},"b":{"k":2},"c":[{"k":1},{"k":2}]}"#,
            "kapot"
        )
        .is_ok());
    }

    #[test]
    fn broken_json_gives_the_given_message() {
        let err = parse(b"{", "kapot").unwrap_err();
        assert_eq!(err.message(), "kapot");
        let deep = "[".repeat(200_000);
        assert_eq!(
            parse(deep.as_bytes(), "kapot").unwrap_err().message(),
            "kapot"
        );
    }
}
