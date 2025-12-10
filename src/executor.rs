use crate::exceptions::InternalRunError;
use crate::exceptions::RunError;
use crate::expressions::Node;

use crate::heap::Heap;
use crate::intern::Interns;
use crate::namespace::Namespaces;
use crate::object::PyObject;
use crate::parse::parse;
use crate::parse_error::ParseError;
use crate::prepare::prepare;
use crate::resource::NoLimitTracker;
use crate::resource::{LimitedTracker, ResourceLimits, ResourceTracker};
use crate::run_frame::RunFrame;
use crate::value::Value;

/// Main executor that parses and runs Python code.
///
/// The executor stores the compiled AST.
#[derive(Debug)]
pub struct Executor {
    namespace_size: usize,
    /// Maps variable names to their indices in the namespace. Used for ref-count testing.
    #[cfg(feature = "ref-counting")]
    name_map: ahash::AHashMap<String, crate::namespace::NamespaceId>,
    nodes: Vec<Node>,
    /// Interned strings used for looking up names and filenames during execution.
    interns: Interns,
}

impl Executor {
    /// Creates a new executor with the given code, filename, and input names.
    ///
    /// # Arguments
    /// * `code` - The Python code to execute.
    /// * `filename` - The filename of the Python code.
    /// * `input_names` - The names of the input variables.
    ///
    /// # Returns
    /// A new `Executor` instance which can be used to execute the code.
    pub fn new(code: &str, filename: &str, input_names: &[&str]) -> Result<Self, ParseError> {
        let parse_result = parse(code, filename)?;
        let prepared = prepare(parse_result, input_names)?;
        Ok(Self {
            namespace_size: prepared.namespace_size,
            #[cfg(feature = "ref-counting")]
            name_map: prepared.name_map,
            nodes: prepared.nodes,
            interns: Interns::new(prepared.interner, prepared.functions),
        })
    }

    /// Executes the code with the given input values.
    ///
    /// # Arguments
    /// * `inputs` - Values to fill the first N slots of the namespace (e.g., function parameters)
    ///
    /// # Example
    /// ```
    /// use std::time::Duration;
    /// use monty::Executor;
    ///
    /// let ex = Executor::new("1 + 2", "test.py", &[]).unwrap();
    /// let py_object = ex.run_no_limits(vec![]).unwrap();
    /// assert_eq!(py_object, monty::PyObject::Int(3));
    /// ```
    pub fn run_no_limits(&self, inputs: Vec<PyObject>) -> Result<PyObject, RunError> {
        self.run_with_tracker(inputs, NoLimitTracker::default())
    }

    /// Executes the code with configurable resource limits.
    ///
    /// # Arguments
    /// * `inputs` - Values to fill the first N slots of the namespace
    /// * `limits` - Resource limits to enforce during execution
    ///
    /// # Example
    /// ```
    /// use std::time::Duration;
    /// use monty::{Executor, ResourceLimits, PyObject};
    ///
    /// let limits = ResourceLimits::new()
    ///     .max_allocations(1000)
    ///     .max_duration(Duration::from_secs(5));
    /// let ex = Executor::new("1 + 2", "test.py", &[]).unwrap();
    /// let py_object = ex.run_with_limits(vec![], limits).unwrap();
    /// assert_eq!(py_object, PyObject::Int(3));
    /// ```
    pub fn run_with_limits(&self, inputs: Vec<PyObject>, limits: ResourceLimits) -> Result<PyObject, RunError> {
        let tracker = LimitedTracker::new(limits);
        self.run_with_tracker(inputs, tracker)
    }

    /// Executes the code with a custom resource tracker.
    ///
    /// This provides full control over resource tracking and garbage collection
    /// scheduling. The tracker is called on each allocation and periodically
    /// during execution to check time limits and trigger GC.
    ///
    /// # Arguments
    /// * `inputs` - Values to fill the first N slots of the namespace
    /// * `tracker` - Custom resource tracker implementation
    ///
    /// # Type Parameters
    /// * `T` - A type implementing `ResourceTracker`
    fn run_with_tracker<T: ResourceTracker>(&self, inputs: Vec<PyObject>, tracker: T) -> Result<PyObject, RunError> {
        let mut heap = Heap::new(self.namespace_size, tracker);
        let mut namespaces = self.prepare_namespaces(inputs, &mut heap, &self.interns)?;

        let frame = RunFrame::module_frame(&self.interns);
        let result = frame.execute(&mut namespaces, &mut heap, &self.nodes);

        // Clean up the global namespace before returning (only needed with dec-ref-check)
        #[cfg(feature = "dec-ref-check")]
        namespaces.drop_global_with_heap(&mut heap);

        result.map(|frame_exit| PyObject::new(frame_exit, &mut heap, &self.interns))
    }

    /// Executes the code and returns both the result and reference count data.
    ///
    /// This is used for testing reference counting behavior. Returns:
    /// - The execution result (`Exit`)
    /// - Reference count data as a tuple of:
    ///   - A map from variable names to their reference counts (only for heap-allocated values)
    ///   - The number of unique heap value IDs referenced by variables
    ///   - The total number of live heap values
    ///
    /// For strict matching validation, compare unique_refs_count with heap_entry_count.
    /// If they're equal, all heap values are accounted for by named variables.
    ///
    /// Only available when the `ref-counting` feature is enabled.
    #[cfg(feature = "ref-counting")]
    pub fn run_ref_counts(&self, inputs: Vec<PyObject>) -> RunRefCountsResult {
        use crate::value::Value;
        use std::collections::HashSet;

        let mut heap = Heap::new(self.namespace_size, NoLimitTracker::default());
        let mut namespaces = self.prepare_namespaces(inputs, &mut heap, &self.interns)?;

        let frame = RunFrame::module_frame(&self.interns);
        let result = frame.execute(&mut namespaces, &mut heap, &self.nodes);

        // Compute ref counts before consuming the heap
        let final_namespace = namespaces.into_global();
        let mut counts = ahash::AHashMap::new();
        let mut unique_ids = HashSet::new();

        for (name, &namespace_id) in &self.name_map {
            if let Some(Value::Ref(id)) = final_namespace.get_opt(namespace_id) {
                counts.insert(name.clone(), heap.get_refcount(*id));
                unique_ids.insert(*id);
            }
        }
        let ref_count_data: RefCountSnapshot = (counts, unique_ids.len(), heap.entry_count());

        // Clean up the namespace after reading ref counts but before moving the heap
        for obj in final_namespace {
            obj.drop_with_heap(&mut heap);
        }

        let python_value = result.map(|frame_exit| PyObject::new(frame_exit, &mut heap, &self.interns))?;

        Ok((python_value, ref_count_data))
    }

    /// Prepares the namespace namespaces for execution.
    ///
    /// Converts each `PyObject` input to a `Value`, allocating on the heap if needed.
    /// Returns the prepared Namespaces or an error if there are too many inputs or invalid input types.
    fn prepare_namespaces<T: ResourceTracker>(
        &self,
        inputs: Vec<PyObject>,
        heap: &mut Heap<T>,
        interns: &Interns,
    ) -> Result<Namespaces, InternalRunError> {
        let Some(extra) = self.namespace_size.checked_sub(inputs.len()) else {
            return Err(InternalRunError::Error(
                format!("input length should be <= {}", self.namespace_size).into(),
            ));
        };
        // Convert each PyObject to a Value, propagating any invalid input errors
        let mut namespace: Vec<Value> = inputs
            .into_iter()
            .map(|pv| pv.to_value(heap, interns))
            .collect::<Result<_, _>>()
            .map_err(|e| InternalRunError::Error(e.to_string().into()))?;
        if extra > 0 {
            namespace.extend((0..extra).map(|_| Value::Undefined));
        }
        Ok(Namespaces::new(namespace))
    }
}

#[cfg(feature = "ref-counting")]
/// Aggregated reference counting statistics returned by `Executor::run_ref_counts`.
type RefCountSnapshot = (ahash::AHashMap<String, usize>, usize, usize);

#[cfg(feature = "ref-counting")]
/// Result type used by `Executor::run_ref_counts`.
type RunRefCountsResult = Result<(PyObject, RefCountSnapshot), RunError>;
