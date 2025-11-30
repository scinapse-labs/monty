use std::borrow::Cow;
use std::fmt::Write;

use indexmap::IndexMap;

use crate::args::Args;
use crate::exceptions::ExcType;
use crate::heap::{Heap, HeapData, ObjectId};
use crate::object::{Attr, Object};
use crate::run::RunResult;
use crate::values::PyValue;

/// Python dict type, wrapping an IndexMap to preserve insertion order.
///
/// This type provides Python dict semantics including dynamic key-value storage,
/// reference counting for heap objects, and standard dict methods like get, keys,
/// values, items, and pop.
///
/// # Storage Strategy
/// Uses `IndexMap<u64, Vec<(Object, Object)>>` to preserve insertion order (matching
/// Python 3.7+ behavior). The key is the hash of the dict key. The Vec handles hash
/// collisions by storing multiple (key, value) pairs with the same hash, allowing
/// proper equality checking for collisions.
///
/// # Reference Counting
/// When objects are added via `set()`, their reference counts are incremented.
/// When using `from_pairs()`, ownership is transferred without incrementing refcounts
/// (caller must ensure objects' refcounts account for the dict's reference).
#[derive(Debug, PartialEq, Default)]
pub struct Dict {
    /// Maps hash -> list of (key, value) pairs with that hash
    /// The Vec handles hash collisions. IndexMap preserves insertion order.
    map: IndexMap<u64, Vec<(Object, Object)>>,
}

impl Dict {
    /// Creates a new empty dict.
    #[must_use]
    pub fn new() -> Self {
        Self { map: IndexMap::new() }
    }

    /// Creates a dict from a vector of (key, value) pairs.
    ///
    /// Assumes the caller is transferring ownership of all keys and values in the pairs.
    /// Does NOT increment reference counts since ownership is being transferred.
    /// Returns Err if any key is unhashable (e.g., list, dict).
    pub fn from_pairs(pairs: Vec<(Object, Object)>, heap: &mut Heap) -> RunResult<'static, Self> {
        let mut dict = Self::new();
        for (key, value) in pairs {
            dict.set_transfer_ownership(key, value, heap)?;
        }
        Ok(dict)
    }

    /// Internal method to set a key-value pair without incrementing refcounts.
    ///
    /// Used when ownership is being transferred (e.g., from_pairs) rather than shared.
    /// The caller must ensure the objects' refcounts already account for this dict's reference.
    fn set_transfer_ownership(
        &mut self,
        key: Object,
        value: Object,
        heap: &mut Heap,
    ) -> RunResult<'static, Option<Object>> {
        let hash = key
            .py_hash_u64(heap)
            .ok_or_else(|| ExcType::type_error_unhashable(key.py_type(heap)))?;

        let bucket = self.map.entry(hash).or_default();

        // Check if key already exists in bucket
        for (i, (k, _v)) in bucket.iter().enumerate() {
            if k.py_eq(&key, heap) {
                // Key exists, replace in place to preserve insertion order
                // Note: we don't decrement old value's refcount since this is a transfer
                // and we don't increment new value's refcount either
                let (_old_key, old_value) = std::mem::replace(&mut bucket[i], (key, value));
                return Ok(Some(old_value));
            }
        }

        // Key doesn't exist, add new pair
        bucket.push((key, value));
        Ok(None)
    }

    /// Gets a value from the dict by key.
    ///
    /// Returns Ok(Some(value)) if key exists, Ok(None) if key doesn't exist.
    /// Returns Err if key is unhashable.
    pub fn get(&self, key: &Object, heap: &mut Heap) -> RunResult<'static, Option<&Object>> {
        let hash = key
            .py_hash_u64(heap)
            .ok_or_else(|| ExcType::type_error_unhashable(key.py_type(heap)))?;
        if let Some(bucket) = self.map.get(&hash) {
            for (k, v) in bucket {
                if k.py_eq(key, heap) {
                    return Ok(Some(v));
                }
            }
        }
        Ok(None)
    }

    /// Sets a key-value pair in the dict.
    ///
    /// The caller transfers ownership of `key` and `value` to the dict. Their refcounts
    /// are NOT incremented here - the caller is responsible for ensuring the refcounts
    /// were already incremented (e.g., via `clone_with_heap` or `evaluate_use`).
    ///
    /// If the key already exists, replaces the old value and returns it (caller now
    /// owns the old value and is responsible for its refcount).
    /// Returns Err if key is unhashable.
    pub fn set(&mut self, key: Object, value: Object, heap: &mut Heap) -> RunResult<'static, Option<Object>> {
        let hash = key
            .py_hash_u64(heap)
            .ok_or_else(|| ExcType::type_error_unhashable(key.py_type(heap)))?;

        let bucket = self.map.entry(hash).or_default();

        // Check if key already exists in bucket
        for (i, (k, _v)) in bucket.iter().enumerate() {
            if k.py_eq(&key, heap) {
                // Key exists, replace in place to preserve insertion order within the bucket
                let (old_key, old_value) = std::mem::replace(&mut bucket[i], (key, value));

                // Decrement refcount for old key (we're discarding it)
                old_key.drop_with_heap(heap);
                // Transfer ownership of old_value to caller (no clone needed)
                return Ok(Some(old_value));
            }
        }

        // Key doesn't exist, add new pair (ownership transfer)
        bucket.push((key, value));
        Ok(None)
    }

    /// Removes and returns a key-value pair from the dict.
    ///
    /// Returns Ok(Some((key, value))) if key exists, Ok(None) if key doesn't exist.
    /// Returns Err if key is unhashable.
    ///
    /// Reference counting: does not decrement refcounts for removed key and value;
    /// caller assumes ownership and is responsible for managing their refcounts.
    pub fn pop(&mut self, key: &Object, heap: &mut Heap) -> RunResult<'static, Option<(Object, Object)>> {
        let hash = key
            .py_hash_u64(heap)
            .ok_or_else(|| ExcType::type_error_unhashable(key.py_type(heap)))?;

        if let Some(bucket) = self.map.get_mut(&hash) {
            for (i, (k, _v)) in bucket.iter().enumerate() {
                if k.py_eq(key, heap) {
                    let (old_key, old_value) = bucket.swap_remove(i);
                    if bucket.is_empty() {
                        self.map.shift_remove(&hash);
                    }
                    // Don't decrement refcounts - caller now owns the objects
                    return Ok(Some((old_key, old_value)));
                }
            }
        }
        Ok(None)
    }

    /// Returns a vector of all keys in the dict with proper reference counting.
    ///
    /// Each key's reference count is incremented since the returned vector
    /// now holds additional references to these objects.
    #[must_use]
    pub fn keys(&self, heap: &mut Heap) -> Vec<Object> {
        let mut result = Vec::new();
        for bucket in self.map.values() {
            for (k, _v) in bucket {
                result.push(k.clone_with_heap(heap));
            }
        }
        result
    }

    /// Returns a vector of all values in the dict with proper reference counting.
    ///
    /// Each value's reference count is incremented since the returned vector
    /// now holds additional references to these objects.
    #[must_use]
    pub fn values(&self, heap: &mut Heap) -> Vec<Object> {
        let mut result = Vec::new();
        for bucket in self.map.values() {
            for (_k, v) in bucket {
                result.push(v.clone_with_heap(heap));
            }
        }
        result
    }

    /// Returns a vector of all (key, value) pairs in the dict with proper reference counting.
    ///
    /// Each key and value's reference count is incremented since the returned vector
    /// now holds additional references to these objects.
    #[must_use]
    pub fn items(&self, heap: &mut Heap) -> Vec<(Object, Object)> {
        let mut result = Vec::new();
        for bucket in self.map.values() {
            for (k, v) in bucket {
                result.push((k.clone_with_heap(heap), v.clone_with_heap(heap)));
            }
        }
        result
    }

    /// Returns the number of key-value pairs in the dict.
    #[must_use]
    pub fn len(&self) -> usize {
        self.map.values().map(Vec::len).sum()
    }

    /// Returns true if the dict is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Creates a deep clone of this dict with proper reference counting.
    ///
    /// All heap-allocated keys and values have their reference counts
    /// incremented. This should be used instead of `.clone()` which would
    /// bypass reference counting.
    #[must_use]
    pub fn clone_with_heap(&self, heap: &mut Heap) -> Self {
        let mut new_map = IndexMap::new();
        for (hash, bucket) in &self.map {
            let new_bucket: Vec<(Object, Object)> = bucket
                .iter()
                .map(|(k, v)| (k.clone_with_heap(heap), v.clone_with_heap(heap)))
                .collect();
            new_map.insert(*hash, new_bucket);
        }
        Self { map: new_map }
    }
}

impl PyValue for Dict {
    fn py_type(&self, _heap: &Heap) -> &'static str {
        "dict"
    }

    fn py_len(&self, _heap: &Heap) -> Option<usize> {
        Some(self.len())
    }

    fn py_eq(&self, other: &Self, heap: &mut Heap) -> bool {
        if self.len() != other.len() {
            return false;
        }

        // Check that all keys in self exist in other with equal values
        for bucket in self.map.values() {
            for (k, v) in bucket {
                match other.get(k, heap) {
                    Ok(Some(other_v)) => {
                        if !v.py_eq(other_v, heap) {
                            return false;
                        }
                    }
                    _ => return false,
                }
            }
        }
        true
    }

    fn py_dec_ref_ids(&self, stack: &mut Vec<ObjectId>) {
        for bucket in self.map.values() {
            for (k, v) in bucket {
                if let Object::Ref(id) = k {
                    stack.push(*id);
                }
                if let Object::Ref(id) = v {
                    stack.push(*id);
                }
            }
        }
    }

    fn py_bool(&self, _heap: &Heap) -> bool {
        !self.is_empty()
    }

    fn py_repr<'h>(&'h self, heap: &'h Heap) -> Cow<'h, str> {
        if self.is_empty() {
            return Cow::Borrowed("{}");
        }

        let mut s = String::from("{");
        let mut first = true;
        for bucket in self.map.values() {
            for (k, v) in bucket {
                if !first {
                    s.push_str(", ");
                }
                first = false;
                let key_repr = k.py_repr(heap);
                let val_repr = v.py_repr(heap);
                let _ = write!(s, "{key_repr}: {val_repr}");
            }
        }
        s.push('}');
        Cow::Owned(s)
    }

    fn py_getitem(&self, key: &Object, heap: &mut Heap) -> RunResult<'static, Object> {
        // Use copy_for_extend to avoid borrow conflict, then increment refcount
        let result = self.get(key, heap)?.map(Object::copy_for_extend);
        match result {
            Some(value) => {
                if let Object::Ref(id) = &value {
                    heap.inc_ref(*id);
                }
                Ok(value)
            }
            None => Err(ExcType::key_error(key, heap)),
        }
    }

    fn py_setitem(&mut self, key: Object, value: Object, heap: &mut Heap) -> RunResult<'static, ()> {
        // Drop the old value if one was replaced
        if let Some(old_value) = self.set(key, value, heap)? {
            old_value.drop_with_heap(heap);
        }
        Ok(())
    }

    fn py_call_attr<'c>(&mut self, heap: &mut Heap, attr: &Attr, args: Args) -> RunResult<'c, Object> {
        match attr {
            Attr::Get => {
                let (key, opt_default) = args.get_one_two_args("get")?;
                // Use copy_for_extend to avoid borrow conflict, then increment refcount
                let result = self.get(&key, heap)?.map(Object::copy_for_extend);
                match result {
                    Some(value) => {
                        if let Object::Ref(id) = &value {
                            heap.inc_ref(*id);
                        }
                        Ok(value)
                    }
                    None => {
                        // Return default if provided, else None
                        if let Some(default) = opt_default {
                            Ok(default.clone_with_heap(heap))
                        } else {
                            Ok(Object::None)
                        }
                    }
                }
            }
            Attr::Keys => {
                args.check_zero_args("dict.keys")?;
                // keys() now handles refcount incrementing
                let keys = self.keys(heap);
                let list_id = heap.allocate(HeapData::List(crate::values::List::new(keys)));
                Ok(Object::Ref(list_id))
            }
            Attr::Values => {
                args.check_zero_args("dict.values")?;
                // values() now handles refcount incrementing
                let values = self.values(heap);
                let list_id = heap.allocate(HeapData::List(crate::values::List::new(values)));
                Ok(Object::Ref(list_id))
            }
            Attr::Items => {
                args.check_zero_args("dict.items")?;
                // items() now handles refcount incrementing for the returned objects
                let items = self.items(heap);
                // Convert to list of tuples
                let mut tuples = Vec::new();
                for (k, v) in items {
                    let tuple_id = heap.allocate(HeapData::Tuple(crate::values::Tuple::from_vec(vec![k, v])));
                    tuples.push(Object::Ref(tuple_id));
                }
                let list_id = heap.allocate(HeapData::List(crate::values::List::new(tuples)));
                Ok(Object::Ref(list_id))
            }
            Attr::Pop => {
                let (key, opt_default) = args.get_one_two_args("pop")?;
                match self.pop(&key, heap)? {
                    Some((k, v)) => {
                        // Decrement key refcount since we're not returning it
                        k.drop_with_heap(heap);
                        Ok(v)
                    }
                    None => {
                        // Return default if provided, else KeyError
                        if let Some(default) = opt_default {
                            Ok(default.clone_with_heap(heap))
                        } else {
                            Err(ExcType::key_error(&key, heap))
                        }
                    }
                }
            }
            // Catch-all for unsupported attributes (including list methods like Append, Insert)
            _ => Err(ExcType::attribute_error("dict", attr)),
        }
    }
}
