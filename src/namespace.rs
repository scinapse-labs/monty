use crate::exceptions::{ExcType, SimpleException};
use crate::expressions::{Identifier, NameScope};
use crate::heap::{Heap, HeapId};
use crate::intern::Interns;
use crate::resource::ResourceTracker;
use crate::run_frame::RunResult;
use crate::value::Value;

/// Unique identifier for values stored inside the namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NamespaceId(u32);

impl NamespaceId {
    pub fn new(index: usize) -> Self {
        NamespaceId(index.try_into().expect("Invalid namespace id"))
    }

    /// Returns the raw index value.
    #[inline]
    fn index(self) -> usize {
        self.0 as usize
    }
}

/// Index for the global (module-level) namespace in Namespaces.
/// At module level, local_idx == GLOBAL_NS_IDX (same namespace).
pub const GLOBAL_NS_IDX: NamespaceId = NamespaceId(0);

#[derive(Debug)]
pub struct Namespace(Vec<Value>);

impl Namespace {
    pub fn get(&self, index: NamespaceId) -> &Value {
        &self.0[index.index()]
    }

    pub fn get_opt(&self, index: NamespaceId) -> Option<&Value> {
        self.0.get(index.index())
    }

    pub fn get_mut(&mut self, index: NamespaceId) -> &mut Value {
        &mut self.0[index.index()]
    }

    pub fn set(&mut self, index: NamespaceId, value: Value) {
        self.0[index.index()] = value;
    }

    pub fn iter(&self) -> impl Iterator<Item = &Value> {
        self.0.iter()
    }
}

impl IntoIterator for Namespace {
    type Item = Value;
    type IntoIter = std::vec::IntoIter<Value>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

/// Storage for all namespaces during execution.
///
/// This struct owns all namespace data, allowing safe mutable access through indices.
/// Index 0 is always the global (module-level) namespace.
///
/// # Design Rationale
///
/// Instead of using raw pointers to share namespace access between frames,
/// we use indices into this central namespaces. Since variable scope (Local vs Global)
/// is known at compile time, we only ever need one mutable reference at a time.
///
/// # Closure Support
///
/// Variables captured by closures are stored in cells on the heap, not in namespaces.
/// The `get_var_value` method handles both namespace-based and cell-based variable access.
#[derive(Debug)]
pub struct Namespaces {
    namespaces: Vec<Namespace>,
}

impl Namespaces {
    /// Creates namespaces with the global namespace initialized.
    ///
    /// The global namespace is always at index 0.
    pub fn new(namespace: Vec<Value>) -> Self {
        Self {
            namespaces: vec![Namespace(namespace)],
        }
    }

    /// Gets an immutable slice reference to a namespace by index.
    ///
    /// Used for reading from the enclosing namespace when defining closures,
    /// without requiring mutable access.
    ///
    /// # Panics
    /// Panics if `idx` is out of bounds.
    pub fn get(&self, idx: NamespaceId) -> &Namespace {
        &self.namespaces[idx.index()]
    }

    /// Gets a mutable slice reference to a namespace by index.
    ///
    /// # Panics
    /// Panics if `idx` is out of bounds.
    pub fn get_mut(&mut self, idx: NamespaceId) -> &mut Namespace {
        &mut self.namespaces[idx.index()]
    }

    /// Creates a new namespace for a function call, returns its index.
    ///
    /// The new namespace is initialized with `Object::Undefined` values.
    /// Call `pop_with_heap()` when the function returns to clean up.
    pub fn push(&mut self, namespace: Vec<Value>) -> NamespaceId {
        let idx = NamespaceId(self.namespaces.len().try_into().expect("NamespaceId overflow"));
        self.namespaces.push(Namespace(namespace));
        idx
    }

    /// Removes the most recently added namespace (after function returns),
    /// properly cleaning up any heap-allocated values.
    ///
    /// This method decrements reference counts for any `Value::Ref` entries
    /// in the namespace before removing it.
    ///
    /// # Panics
    /// Panics if attempting to pop the global namespace (index 0).
    pub fn pop_with_heap<T: ResourceTracker>(&mut self, heap: &mut Heap<T>) {
        debug_assert!(self.namespaces.len() > 1, "cannot pop global namespace");
        if let Some(namespace) = self.namespaces.pop() {
            for value in namespace.0 {
                value.drop_with_heap(heap);
            }
        }
    }

    /// Cleans up the global namespace by dropping all values with proper ref counting.
    ///
    /// Call this before the namespaces is dropped to properly decrement reference counts
    /// for any `Value::Ref` entries in the global namespace.
    ///
    /// Only needed when `dec-ref-check` is enabled, since the Drop impl panics on unfreed Refs.
    #[cfg(feature = "dec-ref-check")]
    pub fn drop_global_with_heap<T: ResourceTracker>(&mut self, heap: &mut Heap<T>) {
        let global = self.get_mut(GLOBAL_NS_IDX);
        for value in &mut global.0 {
            let v = std::mem::replace(value, Value::Undefined);
            v.drop_with_heap(heap);
        }
    }

    /// Looks up a variable by name in the appropriate namespace based on the scope index for mutation.
    ///
    /// # Arguments
    /// * `local_idx` - Index of the local namespace in namespaces
    /// * `ident` - The identifier to look up (contains heap_id and scope)
    /// * `interns` - String storage for looking up variable names in error messages
    ///
    /// # Returns
    /// A mutable reference to the Value at the identifier's location, or NameError if undefined.
    pub fn get_var_mut(
        &mut self,
        local_idx: NamespaceId,
        ident: &Identifier,
        interns: &Interns,
    ) -> RunResult<&mut Value> {
        let ns_idx = match ident.scope {
            NameScope::Local => local_idx,
            NameScope::Global => GLOBAL_NS_IDX,
            NameScope::Cell => {
                // Cell access should use get_var_value which handles cell dereferencing
                panic!("Cell access should use get_var_value, not get_var_mut");
            }
        };
        let namespace = self.get_mut(ns_idx);

        if let Some(value) = namespace.0.get_mut(ident.namespace_id().index()) {
            if !matches!(value, Value::Undefined) {
                return Ok(value);
            }
        }
        Err(
            SimpleException::new(ExcType::NameError, Some(interns.get_str(ident.name_id).to_string()))
                .with_position(ident.position)
                .into(),
        )
    }

    /// Looks up a variable by name in the appropriate namespace based on the scope index.
    ///
    /// # Arguments
    /// * `local_idx` - Index of the local namespace in namespaces
    /// * `ident` - The identifier to look up (contains heap_id and scope)
    /// * `interns` - String storage for looking up variable names in error messages
    ///
    /// # Returns
    /// An immutable reference to the Value at the identifier's location, or NameError if undefined.
    pub fn get_var(&self, local_idx: NamespaceId, ident: &Identifier, interns: &Interns) -> RunResult<&Value> {
        let ns_idx = match ident.scope {
            NameScope::Local => local_idx,
            NameScope::Global => GLOBAL_NS_IDX,
            NameScope::Cell => {
                // Cell access should use get_var_value which handles cell dereferencing
                panic!("Cell access should use get_var_value, not get_var_mut");
            }
        };
        let namespace = self.get(ns_idx);

        if let Some(value) = namespace.0.get(ident.namespace_id().index()) {
            if !matches!(value, Value::Undefined) {
                return Ok(value);
            }
        }
        Err(
            SimpleException::new(ExcType::NameError, Some(interns.get_str(ident.name_id).to_string()))
                .with_position(ident.position)
                .into(),
        )
    }

    /// Gets a variable's value, handling Local, Global, and Cell scopes.
    ///
    /// This is the primary method for reading variable values during expression evaluation.
    /// It handles all scope types:
    /// - `Local` - reads directly from the local namespace
    /// - `Global` - reads directly from the global namespace (index 0)
    /// - `Cell` - namespace slot contains `Value::Ref(cell_id)`, reads through the cell
    ///
    /// # Arguments
    /// * `local_idx` - Index of the local namespace in namespaces
    /// * `heap` - The heap for cell access and cloning ref-counted values
    /// * `ident` - The identifier to look up (contains heap_id and scope)
    /// * `interns` - String storage for looking up variable names in error messages
    ///
    /// # Returns
    /// A cloned copy of the value (with refcount incremented for Ref values), or NameError if undefined.
    pub fn get_var_value<T: ResourceTracker>(
        &self,
        local_idx: NamespaceId,
        heap: &mut Heap<T>,
        ident: &Identifier,
        interns: &Interns,
    ) -> RunResult<Value> {
        // Determine which namespace to use
        let ns_idx = match ident.scope {
            NameScope::Global => GLOBAL_NS_IDX,
            _ => local_idx, // Local and Cell both use local namespace
        };

        match ident.scope {
            NameScope::Cell => {
                // Cell access - namespace slot contains Value::Ref(cell_id)
                let namespace = &self.namespaces[ns_idx.index()];
                if let Value::Ref(cell_id) = namespace.get(ident.namespace_id()) {
                    let value = heap.get_cell_value(*cell_id);
                    // Cell may be undefined if accessed before assignment in enclosing scope
                    if matches!(value, Value::Undefined) {
                        let name = interns.get_str(ident.name_id);
                        Err(ExcType::name_error_free_variable(name).into())
                    } else {
                        Ok(value)
                    }
                } else {
                    panic!("Cell variable slot doesn't contain a cell reference - prepare-time bug");
                }
            }
            _ => {
                // Local or Global scope - direct namespace access
                self.get_var(ns_idx, ident, interns)
                    .map(|object| object.clone_with_heap(heap))
            }
        }
    }

    /// Returns the global namespace for final inspection (e.g., ref-count testing).
    ///
    /// Consumes the namespaces since the namespace Vec is moved out.
    ///
    /// Only available when the `ref-counting` feature is enabled.
    #[cfg(feature = "ref-counting")]
    pub fn into_global(mut self) -> Namespace {
        self.namespaces.swap_remove(GLOBAL_NS_IDX.index())
    }

    /// Returns an iterator over all HeapIds referenced by values in all namespaces.
    ///
    /// This is used by garbage collection to find all root references. Any heap
    /// object reachable from these roots should not be collected.
    pub fn iter_heap_ids(&self) -> impl Iterator<Item = HeapId> + '_ {
        self.namespaces.iter().flat_map(|namespace| {
            namespace
                .iter()
                .filter_map(|value| if let Value::Ref(id) = value { Some(*id) } else { None })
        })
    }
}
