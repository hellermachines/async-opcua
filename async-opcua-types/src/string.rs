// OPCUA for Rust
// SPDX-License-Identifier: MPL-2.0
// Copyright (C) 2017-2024 Adam Lock

//! Contains the implementation of `UAString`.

use bstr::{BString, ByteSlice};
use std::{
    fmt,
    io::{Read, Write},
};

use crate::{
    encoding::{process_decode_io_result, process_encode_io_result, write_i32, EncodingResult},
    read_i32, DecodingOptions, Error, OutOfRange, SimpleBinaryDecodable, SimpleBinaryEncodable,
    UaNullable,
};

/// To avoid naming conflict hell, the OPC UA String type is typed `UAString` so it does not collide
/// with the Rust `String`.
///
/// OPC UA specifies UTF-8 strings. Binary decoding preserves all received bytes, including invalid
/// UTF-8, and binary encoding writes them unchanged. A null value is distinct from an empty string.
/// Use [`UAString::as_str`] to validate UTF-8 before interpreting the bytes as text.
#[derive(Eq, PartialEq, Debug, Clone, Hash)]
pub struct UAString {
    value: Option<BString>,
}

impl fmt::Display for UAString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.value.as_ref() {
            Some(value) => match value.to_str() {
                Ok(value) => f.write_str(value),
                Err(_) => {
                    f.write_str("[invalid UTF-8 bytes:")?;
                    for byte in value.as_bytes() {
                        write!(f, " {byte:02x}")?;
                    }
                    f.write_str("]")
                }
            },
            None => f.write_str("[null]"),
        }
    }
}

impl UaNullable for UAString {
    fn is_ua_null(&self) -> bool {
        self.is_null()
    }
}

#[cfg(feature = "json")]
mod json {
    use std::io::{Read, Write};
    use struson::{
        reader::{JsonReader, JsonStreamReader, ValueType},
        writer::{JsonStreamWriter, JsonWriter},
    };

    use crate::json::{Context, JsonDecodable, JsonEncodable};

    use super::{EncodingResult, UAString};

    impl JsonEncodable for UAString {
        fn encode(
            &self,
            stream: &mut JsonStreamWriter<&mut dyn Write>,
            _ctx: &Context<'_>,
        ) -> EncodingResult<()> {
            match self.as_str() {
                Ok(Some(value)) => stream.string_value(value)?,
                Ok(None) => stream.null_value()?,
                Err(error) => {
                    return Err(crate::Error::encoding(format!(
                        "Cannot encode invalid UTF-8 UAString as JSON: {error}"
                    )));
                }
            }

            Ok(())
        }
    }

    impl JsonDecodable for UAString {
        fn decode(
            stream: &mut JsonStreamReader<&mut dyn Read>,
            _ctx: &Context<'_>,
        ) -> EncodingResult<Self> {
            match stream.peek()? {
                ValueType::String => Ok(stream.next_string()?.into()),
                _ => {
                    stream.next_null()?;
                    Ok(UAString::null())
                }
            }
        }
    }
}

impl SimpleBinaryEncodable for UAString {
    fn byte_len(&self) -> usize {
        // Length plus the actual string length in bytes for a non-null string.
        4 + match &self.value {
            Some(s) => s.len(),
            None => 0,
        }
    }

    fn encode<S: Write + ?Sized>(&self, stream: &mut S) -> EncodingResult<()> {
        // Strings are encoded as UTF8 chars preceded by an Int32 length. A -1 indicates a null string
        match &self.value {
            Some(s) => {
                let len = i32::try_from(s.len()).map_err(|error| {
                    Error::encoding(format!(
                        "String length exceeds the binary encoding limit: {error}"
                    ))
                })?;
                write_i32(stream, len)?;
                process_encode_io_result(stream.write_all(s.as_bytes()))
            }
            None => write_i32(stream, -1),
        }
    }
}

impl SimpleBinaryDecodable for UAString {
    fn decode<S: Read + ?Sized>(
        stream: &mut S,
        decoding_options: &DecodingOptions,
    ) -> EncodingResult<Self> {
        let len = read_i32(stream)?;
        // Null string?
        if len == -1 {
            Ok(UAString::null())
        } else if len < -1 {
            Err(Error::decoding(format!(
                "String buf length is a negative number {len}"
            )))
        } else if len as usize > decoding_options.max_string_length {
            Err(Error::decoding(format!(
                "String buf length {} exceeds decoding limit {}",
                len, decoding_options.max_string_length
            )))
        } else {
            // Create a buffer filled with zeroes and read the string over the top
            let mut buf = vec![0u8; len as usize];
            process_decode_io_result(stream.read_exact(&mut buf))?;
            Ok(UAString::from(buf))
        }
    }
}

#[cfg(feature = "xml")]
mod xml {
    use crate::xml::*;
    use std::io::{Read, Write};

    use super::UAString;

    impl XmlType for UAString {
        const TAG: &'static str = "String";
    }

    impl XmlEncodable for UAString {
        fn encode(
            &self,
            writer: &mut XmlStreamWriter<&mut dyn Write>,
            _ctx: &Context<'_>,
        ) -> Result<(), Error> {
            match self.as_str() {
                Ok(Some(value)) => writer.write_text(value)?,
                Ok(None) => {}
                Err(error) => {
                    return Err(crate::Error::encoding(format!(
                        "Cannot encode invalid UTF-8 UAString as XML: {error}"
                    )));
                }
            }

            Ok(())
        }
    }

    impl XmlDecodable for UAString {
        fn decode(
            read: &mut XmlStreamReader<&mut dyn Read>,
            _context: &Context<'_>,
        ) -> Result<Self, Error> {
            Ok(read.consume_as_text()?.into())
        }
    }
}

impl<'a> From<&'a str> for UAString {
    fn from(value: &'a str) -> Self {
        Self::from(value.to_string())
    }
}

impl From<&String> for UAString {
    fn from(value: &String) -> Self {
        UAString::from(value.as_str())
    }
}

impl From<String> for UAString {
    fn from(value: String) -> Self {
        UAString {
            value: Some(BString::from(value.into_bytes())),
        }
    }
}

impl From<Vec<u8>> for UAString {
    fn from(value: Vec<u8>) -> Self {
        UAString {
            value: Some(BString::from(value)),
        }
    }
}

impl From<BString> for UAString {
    fn from(value: BString) -> Self {
        UAString { value: Some(value) }
    }
}

impl From<Option<String>> for UAString {
    fn from(value: Option<String>) -> Self {
        UAString::from(value.map(|value| BString::from(value.into_bytes())))
    }
}

impl From<Option<BString>> for UAString {
    fn from(value: Option<BString>) -> Self {
        UAString { value }
    }
}

impl Default for UAString {
    fn default() -> Self {
        UAString::null()
    }
}

impl PartialEq<str> for UAString {
    fn eq(&self, other: &str) -> bool {
        self.as_str()
            .ok()
            .flatten()
            .is_some_and(|value| value == other)
    }
}

impl PartialEq<&str> for UAString {
    fn eq(&self, other: &&str) -> bool {
        self.as_str()
            .ok()
            .flatten()
            .is_some_and(|value| value == *other)
    }
}

impl PartialEq<&String> for UAString {
    fn eq(&self, other: &&String) -> bool {
        self.as_str()
            .ok()
            .flatten()
            .is_some_and(|value| value == other.as_str())
    }
}

impl PartialEq<String> for UAString {
    fn eq(&self, other: &String) -> bool {
        self.as_str()
            .ok()
            .flatten()
            .is_some_and(|value| value == other.as_str())
    }
}

impl UAString {
    /// Get the inner raw value.
    pub fn value(&self) -> &Option<BString> {
        &self.value
    }

    /// Set the inner value.
    pub fn set_value(&mut self, value: Option<BString>) {
        self.value = value;
    }

    /// Return the bytes in the string, or `None` when the string is null.
    pub fn as_bytes(&self) -> Option<&[u8]> {
        self.value.as_ref().map(|value| value.as_bytes())
    }

    /// Interpret the string as UTF-8, preserving null as `None`.
    ///
    /// # Errors
    /// Returns an error if the non-null value is not valid UTF-8.
    pub fn as_str(&self) -> Result<Option<&str>, bstr::Utf8Error> {
        self.value.as_ref().map(|value| value.to_str()).transpose()
    }

    /// Returns true if the string is null or empty, false otherwise
    pub fn is_empty(&self) -> bool {
        self.value.is_none() || self.value.as_ref().is_some_and(|v| v.is_empty())
    }

    /// Returns the length of the string in bytes or -1 for null.
    pub fn len(&self) -> isize {
        if let Some(v) = &self.value {
            v.len() as isize
        } else {
            -1
        }
    }

    /// Create a null string (not the same as an empty string).
    pub fn null() -> UAString {
        UAString { value: None }
    }

    /// Test if the string is null.
    pub fn is_null(&self) -> bool {
        self.value.is_none()
    }

    /// This function is meant for use with NumericRange. It creates a substring using byte offsets
    /// from min up to and inclusive of max, without requiring UTF-8 character boundaries. Note that
    /// min must have an index within the string, but max may be beyond the end, in which case the
    /// remainder is returned (see docs for NumericRange).
    pub fn substring(&self, min: usize, max: usize) -> Result<UAString, OutOfRange> {
        let Some(value) = self.value.as_ref() else {
            return Err(OutOfRange);
        };
        let bytes = value.as_bytes();
        if min >= bytes.len() || min > max {
            return Err(OutOfRange);
        }
        let max = max.min(bytes.len() - 1);
        let Some(substring) = bytes.get(min..=max) else {
            return Err(OutOfRange);
        };
        Ok(UAString::from(substring.to_vec()))
    }
}

#[test]
fn string_null() {
    let s = UAString::null();
    assert!(s.is_null());
    assert!(s.is_empty());
    assert_eq!(s.len(), -1);
}

#[test]
fn string_empty() {
    let s = UAString::from("");
    assert!(!s.is_null());
    assert!(s.is_empty());
    assert_eq!(s.len(), 0);
}

#[test]
fn string_value() {
    let v = "Mary had a little lamb";
    let s = UAString::from(v);
    assert!(!s.is_null());
    assert!(!s.is_empty());
    assert_eq!(s.as_str().ok().flatten(), Some(v));
}

#[test]
#[allow(clippy::comparison_to_empty)]
fn string_eq() {
    let s = UAString::null();
    assert!(!s.eq(""));

    let s = UAString::from("");
    assert!(s.eq(""));

    let s = UAString::from("Sunshine");
    assert!(s.ne("Moonshine"));
    assert!(s.eq("Sunshine"));
    assert!(!s.eq("Sunshine "));
}

#[test]
fn string_substring() {
    let a = "Mary had a little lamb";
    let v = UAString::from(a);
    let v2 = v.substring(0, 4).unwrap();
    let a2 = v2.as_str().ok().flatten();
    assert_eq!(a2, Some("Mary "));

    let v2 = v.substring(2, 2).unwrap();
    let a2 = v2.as_str().ok().flatten();
    assert_eq!(a2, Some("r"));

    let v2 = v.substring(0, 2000).unwrap();
    assert_eq!(v, v2);
    assert_eq!(v2.as_str().ok().flatten(), Some(a));

    assert!(v.substring(22, 10000).is_err());

    assert!(UAString::null().substring(0, 0).is_err());
}

#[test]
fn invalid_utf8_binary_string_decodes() -> EncodingResult<()> {
    let wire_bytes = [3, 0, 0, 0, 0xff, b'A', 0x80];
    let decoded = UAString::decode(
        &mut std::io::Cursor::new(wire_bytes),
        &DecodingOptions::default(),
    )?;

    assert_eq!(decoded.as_bytes(), Some(&wire_bytes[4..]));
    assert!(decoded.as_str().is_err());
    assert_eq!(decoded.to_string(), "[invalid UTF-8 bytes: ff 41 80]");
    let mut encoded = Vec::new();
    decoded.encode(&mut encoded)?;
    assert_eq!(encoded, wire_bytes);
    Ok(())
}

#[test]
fn binary_string_roundtrip_preserves_bytes_and_null() -> EncodingResult<()> {
    let cases: &[Option<&[u8]>] = &[
        None,
        Some(b""),
        Some(b"plain"),
        Some(b"\xc3\xa4"),
        Some(b"M\xfcller \xc3\xa4"),
        Some(&[0, 0xff, 0x80]),
    ];
    for bytes in cases {
        let value = match bytes {
            Some(bytes) => UAString::from(bytes.to_vec()),
            None => UAString::null(),
        };
        let mut wire = Vec::new();
        value.encode(&mut wire)?;
        assert_eq!(wire.len(), value.byte_len());
        let decoded = UAString::decode(
            &mut std::io::Cursor::new(&wire),
            &DecodingOptions::default(),
        )?;
        assert_eq!(decoded.as_bytes(), *bytes);
        let mut encoded = Vec::new();
        decoded.encode(&mut encoded)?;
        assert_eq!(encoded, wire);
    }
    Ok(())
}

#[test]
fn binary_string_length_limits_and_truncated_data() {
    let options = DecodingOptions {
        max_string_length: 3,
        ..DecodingOptions::default()
    };
    let at_limit = [3, 0, 0, 0, b'A', 0xff, b'B'];
    assert!(UAString::decode(&mut std::io::Cursor::new(at_limit), &options).is_ok());
    let above_limit = [4, 0, 0, 0];
    assert!(matches!(
        UAString::decode(&mut std::io::Cursor::new(above_limit), &options),
        Err(error) if error.to_string().contains("exceeds decoding limit")
    ));
    let negative_length = [0xfe, 0xff, 0xff, 0xff];
    assert!(UAString::decode(&mut std::io::Cursor::new(negative_length), &options).is_err());
    let truncated = [3, 0, 0, 0, b'A', b'B'];
    assert!(UAString::decode(&mut std::io::Cursor::new(truncated), &options).is_err());

    let zero_limit = DecodingOptions {
        max_string_length: 0,
        ..DecodingOptions::default()
    };
    let null = [0xff, 0xff, 0xff, 0xff];
    let empty = [0, 0, 0, 0];
    let non_empty = [1, 0, 0, 0, b'A'];
    assert!(UAString::decode(&mut std::io::Cursor::new(null), &zero_limit).is_ok());
    assert!(UAString::decode(&mut std::io::Cursor::new(empty), &zero_limit).is_ok());
    assert!(UAString::decode(&mut std::io::Cursor::new(non_empty), &zero_limit).is_err());
}

#[test]
fn utf8_access_preserves_null_and_rejects_invalid_bytes() {
    assert!(matches!(UAString::null().as_str(), Ok(None)));
    assert!(matches!(UAString::from("").as_str(), Ok(Some(""))));
    assert!(matches!(
        UAString::from(vec![0xc3, 0xa4]).as_str(),
        Ok(Some("\u{e4}"))
    ));
    assert!(UAString::from(vec![0xe4]).as_str().is_err());
}

#[test]
fn substring_uses_byte_offsets() -> Result<(), OutOfRange> {
    let value = UAString::from(vec![b'A', 0xc3, 0xa4, 0xff]);
    let character = value.substring(1, 2)?;
    assert_eq!(character.as_bytes(), Some(&[0xc3, 0xa4][..]));
    let partial = value.substring(2, 3)?;
    assert_eq!(partial.as_bytes(), Some(&[0xa4, 0xff][..]));
    assert!(partial.as_str().is_err());
    assert!(value.substring(3, 2).is_err());
    Ok(())
}

#[test]
fn localized_text_variant_preserves_alarm_bytes() -> EncodingResult<()> {
    let bytes = b"Alarm \xfc \xc3\xa4";
    let value = crate::Variant::LocalizedText(Box::new(crate::LocalizedText {
        locale: UAString::from("de-DE"),
        text: UAString::from(bytes.to_vec()),
    }));
    let owned = crate::ContextOwned::default();
    let context = owned.context();
    let mut wire = Vec::new();
    crate::BinaryEncodable::encode(&value, &mut wire, &context)?;
    let decoded = <crate::Variant as crate::BinaryDecodable>::decode(
        &mut std::io::Cursor::new(&wire),
        &context,
    )?;
    assert_eq!(decoded, value);
    let crate::Variant::LocalizedText(message) = decoded else {
        return Err(Error::decoding("Expected a LocalizedText variant"));
    };
    assert_eq!(message.text.as_bytes(), Some(&bytes[..]));
    assert!(message.text.as_str().is_err());
    Ok(())
}

#[test]
fn node_id_hashes_match_borrowed_identifiers() {
    use std::{
        collections::hash_map::DefaultHasher,
        hash::{Hash, Hasher},
    };

    fn hash(value: &impl Hash) -> u64 {
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        hasher.finish()
    }

    let node = crate::NodeId::new(2, "alpha");
    let reference = crate::node_id::NodeIdRef::from((2u16, "alpha"));
    assert_eq!(node, reference);
    assert_eq!(hash(&node), hash(&reference));

    let raw = UAString::from(vec![0xff, b'A']);
    let node = crate::NodeId::new(2, raw.clone());
    let equal = crate::NodeId::new(2, raw.clone());
    let reference = crate::node_id::NodeIdRef::from((2u16, &raw));
    assert_eq!(node, equal);
    assert_eq!(node, reference);
    assert_eq!(hash(&node), hash(&equal));
    assert_eq!(hash(&node), hash(&reference));
    assert_ne!(
        node,
        crate::NodeId::new(2, UAString::from(vec![0xff, b'B']))
    );
    assert_ne!(
        crate::NodeId::new(2, UAString::null()),
        crate::NodeId::new(2, "")
    );
}
