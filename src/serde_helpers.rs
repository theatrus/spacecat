//! Shared serde deserializers for NINA's "unknown" payload shapes.
//!
//! NINA's Advanced API has two recurring ways of saying "this value isn't
//! available":
//!
//!   * the field comes back as an empty JSON array `[]` (e.g. filter
//!     wheel `Name`/`Id` when no slot is selected, or every TS-TARGETSTART
//!     `Coordinates.*` when the target has no coords),
//!   * the field comes back as the JSON *string* `"NaN"` (the focuser
//!     `Temperature` when no sensor is attached; also `"Infinity"` /
//!     `"-Infinity"` show up in autofocus reports for unreached limits).
//!
//! These helpers accept the normal typed payload plus those sentinels and
//! map them to a per-type unknown value (NaN for floats, `-1` for filter
//! IDs, empty string for names/coords).

use serde::{Deserialize, Deserializer, de::Error};

/// `f64` field that may also arrive as a stringified `"NaN"` / `"Infinity"`
/// / `"-Infinity"`, as an empty array `[]`, or as `null`. All "unknown"
/// sentinels resolve to the appropriate `f64` value (`NAN` by default).
pub fn de_f64_tolerant<'de, D>(d: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    let v = serde_json::Value::deserialize(d)?;
    match v {
        serde_json::Value::Number(n) => n.as_f64().ok_or_else(|| D::Error::custom("not f64")),
        serde_json::Value::String(s) => match s.as_str() {
            "NaN" => Ok(f64::NAN),
            "Infinity" => Ok(f64::INFINITY),
            "-Infinity" => Ok(f64::NEG_INFINITY),
            other => other.parse::<f64>().map_err(D::Error::custom),
        },
        serde_json::Value::Array(a) if a.is_empty() => Ok(f64::NAN),
        serde_json::Value::Null => Ok(f64::NAN),
        other => Err(D::Error::custom(format!(
            "expected number, NaN string, [], or null; got {other}"
        ))),
    }
}

/// `String` field that may also arrive as an empty array `[]` — empty
/// arrays become the empty string.
pub fn de_string_tolerant<'de, D>(d: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let v = serde_json::Value::deserialize(d)?;
    match v {
        serde_json::Value::String(s) => Ok(s),
        serde_json::Value::Array(a) if a.is_empty() => Ok(String::new()),
        other => Err(D::Error::custom(format!(
            "expected string or []; got {other}"
        ))),
    }
}

/// `i32` field that may also arrive as an empty array `[]` — empty arrays
/// become `-1` (used as the "unknown filter id" sentinel).
pub fn de_i32_tolerant<'de, D>(d: D) -> Result<i32, D::Error>
where
    D: Deserializer<'de>,
{
    let v = serde_json::Value::deserialize(d)?;
    match v {
        serde_json::Value::Number(n) => n
            .as_i64()
            .ok_or_else(|| D::Error::custom("not an integer"))
            .map(|x| x as i32),
        serde_json::Value::Array(a) if a.is_empty() => Ok(-1),
        other => Err(D::Error::custom(format!(
            "expected integer or []; got {other}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct F(#[serde(deserialize_with = "de_f64_tolerant")] f64);
    #[derive(Deserialize)]
    struct S(#[serde(deserialize_with = "de_string_tolerant")] String);
    #[derive(Deserialize)]
    struct I(#[serde(deserialize_with = "de_i32_tolerant")] i32);

    #[test]
    fn f64_number() {
        let F(v) = serde_json::from_str("14.7").unwrap();
        assert!((v - 14.7).abs() < 1e-9);
    }

    #[test]
    fn f64_nan_string() {
        let F(v) = serde_json::from_str("\"NaN\"").unwrap();
        assert!(v.is_nan());
    }

    #[test]
    fn f64_inf_strings() {
        let F(v) = serde_json::from_str("\"Infinity\"").unwrap();
        assert!(v.is_infinite() && v > 0.0);
        let F(v) = serde_json::from_str("\"-Infinity\"").unwrap();
        assert!(v.is_infinite() && v < 0.0);
    }

    #[test]
    fn f64_empty_array_is_nan() {
        let F(v) = serde_json::from_str("[]").unwrap();
        assert!(v.is_nan());
    }

    #[test]
    fn f64_null_is_nan() {
        let F(v) = serde_json::from_str("null").unwrap();
        assert!(v.is_nan());
    }

    #[test]
    fn string_normal_and_empty_array() {
        let S(s) = serde_json::from_str("\"hello\"").unwrap();
        assert_eq!(s, "hello");
        let S(s) = serde_json::from_str("[]").unwrap();
        assert_eq!(s, "");
    }

    #[test]
    fn int_normal_and_empty_array() {
        let I(i) = serde_json::from_str("42").unwrap();
        assert_eq!(i, 42);
        let I(i) = serde_json::from_str("[]").unwrap();
        assert_eq!(i, -1);
    }
}
