/// Trait for heap-allocated Python values that need common operations.
///
/// This trait abstracts over container types (List, Tuple, Str, Bytes) stored
/// in the heap, providing a unified interface for operations like length,
/// equality, reference counting support, and attribute dispatch.
///
/// The trait is designed to work with `enum_dispatch` for efficient virtual
/// dispatch on `HeapData` without boxing overhead.
use std::borrow::Cow;

use crate::args::Args;
use crate::exceptions::ExcType;
use crate::heap::{Heap, ObjectId};
use crate::object::{Attr, Object};
use crate::run::RunResult;

/// Common operations for heap-allocated Python values.
///
/// Implementers should provide Python-compatible semantics for all operations.
/// Most methods take a `&Heap` reference to allow nested lookups for containers
/// holding `Object::Ref` values.
///
/// This trait is used with `enum_dispatch` on `HeapData` to enable efficient
/// virtual dispatch without boxing overhead.
pub trait PyValue {
    /// Returns the Python type name for this value (e.g., "list", "str").
    ///
    /// Used for error messages and the `type()` builtin.
    /// Takes heap reference for cases where nested Object lookups are needed.
    fn py_type(&self, heap: &Heap) -> &'static str;

    /// Returns the number of elements in this container.
    ///
    /// For strings, returns the number of bytes (not Unicode codepoints).
    /// Returns `None` if the type doesn't support `len()`.
    fn py_len(&self, heap: &Heap) -> Option<usize>;

    /// Python equality comparison (`==`).
    ///
    /// For containers, this performs element-wise comparison using the heap
    /// to resolve nested references. Takes `&mut Heap` to allow lazy hash
    /// computation for dict key lookups.
    fn py_eq(&self, other: &Self, heap: &mut Heap) -> bool;

    /// Pushes any contained `ObjectId`s onto the stack for reference counting.
    ///
    /// This is called during `dec_ref` to find nested heap references that
    /// need their refcounts decremented when this object is freed.
    fn py_dec_ref_ids(&self, stack: &mut Vec<ObjectId>);

    /// Returns the truthiness of the value following Python semantics.
    ///
    /// Container types should typically report `false` when empty.
    fn py_bool(&self, heap: &Heap) -> bool {
        self.py_len(heap) != Some(0)
    }

    /// Returns the Python `repr()` string for this value.
    fn py_repr<'h>(&'h self, _heap: &'h Heap) -> Cow<'h, str>;

    /// Returns the Python `str()` string for this value.
    fn py_str<'h>(&'h self, heap: &'h Heap) -> Cow<'h, str> {
        self.py_repr(heap)
    }

    /// Python addition (`__add__`).
    fn py_add(&self, _other: &Self, _heap: &mut Heap) -> Option<Object> {
        None
    }

    /// Python subtraction (`__sub__`).
    fn py_sub(&self, _other: &Self, _heap: &mut Heap) -> Option<Object> {
        None
    }

    /// Python modulus (`__mod__`).
    fn py_mod(&self, _other: &Self) -> Option<Object> {
        None
    }

    /// Optimized helper for `(a % b) == c` comparisons.
    fn py_mod_eq(&self, _other: &Self, _right_value: i64) -> Option<bool> {
        None
    }

    /// Python in-place addition (`__iadd__`).
    fn py_iadd(&mut self, other: Object, _heap: &mut Heap, _self_id: Option<ObjectId>) -> Result<(), Object> {
        Err(other)
    }

    /// Calls an attribute method on this value (e.g., `list.append()`).
    ///
    /// Returns an error if the attribute doesn't exist or the arguments are invalid.
    fn py_call_attr<'c>(&mut self, heap: &mut Heap, attr: &Attr, _args: Args) -> RunResult<'c, Object> {
        Err(ExcType::attribute_error(self.py_type(heap), attr))
    }

    /// Python subscript get operation (`__getitem__`), e.g., `d[key]`.
    ///
    /// Returns the value associated with the key, or an error if the key doesn't exist
    /// or the type doesn't support subscripting.
    ///
    /// The `&mut Heap` parameter is needed for proper reference counting when cloning
    /// the returned value.
    ///
    /// Default implementation returns TypeError.
    fn py_getitem(&self, _key: &Object, heap: &mut Heap) -> RunResult<'static, Object> {
        Err(ExcType::type_error_not_sub(self.py_type(heap)))
    }

    /// Python subscript set operation (`__setitem__`), e.g., `d[key] = value`.
    ///
    /// Sets the value associated with the key, or returns an error if the key is invalid
    /// or the type doesn't support subscript assignment.
    ///
    /// Default implementation returns TypeError.
    fn py_setitem(&mut self, _key: Object, _value: Object, heap: &mut Heap) -> RunResult<'static, ()> {
        Err(ExcType::TypeError).map_err(|e| {
            crate::exceptions::exc_fmt!(e; "'{}' object does not support item assignment", self.py_type(heap)).into()
        })
    }
}
