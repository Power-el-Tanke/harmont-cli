//! Custom borsh serializers for third-party types that lack native
//! borsh support (`DateTime<Utc>`, `semver::Version`, `char`).
//!
//! These are used via `#[borsh(serialize_with = ..., deserialize_with = ...)]`
//! field attributes.

use std::io::{self, Read, Write};

use borsh::{BorshDeserialize, BorshSerialize};
use chrono::{DateTime, TimeZone, Utc};

// ---------------------------------------------------------------------------
// DateTime<Utc>  <->  i64 (milliseconds since epoch)
// ---------------------------------------------------------------------------

pub(crate) fn serialize_datetime<W: Write>(dt: &DateTime<Utc>, writer: &mut W) -> io::Result<()> {
    dt.timestamp_millis().serialize(writer)
}

pub(crate) fn deserialize_datetime<R: Read>(reader: &mut R) -> io::Result<DateTime<Utc>> {
    let millis = i64::deserialize_reader(reader)?;
    Utc.timestamp_millis_opt(millis)
        .single()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid timestamp millis"))
}

// ---------------------------------------------------------------------------
// char  <->  u32  (Unicode scalar value)
// ---------------------------------------------------------------------------

pub(crate) fn serialize_char<W: Write>(c: &char, writer: &mut W) -> io::Result<()> {
    (*c as u32).serialize(writer)
}

pub(crate) fn deserialize_char<R: Read>(reader: &mut R) -> io::Result<char> {
    let n = u32::deserialize_reader(reader)?;
    char::from_u32(n)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid char codepoint"))
}

pub(crate) fn serialize_option_char<W: Write>(opt: &Option<char>, writer: &mut W) -> io::Result<()> {
    match opt {
        None => 0u8.serialize(writer),
        Some(c) => {
            1u8.serialize(writer)?;
            serialize_char(c, writer)
        }
    }
}

pub(crate) fn deserialize_option_char<R: Read>(reader: &mut R) -> io::Result<Option<char>> {
    let tag = u8::deserialize_reader(reader)?;
    match tag {
        0 => Ok(None),
        1 => deserialize_char(reader).map(Some),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid Option tag",
        )),
    }
}

// ---------------------------------------------------------------------------
// semver::Version  <->  String
// ---------------------------------------------------------------------------

pub(crate) fn serialize_semver<W: Write>(v: &semver::Version, writer: &mut W) -> io::Result<()> {
    v.to_string().serialize(writer)
}

pub(crate) fn deserialize_semver<R: Read>(reader: &mut R) -> io::Result<semver::Version> {
    let s = String::deserialize_reader(reader)?;
    s.parse::<semver::Version>()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}
