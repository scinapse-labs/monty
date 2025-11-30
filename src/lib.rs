mod args;
mod builtins;
mod evaluate;
pub mod exceptions;
mod exit;
mod expressions;
mod heap;
mod object;
mod operators;
mod parse;
mod parse_error;
mod prepare;
mod run;
mod values;

#[cfg(feature = "ref-counting")]
use std::collections::HashMap;

#[cfg(feature = "ref-counting")]
use ahash::AHashMap;

use crate::exceptions::{InternalRunError, RunError};
pub use crate::exit::{Exit, Value};
use crate::expressions::{Const, Node};
use crate::heap::{Heap, HeapData};
use crate::object::Object;
use crate::parse::parse;
// TODO should these really be public?
pub use crate::parse_error::{ParseError, ParseResult};
use crate::prepare::prepare;
use crate::run::RunFrame;

/// Main executor that compiles and runs Python code.
///
/// The executor stores the compiled AST and initial namespace as literals (not runtime
/// objects). When `run()` is called, literals are converted to heap-allocated runtime
/// objects, ensuring proper reference counting from the start of execution.
///
/// When the `ref-count-testing` feature is enabled, the `last_namespace` field stores
/// the final state of all variables, which can be used along with `name_map` to inspect
/// reference counts for testing.
#[derive(Debug)]
pub struct Executor<'c> {
    initial_namespace: Vec<Const>,
    /// Maps variable names to their indices in the namespace. Preserved for ref-count testing.
    #[cfg(feature = "ref-counting")]
    name_map: AHashMap<String, usize>,
    nodes: Vec<Node<'c>>,
    heap: Heap,
    /// Stores the namespace after the last run() call for ref-count inspection.
    #[cfg(feature = "ref-counting")]
    last_namespace: Option<Vec<Object>>,
}

impl<'c> Executor<'c> {
    pub fn new(code: &'c str, filename: &'c str, input_names: &[&str]) -> ParseResult<'c, Self> {
        let nodes = parse(code, filename)?;
        // dbg!(&nodes);
        let prepared = prepare(nodes, input_names)?;
        // dbg!(&prepared.namespace, &prepared.nodes);
        Ok(Self {
            initial_namespace: prepared.namespace,
            #[cfg(feature = "ref-counting")]
            name_map: prepared.name_map,
            nodes: prepared.nodes,
            heap: Heap::default(),
            #[cfg(feature = "ref-counting")]
            last_namespace: None,
        })
    }

    /// Executes the code with the given input values.
    ///
    /// The heap is cleared at the start of each run, ensuring no state leaks between
    /// executions. The initial namespace (stored as Literals) is converted to runtime
    /// Objects with proper heap allocation and reference counting.
    ///
    /// When `ref-count-testing` feature is enabled, the namespace is stored in
    /// `last_namespace` after execution for ref-count inspection.
    ///
    /// # Arguments
    /// * `inputs` - Values to fill the first N slots of the namespace (e.g., function parameters)
    pub fn run<'h>(&'h mut self, inputs: Vec<Object>) -> Result<Exit<'c, 'h>, InternalRunError> {
        // Clear heap before starting new execution
        self.heap.clear();
        #[cfg(feature = "ref-counting")]
        {
            self.last_namespace = None;
        }

        // Convert initial namespace from Literals to Objects with heap allocation
        let mut namespace: Vec<Object> = self
            .initial_namespace
            .iter()
            .map(|lit| lit.to_object(&mut self.heap))
            .collect();

        // Fill in the input values (overwriting the default Undefined slots)
        for (i, input) in inputs.into_iter().enumerate() {
            namespace[i] = input;
        }
        // dbg!(&self.nodes, &self.heap);

        let mut frame = RunFrame::new(namespace);
        let result = frame.execute(&mut self.heap, &self.nodes);

        // Store namespace for ref-count inspection (only with feature enabled)
        #[cfg(feature = "ref-counting")]
        {
            self.last_namespace = Some(frame.into_namespace());
        }

        match result {
            Ok(v) => Ok(Exit::new(v, &self.heap)),
            Err(e) => match e {
                RunError::Exc(exc) => Ok(Exit::Raise(exc)),
                RunError::Internal(internal) => Err(internal),
            },
        }
    }

    /// Returns reference counts for named variables after the last run.
    ///
    /// Returns a tuple of:
    /// - A map from variable names to their reference counts (only for heap-allocated objects)
    /// - The number of unique heap object IDs referenced by variables
    /// - The total number of live heap objects (for strict matching validation)
    ///
    /// For strict matching, compare unique_refs_count with heap_object_count. If they're equal,
    /// all heap objects are accounted for by named variables.
    ///
    /// Returns None if no execution has occurred yet.
    ///
    /// Only available when the `ref-counting` feature is enabled.
    #[cfg(feature = "ref-counting")]
    #[must_use]
    pub fn get_ref_counts(&self) -> Option<(HashMap<String, usize>, usize, usize)> {
        use std::collections::HashSet;

        let namespace = self.last_namespace.as_ref()?;
        let mut counts = HashMap::new();
        let mut unique_ids = HashSet::new();

        for (name, &index) in &self.name_map {
            if let Some(Object::Ref(id)) = namespace.get(index) {
                counts.insert(name.clone(), self.heap.get_refcount(*id));
                unique_ids.insert(*id);
            }
        }
        Some((counts, unique_ids.len(), self.heap.object_count()))
    }
}

/// parse code and show the parsed AST, mostly for testing
pub fn parse_show(code: &str, filename: &str) -> Result<String, String> {
    match parse(code, filename) {
        Ok(ast) => Ok(format!("{ast:#?}")),
        Err(e) => Err(e.to_string()),
    }
}
