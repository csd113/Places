//! Refuse source values the editor's JSON representation cannot preserve.

use std::collections::BTreeSet;
use std::fmt;

use serde::Deserialize;
use serde::de::{self, MapAccess, SeqAccess, Visitor};

pub(super) fn check_editable_source(bytes: &[u8]) -> Result<(), String> {
    check_number_precision(bytes)?;
    let mut parser = serde_json::Deserializer::from_slice(bytes);
    let _checked = UniqueKeys::deserialize(&mut parser).map_err(|error| {
        format!("Cannot safely edit source with duplicate/unsupported JSON: {error}")
    })?;
    parser.end().map_err(|error| error.to_string())
}

fn check_number_precision(bytes: &[u8]) -> Result<(), String> {
    let mut quoted = false;
    let mut escaped = false;
    let mut index = 0_usize;
    while let Some(byte) = bytes.get(index).copied() {
        if quoted {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = false;
            }
        } else if byte == b'"' {
            quoted = true;
        } else if byte == b'-' || byte.is_ascii_digit() {
            let begin = index;
            index = index.saturating_add(1);
            while bytes.get(index).is_some_and(|candidate| {
                candidate.is_ascii_digit() || matches!(candidate, b'-' | b'+' | b'.' | b'e' | b'E')
            }) {
                index = index.saturating_add(1);
            }
            let token = bytes
                .get(begin..index)
                .ok_or("Invalid JSON number bounds")?;
            let original = std::str::from_utf8(token).map_err(|error| error.to_string())?;
            let parsed: serde_json::Number = serde_json::from_slice(token)
                .map_err(|error| format!("{} ({error})", precision_error()))?;
            if normalize_number(original)? != normalize_number(&parsed.to_string())? {
                return Err(precision_error());
            }
            continue;
        }
        index = index.saturating_add(1);
    }
    Ok(())
}

fn precision_error() -> String {
    "Unsupported numeric precision: this source contains a number the editor cannot preserve exactly; the source was not changed".into()
}

/// Decimal normalization compares mathematical values, so 1e-06 and 0.000001
/// are equivalent while a rounded large integer or long fraction is refused.
fn normalize_number(text: &str) -> Result<(bool, String, i64), String> {
    let (mantissa, exponent) = match text.split_once(['e', 'E']) {
        Some((mantissa, exponent)) => (
            mantissa,
            exponent
                .parse::<i64>()
                .map_err(|error| format!("{} ({error})", precision_error()))?,
        ),
        None => (text, 0_i64),
    };
    let negative = mantissa.starts_with('-');
    let unsigned = mantissa.strip_prefix('-').unwrap_or(mantissa);
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    let digits = format!("{whole}{fraction}");
    let leading_trimmed = digits.trim_start_matches('0');
    let significant = leading_trimmed.trim_end_matches('0');
    if significant.is_empty() {
        return Ok((false, String::new(), 0));
    }
    let fraction_count = i64::try_from(fraction.len())
        .map_err(|error| format!("{} ({error})", precision_error()))?;
    let trailing_count = i64::try_from(leading_trimmed.len().saturating_sub(significant.len()))
        .map_err(|error| format!("{} ({error})", precision_error()))?;
    let normalized_exponent = exponent
        .checked_sub(fraction_count)
        .and_then(|value| value.checked_add(trailing_count))
        .ok_or_else(precision_error)?;
    Ok((negative, significant.into(), normalized_exponent))
}

struct UniqueKeys;

impl<'de> Deserialize<'de> for UniqueKeys {
    fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(UniqueVisitor)
    }
}

struct UniqueVisitor;

impl<'de> Visitor<'de> for UniqueVisitor {
    type Value = UniqueKeys;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("JSON with unique object keys")
    }

    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
        let mut keys = BTreeSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !keys.insert(key.clone()) {
                return Err(de::Error::custom(format!("duplicate key '{key}'")));
            }
            let _checked = map.next_value::<UniqueKeys>()?;
        }
        Ok(UniqueKeys)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
        while sequence.next_element::<UniqueKeys>()?.is_some() {}
        Ok(UniqueKeys)
    }

    fn visit_bool<E: de::Error>(self, _value: bool) -> Result<Self::Value, E> {
        Ok(UniqueKeys)
    }

    fn visit_i64<E: de::Error>(self, _value: i64) -> Result<Self::Value, E> {
        Ok(UniqueKeys)
    }

    fn visit_u64<E: de::Error>(self, _value: u64) -> Result<Self::Value, E> {
        Ok(UniqueKeys)
    }

    fn visit_f64<E: de::Error>(self, _value: f64) -> Result<Self::Value, E> {
        Ok(UniqueKeys)
    }

    fn visit_str<E: de::Error>(self, _value: &str) -> Result<Self::Value, E> {
        Ok(UniqueKeys)
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(UniqueKeys)
    }
}
