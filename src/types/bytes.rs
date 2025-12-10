/// Python bytes type, wrapping a `Vec<u8>`.
///
/// This type provides Python bytes semantics. Currently supports basic
/// operations like length and equality comparison.
use std::fmt::Write;

use ahash::AHashSet;

use crate::heap::{Heap, HeapId};
use crate::intern::Interns;
use crate::resource::ResourceTracker;
use crate::types::PyTrait;

/// Python bytes value stored on the heap.
///
/// Wraps a `Vec<u8>` and provides Python-compatible operations.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Bytes(Vec<u8>);

impl Bytes {
    /// Creates a new Bytes from a byte vector.
    #[must_use]
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// Returns a reference to the inner byte slice.
    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }

    /// Returns a mutable reference to the inner byte vector.
    pub fn as_vec_mut(&mut self) -> &mut Vec<u8> {
        &mut self.0
    }
}

impl From<Vec<u8>> for Bytes {
    fn from(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}

impl From<&[u8]> for Bytes {
    fn from(bytes: &[u8]) -> Self {
        Self(bytes.to_vec())
    }
}

impl From<Bytes> for Vec<u8> {
    fn from(bytes: Bytes) -> Self {
        bytes.0
    }
}

impl std::ops::Deref for Bytes {
    type Target = Vec<u8>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl PyTrait for Bytes {
    fn py_type<T: ResourceTracker>(&self, _heap: Option<&Heap<T>>) -> &'static str {
        "bytes"
    }

    fn py_estimate_size(&self) -> usize {
        std::mem::size_of::<Self>() + self.0.len()
    }

    fn py_len<T: ResourceTracker>(&self, _heap: &Heap<T>, _interns: &Interns) -> Option<usize> {
        Some(self.0.len())
    }

    fn py_eq<T: ResourceTracker>(&self, other: &Self, _heap: &mut Heap<T>, _interns: &Interns) -> bool {
        self.0 == other.0
    }

    /// Bytes don't contain nested heap references.
    fn py_dec_ref_ids(&mut self, _stack: &mut Vec<HeapId>) {
        // No-op: bytes don't hold Value references
    }

    fn py_bool<T: ResourceTracker>(&self, _heap: &Heap<T>, _interns: &Interns) -> bool {
        !self.0.is_empty()
    }

    fn py_repr_fmt<W: Write, T: ResourceTracker>(
        &self,
        f: &mut W,
        _heap: &Heap<T>,
        _heap_ids: &mut AHashSet<HeapId>,
        _interns: &Interns,
    ) -> std::fmt::Result {
        bytes_repr_fmt(&self.0, f)
    }
    // py_call_attr uses default implementation which returns AttributeError
}

/// Writes a CPython-compatible repr string for bytes to a formatter.
///
/// Format: `b'...'` or `b"..."` depending on content.
/// - Uses single quotes by default
/// - Switches to double quotes if bytes contain `'` but not `"`
/// - Escapes: `\\`, `\t`, `\n`, `\r`, `\xNN` for non-printable bytes
pub fn bytes_repr_fmt<W: Write>(bytes: &[u8], f: &mut W) -> std::fmt::Result {
    // Determine quote character: use double quotes if single quote present but not double
    let has_single = bytes.contains(&b'\'');
    let has_double = bytes.contains(&b'"');
    let quote = if has_single && !has_double { '"' } else { '\'' };

    f.write_char('b')?;
    f.write_char(quote)?;

    for &byte in bytes {
        match byte {
            b'\\' => f.write_str("\\\\")?,
            b'\t' => f.write_str("\\t")?,
            b'\n' => f.write_str("\\n")?,
            b'\r' => f.write_str("\\r")?,
            b'\'' if quote == '\'' => f.write_str("\\'")?,
            b'"' if quote == '"' => f.write_str("\\\"")?,
            // Printable ASCII (32-126)
            0x20..=0x7e => f.write_char(byte as char)?,
            // Non-printable: use \xNN format
            _ => write!(f, "\\x{byte:02x}")?,
        }
    }

    f.write_char(quote)
}

/// Returns a CPython-compatible repr string for bytes.
///
/// Convenience wrapper around `bytes_repr_fmt` that returns an owned String.
#[must_use]
pub fn bytes_repr(bytes: &[u8]) -> String {
    let mut result = String::new();
    // Writing to String never fails
    bytes_repr_fmt(bytes, &mut result).unwrap();
    result
}
