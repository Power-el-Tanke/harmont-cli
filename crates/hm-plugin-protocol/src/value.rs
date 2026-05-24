//! A self-describing dynamic value type that replaces `serde_json::Value`
//! on the FFI boundary. Unlike `serde_json::Value`, this type derives
//! both `serde` (for JSON compat) and `borsh` (for FFI serialisation).

use std::collections::BTreeMap;

use borsh::{BorshDeserialize, BorshSerialize};
use schemars::JsonSchema as DeriveJsonSchema;
use serde::{Deserialize, Serialize};

/// A dynamic value that can cross the plugin FFI boundary.
///
/// `#[serde(untagged)]` ensures JSON round-trips are identical to
/// `serde_json::Value` — raw JSON maps to the matching variant.
#[derive(Debug, Clone, PartialEq, BorshSerialize, BorshDeserialize, Serialize, Deserialize, DeriveJsonSchema)]
#[serde(untagged)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Array(Vec<Value>),
    Object(BTreeMap<String, Value>),
}

impl Value {
    /// Returns `true` if this value is `Null`.
    #[must_use]
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// Returns the contained string, if any.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s),
            _ => None,
        }
    }

    /// Returns the contained `i64`, if this is an `Int`.
    #[must_use]
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Int(n) => Some(*n),
            _ => None,
        }
    }

    /// Returns the contained `f64`, if this is a `Float`.
    #[must_use]
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Float(n) => Some(*n),
            _ => None,
        }
    }

    /// Returns the contained `bool`, if this is a `Bool`.
    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Returns a reference to the contained array, if any.
    #[must_use]
    pub fn as_array(&self) -> Option<&Vec<Value>> {
        match self {
            Self::Array(a) => Some(a),
            _ => None,
        }
    }

    /// Returns a reference to the contained object, if any.
    #[must_use]
    pub fn as_object(&self) -> Option<&BTreeMap<String, Value>> {
        match self {
            Self::Object(m) => Some(m),
            _ => None,
        }
    }

    /// Looks up a key in an `Object` variant. Returns `None` when
    /// `self` is not an object or when the key is absent.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.as_object()?.get(key)
    }
}

// ---------------------------------------------------------------------------
// Conversions: serde_json::Value <-> Value
// ---------------------------------------------------------------------------

impl From<serde_json::Value> for Value {
    fn from(v: serde_json::Value) -> Self {
        match v {
            serde_json::Value::Null => Self::Null,
            serde_json::Value::Bool(b) => Self::Bool(b),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Self::Int(i)
                } else if let Some(f) = n.as_f64() {
                    Self::Float(f)
                } else {
                    // u64 that doesn't fit in i64 — store as float (lossy but
                    // this matches serde_json's own behaviour for large u64).
                    #[allow(clippy::cast_precision_loss)]
                    Self::Float(n.as_u64().unwrap_or(0) as f64)
                }
            }
            serde_json::Value::String(s) => Self::Str(s),
            serde_json::Value::Array(a) => Self::Array(a.into_iter().map(Into::into).collect()),
            serde_json::Value::Object(m) => {
                Self::Object(m.into_iter().map(|(k, v)| (k, v.into())).collect())
            }
        }
    }
}

impl From<Value> for serde_json::Value {
    fn from(v: Value) -> Self {
        match v {
            Value::Null => Self::Null,
            Value::Bool(b) => Self::Bool(b),
            Value::Int(i) => Self::Number(i.into()),
            Value::Float(f) => {
                serde_json::Number::from_f64(f).map_or(Self::Null, Self::Number)
            }
            Value::Str(s) => Self::String(s),
            Value::Array(a) => Self::Array(a.into_iter().map(Into::into).collect()),
            Value::Object(m) => {
                Self::Object(m.into_iter().map(|(k, v)| (k, v.into())).collect())
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn serde_round_trip() {
        let json = r#"{"name":"test","count":42,"ok":true,"nested":{"x":1.5},"list":[1,2,3],"nil":null}"#;
        let v: Value = serde_json::from_str(json).unwrap();
        let back = serde_json::to_string(&v).unwrap();
        // Parse both into serde_json::Value to compare canonically.
        let a: serde_json::Value = serde_json::from_str(json).unwrap();
        let b: serde_json::Value = serde_json::from_str(&back).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn borsh_round_trip() {
        let v = Value::Object({
            let mut m = BTreeMap::new();
            m.insert("a".into(), Value::Int(1));
            m.insert("b".into(), Value::Str("hello".into()));
            m.insert("c".into(), Value::Array(vec![Value::Bool(true), Value::Null]));
            m
        });
        let bytes = borsh::to_vec(&v).unwrap();
        let decoded = Value::try_from_slice(&bytes).unwrap();
        assert_eq!(v, decoded);
    }

    #[test]
    fn from_serde_json_value() {
        let jv = serde_json::json!({"region": "us", "count": 3});
        let v: Value = jv.into();
        assert_eq!(v.get("region").and_then(Value::as_str), Some("us"));
        assert_eq!(v.get("count").and_then(Value::as_i64), Some(3));
    }

    #[test]
    fn into_serde_json_value() {
        let v = Value::Object({
            let mut m = BTreeMap::new();
            m.insert("x".into(), Value::Float(1.5));
            m
        });
        let jv: serde_json::Value = v.into();
        assert_eq!(jv, serde_json::json!({"x": 1.5}));
    }

    #[test]
    fn accessors() {
        assert!(Value::Null.is_null());
        assert!(!Value::Bool(true).is_null());
        assert_eq!(Value::Bool(false).as_bool(), Some(false));
        assert_eq!(Value::Int(42).as_i64(), Some(42));
        assert_eq!(Value::Float(3.14).as_f64(), Some(3.14));
        assert_eq!(Value::Str("hi".into()).as_str(), Some("hi"));
    }
}
