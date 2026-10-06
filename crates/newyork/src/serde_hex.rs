//! Hexadecimal serde representations for the IR's numeric and byte-string fields.
//!
//! A derived implementation would encode a [`BigUint`] as an array of `u32` digits
//! and a byte vector as an array of integers. Both forms are several times larger
//! than the hexadecimal text and unreadable in a dumped stage artifact, where the
//! reader is matching literals against Yul source or an SMT counterexample.
//!
//! Numbers and byte strings are deliberately encoded differently: a number drops
//! leading zeros (`0x1f`), while a byte string keeps two digits per byte so that
//! its length survives the round trip.

use std::collections::BTreeMap;

use num::BigUint;
use num::Num;
use serde::de::Error;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serializer;

/// The prefix every hexadecimal string in the IR artifacts carries.
const HEX_PREFIX: &str = "0x";

/// The radix of every hexadecimal string in the IR artifacts.
const HEX_RADIX: u32 = 16;

/// Strips [`HEX_PREFIX`], reporting the offending text when it is absent.
fn strip_prefix(text: &str) -> Result<&str, String> {
    text.strip_prefix(HEX_PREFIX).ok_or_else(|| {
        format!("expected a `{HEX_PREFIX}` prefixed hexadecimal string, got `{text}`")
    })
}

/// A `0x`-prefixed hexadecimal integer, for use with `#[serde(with = ...)]`.
pub mod biguint {
    use super::*;

    /// Serializes `value` as a `0x`-prefixed lowercase hexadecimal integer.
    pub fn serialize<S: Serializer>(value: &BigUint, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&format_args!("{HEX_PREFIX}{value:x}"))
    }

    /// Parses a `0x`-prefixed hexadecimal integer written by [`serialize`].
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<BigUint, D::Error> {
        let text = String::deserialize(deserializer)?;
        parse(&text).map_err(D::Error::custom)
    }
}

/// An optional [`biguint`], absent as `null`.
pub mod optional_biguint {
    use super::*;

    /// Serializes `value` as a hexadecimal integer, or as `null` when absent.
    pub fn serialize<S: Serializer>(
        value: &Option<BigUint>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match value {
            Some(value) => biguint::serialize(value, serializer),
            None => serializer.serialize_none(),
        }
    }

    /// Parses the output of [`serialize`].
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<BigUint>, D::Error> {
        let text = Option::<String>::deserialize(deserializer)?;
        text.map(|text| parse(&text))
            .transpose()
            .map_err(D::Error::custom)
    }
}

/// A sequence of [`biguint`] values.
pub mod biguint_sequence {
    use super::*;

    /// Serializes every element as a hexadecimal integer.
    pub fn serialize<S: Serializer>(values: &[BigUint], serializer: S) -> Result<S::Ok, S::Error> {
        let texts = values
            .iter()
            .map(|value| format!("{HEX_PREFIX}{value:x}"))
            .collect::<Vec<_>>();
        serde::Serialize::serialize(&texts, serializer)
    }

    /// Parses the output of [`serialize`].
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<BigUint>, D::Error> {
        let texts = Vec::<String>::deserialize(deserializer)?;
        texts
            .iter()
            .map(|text| parse(text))
            .collect::<Result<Vec<_>, _>>()
            .map_err(D::Error::custom)
    }
}

/// A map of named byte strings, each `0x`-prefixed with two digits per byte.
pub mod byte_string_map {
    use super::*;

    /// Serializes every value as a `0x`-prefixed byte string.
    pub fn serialize<S: Serializer>(
        values: &BTreeMap<String, Vec<u8>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let texts = values
            .iter()
            .map(|(name, bytes)| (name.clone(), format!("{HEX_PREFIX}{}", hex::encode(bytes))))
            .collect::<BTreeMap<_, _>>();
        serde::Serialize::serialize(&texts, serializer)
    }

    /// Parses the output of [`serialize`].
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<BTreeMap<String, Vec<u8>>, D::Error> {
        let texts = BTreeMap::<String, String>::deserialize(deserializer)?;
        texts
            .into_iter()
            .map(|(name, text)| decode_byte_string(&text).map(|bytes| (name, bytes)))
            .collect::<Result<BTreeMap<_, _>, _>>()
            .map_err(D::Error::custom)
    }
}

/// Parses a `0x`-prefixed hexadecimal integer.
fn parse(text: &str) -> Result<BigUint, String> {
    let digits = strip_prefix(text)?;
    BigUint::from_str_radix(digits, HEX_RADIX)
        .map_err(|error| format!("invalid hexadecimal integer `{text}`: {error}"))
}

/// Parses a `0x`-prefixed byte string, two digits per byte.
fn decode_byte_string(text: &str) -> Result<Vec<u8>, String> {
    let digits = strip_prefix(text)?;
    hex::decode(digits).map_err(|error| format!("invalid byte string `{text}`: {error}"))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use num::BigUint;

    use crate::ir::BitWidth;
    use crate::ir::Expression;
    use crate::ir::Function;
    use crate::ir::FunctionId;
    use crate::ir::Object;
    use crate::ir::Region;
    use crate::ir::Statement;
    use crate::ir::SwitchCase;
    use crate::ir::Type;
    use crate::ir::Value;
    use crate::ir::ValueId;

    /// Builds an object touching every hexadecimal field: a bare [`BigUint`]
    /// (literal, custom error selector, switch case), an optional one (the static
    /// storage slots), a sequence of them (the error string words) and the byte
    /// string map (the object's data sections).
    fn fixture() -> Object {
        let mut object = Object::new("Test".to_owned());

        object.code.push(Statement::Let {
            bindings: vec![ValueId(0)],
            value: Expression::Literal {
                value: BigUint::from(0xdead_beef_u64),
                value_type: Type::Int(BitWidth::I256),
            },
        });
        object.code.push(Statement::Let {
            bindings: vec![ValueId(1)],
            value: Expression::Literal {
                value: BigUint::from(0u8),
                value_type: Type::Int(BitWidth::I256),
            },
        });
        object.code.push(Statement::Let {
            bindings: vec![ValueId(2)],
            value: Expression::SLoad {
                key: Value::int(ValueId(0)),
                static_slot: Some(BigUint::from(7u8)),
            },
        });
        object.code.push(Statement::Let {
            bindings: vec![ValueId(3)],
            value: Expression::SLoad {
                key: Value::int(ValueId(0)),
                static_slot: None,
            },
        });
        object.code.push(Statement::ErrorStringRevert {
            length: 5,
            data: vec![BigUint::from(1u8), BigUint::from(1u8) << 200],
        });
        object.code.push(Statement::CustomErrorRevert {
            selector: BigUint::from(1u8) << 224,
            arguments: vec![Value::int(ValueId(0))],
        });
        object.code.push(Statement::Switch {
            scrutinee: Value::int(ValueId(0)),
            inputs: Vec::new(),
            cases: vec![SwitchCase {
                value: BigUint::from(0xff_u8),
                body: Region::new(),
            }],
            default: None,
            outputs: Vec::new(),
        });

        // Leading and trailing zero bytes must survive; a number encoding drops them.
        object
            .data
            .insert("x".to_owned(), vec![0x00, 0x01, 0xff, 0x00]);
        object.data.insert("empty".to_owned(), Vec::new());

        // A `BTreeMap` keyed by a newtype: JSON object keys must be strings, so
        // this is the field most likely to break the round trip.
        let mut function = Function::new(FunctionId(3), "helper".to_owned());
        function
            .parameters
            .push((ValueId(4), Type::Int(BitWidth::I256)));
        function.returns.push(Type::Int(BitWidth::I64));
        object.functions.insert(FunctionId(3), function);

        object
            .subobjects
            .push(Object::new("Test_deployed".to_owned()));
        object
    }

    #[test]
    fn round_trip_is_lossless() {
        let original = fixture();
        let json = serde_json::to_string(&original).expect("the fixture should serialize");
        let parsed: Object = serde_json::from_str(&json).expect("the fixture should deserialize");
        let reserialized = serde_json::to_string(&parsed).expect("the parse should re-serialize");

        assert_eq!(json, reserialized, "the round trip changed the encoding");
    }

    #[test]
    fn numbers_and_byte_strings_use_distinct_forms() {
        let json = serde_json::to_string(&fixture()).expect("the fixture should serialize");

        assert!(json.contains(r#""value":"0xdeadbeef""#), "literal: {json}");
        assert!(json.contains(r#""value":"0x0""#), "zero: {json}");
        assert!(
            json.contains(r#""static_slot":"0x7""#),
            "static slot: {json}"
        );
        assert!(
            json.contains(r#""static_slot":null"#),
            "absent slot: {json}"
        );
        assert!(
            json.contains(r#"["0x1","0x100000000000000000000000000000000000000000000000000"]"#),
            "sequence: {json}",
        );
        assert!(
            json.contains(r#""x":"0x0001ff00""#),
            "byte string zeros: {json}"
        );
        assert!(
            json.contains(r#""empty":"0x""#),
            "empty byte string: {json}"
        );
        // JSON object keys must be strings; `FunctionId` is a newtype over `u32`.
        assert!(
            json.contains(r#""functions":{"3":"#),
            "newtype map key: {json}"
        );

        // A derived implementation would emit the digit array `[3735928559]` here.
        assert!(!json.contains(r#""value":["#), "digit array leaked: {json}");
        assert!(
            !json.contains(r#""selector":["#),
            "digit array leaked: {json}"
        );
    }

    #[test]
    fn malformed_input_is_rejected() {
        let json = serde_json::to_string(&fixture()).expect("the fixture should serialize");

        for (description, broken) in [
            (
                "an unprefixed integer",
                json.replace(r#""0xdeadbeef""#, r#""deadbeef""#),
            ),
            (
                "an odd-length byte string",
                json.replace(r#""0x0001ff00""#, r#""0x001""#),
            ),
            (
                "a non-hexadecimal digit",
                json.replace(r#""0xdeadbeef""#, r#""0xnope""#),
            ),
        ] {
            assert!(
                serde_json::from_str::<Object>(&broken).is_err(),
                "{description} was accepted",
            );
        }
    }

    #[test]
    fn data_sections_survive_a_round_trip() {
        let original = fixture();
        let json = serde_json::to_string(&original).expect("the fixture should serialize");
        let parsed: Object = serde_json::from_str(&json).expect("the fixture should deserialize");

        assert_eq!(
            parsed.data,
            BTreeMap::from([
                ("x".to_owned(), vec![0x00, 0x01, 0xff, 0x00]),
                ("empty".to_owned(), Vec::new()),
            ]),
        );
    }
}
