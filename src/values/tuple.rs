/// Python tuple type, wrapping a `Vec<Object>`.
///
/// This type provides Python tuple semantics. Tuples are immutable sequences
/// that can contain any Python object. Like lists, tuples properly handle
/// reference counting for heap-allocated objects.
use std::borrow::Cow;

use crate::args::Args;
use crate::exceptions::ExcType;
use crate::heap::{Heap, ObjectId};
use crate::object::{Attr, Object};
use crate::run::RunResult;
use crate::values::list::repr_sequence;
use crate::values::PyValue;

/// Python tuple value stored on the heap.
///
/// Wraps a `Vec<Object>` and provides Python-compatible tuple operations.
/// Unlike lists, tuples are conceptually immutable (though this is not
/// enforced at the type level for internal operations).
///
/// # Reference Counting
/// When a tuple is freed, all contained heap references have their refcounts
/// decremented via `push_stack_ids`.
#[derive(Debug, PartialEq, Default)]
pub struct Tuple(Vec<Object>);

impl Tuple {
    /// Creates a new tuple from a vector of objects.
    ///
    /// Note: This does NOT increment reference counts - the caller must
    /// ensure refcounts are properly managed.
    #[must_use]
    pub fn from_vec(vec: Vec<Object>) -> Self {
        Self(vec)
    }

    /// Returns a reference to the underlying vector.
    #[must_use]
    pub fn as_vec(&self) -> &Vec<Object> {
        &self.0
    }

    /// Creates a deep clone of this tuple with proper reference counting.
    ///
    /// All heap-allocated objects in the tuple have their reference counts
    /// incremented. This should be used instead of `.clone()` which would
    /// bypass reference counting.
    #[must_use]
    pub fn clone_with_heap(&self, heap: &mut Heap) -> Self {
        let cloned: Vec<Object> = self.0.iter().map(|obj| obj.clone_with_heap(heap)).collect();
        Self(cloned)
    }
}

impl From<Vec<Object>> for Tuple {
    fn from(vec: Vec<Object>) -> Self {
        Self(vec)
    }
}

impl std::iter::FromIterator<Object> for Tuple {
    fn from_iter<I: IntoIterator<Item = Object>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl From<Tuple> for Vec<Object> {
    fn from(tuple: Tuple) -> Self {
        tuple.0
    }
}

impl PyValue for Tuple {
    fn py_type(&self, _heap: &Heap) -> &'static str {
        "tuple"
    }

    fn py_len(&self, _heap: &Heap) -> Option<usize> {
        Some(self.0.len())
    }

    fn py_getitem(&self, key: &Object, heap: &mut Heap) -> RunResult<'static, Object> {
        // Extract integer index from key, returning TypeError if not an int
        let index = match key {
            Object::Int(i) => *i,
            _ => return Err(ExcType::type_error_indices("tuple", key.py_type(heap))),
        };

        // Convert to usize, handling negative indices (Python-style: -1 = last element)
        let len = self.0.len() as i64;
        let normalized_index = if index < 0 { index + len } else { index };

        // Bounds check
        if normalized_index < 0 || normalized_index >= len {
            return Err(ExcType::tuple_index_error());
        }

        // Return clone of the item with proper refcount increment
        Ok(self.0[normalized_index as usize].clone_with_heap(heap))
    }

    fn py_eq(&self, other: &Self, heap: &mut Heap) -> bool {
        if self.0.len() != other.0.len() {
            return false;
        }
        for (i1, i2) in self.0.iter().zip(&other.0) {
            if !i1.py_eq(i2, heap) {
                return false;
            }
        }
        true
    }

    /// Pushes all heap object IDs contained in this tuple onto the stack.
    ///
    /// Called during garbage collection to decrement refcounts of nested objects.
    fn py_dec_ref_ids(&self, stack: &mut Vec<ObjectId>) {
        for obj in &self.0 {
            if let Object::Ref(id) = obj {
                stack.push(*id);
            }
        }
    }

    /// Tuples don't support attribute calls.
    fn py_call_attr<'c>(&mut self, heap: &mut Heap, attr: &Attr, _args: Args) -> RunResult<'c, Object> {
        Err(ExcType::attribute_error(self.py_type(heap), attr))
    }

    fn py_bool(&self, _heap: &Heap) -> bool {
        !self.0.is_empty()
    }

    fn py_repr<'h>(&'h self, heap: &'h Heap) -> Cow<'h, str> {
        Cow::Owned(repr_sequence('(', ')', &self.0, heap))
    }
}
