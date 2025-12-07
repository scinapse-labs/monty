/// Python string type, wrapping a Rust `String`.
///
/// This type provides Python string semantics. Currently supports basic
/// operations like length and equality comparison.
use std::borrow::Cow;

use crate::args::ArgValues;
use crate::exceptions::ExcType;
use crate::heap::{Heap, HeapData, HeapId};
use crate::run::RunResult;
use crate::value::{Attr, Value};
use crate::values::PyTrait;

/// Python string value stored on the heap.
///
/// Wraps a Rust `String` and provides Python-compatible operations.
/// `len()` returns the number of Unicode codepoints (characters), matching Python semantics.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Str(String);

impl Str {
    /// Creates a new Str from a Rust String.
    #[must_use]
    pub fn new(s: String) -> Self {
        Self(s)
    }

    /// Returns a reference to the inner string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns a mutable reference to the inner string.
    pub fn as_string_mut(&mut self) -> &mut String {
        &mut self.0
    }
}

impl From<String> for Str {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for Str {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<Str> for String {
    fn from(value: Str) -> Self {
        value.0
    }
}

impl std::ops::Deref for Str {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'c, 'e> PyTrait<'c, 'e> for Str {
    fn py_type(&self, _heap: &Heap<'c, 'e>) -> &'static str {
        "str"
    }

    fn py_len(&self, _heap: &Heap<'c, 'e>) -> Option<usize> {
        // Count Unicode characters, not bytes, to match Python semantics
        Some(self.0.chars().count())
    }

    fn py_eq(&self, other: &Self, _heap: &mut Heap<'c, 'e>) -> bool {
        self.0 == other.0
    }

    /// Strings don't contain nested heap references.
    fn py_dec_ref_ids(&mut self, _stack: &mut Vec<HeapId>) {
        // No-op: strings don't hold Value references
    }

    fn py_bool(&self, _heap: &Heap<'c, 'e>) -> bool {
        !self.0.is_empty()
    }

    fn py_repr<'a>(&'a self, _heap: &'a Heap<'c, 'e>) -> Cow<'a, str> {
        Cow::Owned(string_repr(&self.0))
    }

    fn py_str<'a>(&'a self, _heap: &'a Heap<'c, 'e>) -> Cow<'a, str> {
        self.0.as_str().into()
    }

    fn py_add(&self, other: &Self, heap: &mut Heap<'c, 'e>) -> Option<Value<'c, 'e>> {
        let result = format!("{}{}", self.0, other.0);
        let id = heap.allocate(HeapData::Str(result.into()));
        Some(Value::Ref(id))
    }

    fn py_iadd(&mut self, other: Value<'c, 'e>, heap: &mut Heap<'c, 'e>, self_id: Option<HeapId>) -> bool {
        match other {
            Value::Ref(other_id) => {
                if Some(other_id) == self_id {
                    let rhs = self.0.clone();
                    self.0.push_str(&rhs);
                    true
                } else if let HeapData::Str(rhs) = heap.get(other_id) {
                    self.0.push_str(rhs.as_str());
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn py_call_attr(
        &mut self,
        heap: &mut Heap<'c, 'e>,
        attr: &Attr,
        _args: ArgValues<'c, 'e>,
    ) -> RunResult<'c, Value<'c, 'e>> {
        Err(ExcType::attribute_error(self.py_type(heap), attr))
    }
}

/// Macro for common string escape replacements used in repr formatting.
///
/// Replaces backslash, newline, tab, and carriage return with their escaped forms.
macro_rules! string_replace_common {
    ($s:expr) => {
        $s.replace('\\', "\\\\")
            .replace('\n', "\\n")
            .replace('\t', "\\t")
            .replace('\r', "\\r")
    };
}

/// Returns a Python repr() string for a given string slice.
///
/// Chooses between single and double quotes based on the string content:
/// - Uses double quotes if the string contains single quotes but not double quotes
/// - Uses single quotes by default, escaping any contained single quotes
///
/// Common escape sequences (backslash, newline, tab, carriage return) are always escaped.
pub fn string_repr(s: &str) -> String {
    // Check if the string contains single quotes but not double quotes
    if s.contains('\'') && !s.contains('"') {
        // Use double quotes if string contains only single quotes
        format!("\"{}\"", string_replace_common!(s))
    } else {
        // Use single quotes by default, escape any single quotes in the string
        format!("'{}'", string_replace_common!(s.replace('\'', "\\'")))
    }
}
