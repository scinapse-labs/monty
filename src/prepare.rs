use std::collections::hash_map::Entry;

use ahash::AHashMap;

use crate::exceptions::{ExcType, ExceptionRaise, SimpleException};
use crate::expressions::{ArgsExpr, Const, Expr, ExprLoc, Identifier, Node};
use crate::operators::{CmpOperator, Operator};
use crate::parse_error::{ParseError, ParseResult};

/// Result of the prepare phase, containing everything needed to execute code.
///
/// This struct holds the outputs of name resolution and AST transformation:
/// - The initial namespace with placeholder values for each variable
/// - A mapping from variable names to their namespace indices (for ref-count testing)
/// - The transformed AST nodes ready for execution
pub(crate) struct PrepareResult<'c> {
    /// Initial namespace as compile-time constants. Each variable gets a slot,
    /// initialized to `Const::Undefined` until assigned at runtime.
    pub namespace: Vec<Const>,
    /// Maps variable names to their indices in the namespace.
    /// Used for ref-count testing to look up variables by name.
    /// Only available when the `ref-counting` feature is enabled.
    #[cfg(feature = "ref-counting")]
    pub name_map: AHashMap<String, usize>,
    /// The prepared AST nodes with all names resolved to namespace indices.
    pub nodes: Vec<Node<'c>>,
}

/// Prepares parsed nodes for execution by resolving names and building the initial namespace.
///
/// The namespace will be converted to runtime Objects when execution begins and the heap is available.
pub(crate) fn prepare<'c>(nodes: Vec<Node<'c>>, input_names: &[&str]) -> ParseResult<'c, PrepareResult<'c>> {
    let mut p = Prepare::new(nodes.len(), input_names, true);
    let prepared_nodes = p.prepare_nodes(nodes)?;
    Ok(PrepareResult {
        namespace: p.namespace,
        #[cfg(feature = "ref-counting")]
        name_map: p.name_map,
        nodes: prepared_nodes,
    })
}

/// State machine for the preparation phase that transforms parsed AST nodes into an executable form.
///
/// This struct maintains the mapping between variable names and their namespace indices,
/// builds the initial namespace with Literals (pre-runtime), and handles scope resolution.
/// The preparation phase is crucial for converting string-based name lookups into efficient
/// integer-indexed namespace access during execution.
struct Prepare {
    /// Maps variable names to their indices in the namespace vector
    name_map: AHashMap<String, usize>,
    /// The namespace vector storing Literal values, one per unique variable name.
    /// Undefined literals are used as placeholders for variables not yet assigned.
    namespace: Vec<Const>,
    /// Root frame is the outer frame of the script, e.g. the "global" scope.
    /// When true, the last expression in a block is implicitly returned.
    root_frame: bool,
}

impl Prepare {
    /// Creates a new Prepare instance with pre-populated input names.
    ///
    /// Input names are typically function parameters or external variables that are
    /// already defined when execution begins. They are given sequential namespace indices
    /// starting from 0.
    ///
    /// # Arguments
    /// * `capacity` - Expected number of nodes, used to preallocate the name map
    /// * `input_names` - Names that should be pre-registered in the namespace (e.g., function parameters)
    /// * `root_frame` - Whether this is the root/global scope (affects return behavior)
    fn new(capacity: usize, input_names: &[&str], root_frame: bool) -> Self {
        let mut name_map = AHashMap::with_capacity(capacity);
        for (index, name) in input_names.iter().enumerate() {
            name_map.insert((*name).to_string(), index);
        }
        let namespace = vec![Const::Undefined; name_map.len()];
        Self {
            name_map,
            namespace,
            root_frame,
        }
    }

    /// Recursively prepares a sequence of AST nodes by resolving names and transforming expressions.
    ///
    /// This method processes each node type differently:
    /// - Resolves variable names to namespace indices
    /// - Transforms function calls from identifier-based to builtin type-based
    /// - Handles special cases like implicit returns in root frames
    /// - Validates that names used in attribute calls are already defined
    ///
    /// # Returns
    /// A vector of prepared nodes ready for execution
    fn prepare_nodes<'c>(&mut self, nodes: Vec<Node<'c>>) -> ParseResult<'c, Vec<Node<'c>>> {
        let nodes_len = nodes.len();
        let mut new_nodes = Vec::with_capacity(nodes_len);
        for (index, node) in nodes.into_iter().enumerate() {
            match node {
                Node::Pass => (),
                Node::Expr(expr) => {
                    let expr = self.prepare_expression(expr)?;
                    // In the root frame (global scope), the last expression is implicitly returned
                    // if it's not None. This matches Python REPL behavior where the last expression
                    // value is displayed/returned.
                    if self.root_frame && index == nodes_len - 1 && !expr.expr.is_none() {
                        new_nodes.push(Node::Return(expr));
                    } else {
                        new_nodes.push(Node::Expr(expr));
                    }
                }
                Node::Return(expr) => {
                    let expr = self.prepare_expression(expr)?;
                    new_nodes.push(Node::Return(expr));
                }
                Node::ReturnNone => new_nodes.push(Node::ReturnNone),
                Node::Raise(exc) => {
                    let expr = match exc {
                        Some(expr) => {
                            match expr.expr {
                                Expr::Name(id) => {
                                    // Handle raising an exception type without instantiation, e.g. `raise TypeError`.
                                    // This is transformed into a call: `raise TypeError()` so the exception
                                    // is properly instantiated before being raised.
                                    let callable = id.name.parse().map_err(|()| {
                                        let name = &id.name;
                                        ParseError::Internal(format!("unknown function: `{name}`").into())
                                    })?;
                                    let expr = Expr::Call {
                                        callable,
                                        args: ArgsExpr::Zero,
                                    };
                                    Some(ExprLoc::new(id.position, expr))
                                }
                                _ => Some(self.prepare_expression(expr)?),
                            }
                        }
                        None => None,
                    };
                    new_nodes.push(Node::Raise(expr));
                }
                Node::Assign { target, object } => {
                    let object = self.prepare_expression(object)?;
                    let (target, _) = self.get_id(target);
                    new_nodes.push(Node::Assign { target, object });
                }
                Node::OpAssign { target, op, object } => {
                    let target = self.get_id(target).0;
                    let object = self.prepare_expression(object)?;
                    new_nodes.push(Node::OpAssign { target, op, object });
                }
                Node::SubscriptAssign { target, index, value } => {
                    let target = self.get_id(target).0;
                    let index = self.prepare_expression(index)?;
                    let value = self.prepare_expression(value)?;
                    new_nodes.push(Node::SubscriptAssign { target, index, value });
                }
                Node::For {
                    target,
                    iter,
                    body,
                    or_else,
                } => {
                    new_nodes.push(Node::For {
                        target: self.get_id(target).0,
                        iter: self.prepare_expression(iter)?,
                        body: self.prepare_nodes(body)?,
                        or_else: self.prepare_nodes(or_else)?,
                    });
                }
                Node::If { test, body, or_else } => {
                    let test = self.prepare_expression(test)?;
                    let body = self.prepare_nodes(body)?;
                    let or_else = self.prepare_nodes(or_else)?;
                    new_nodes.push(Node::If { test, body, or_else });
                }
            }
        }
        Ok(new_nodes)
    }

    /// Prepares an expression by resolving names, transforming calls, and applying optimizations.
    ///
    /// Key transformations performed:
    /// - Name lookups are resolved to namespace indices via `get_id`
    /// - Function calls are resolved from identifiers to builtin types
    /// - Attribute calls validate that the object is already defined (not a new name)
    /// - Lists and tuples are recursively prepared
    /// - Modulo equality patterns like `x % n == k` (constant right-hand side) are optimized to
    ///   `CmpOperator::ModEq`
    ///
    /// # Errors
    /// Returns a NameError if an attribute call references an undefined variable
    fn prepare_expression<'c>(&mut self, loc_expr: ExprLoc<'c>) -> ParseResult<'c, ExprLoc<'c>> {
        let ExprLoc { position, expr } = loc_expr;
        let expr = match expr {
            Expr::Constant(object) => Expr::Constant(object),
            Expr::Name(name) => Expr::Name(self.get_id(name).0),
            Expr::Op { left, op, right } => Expr::Op {
                left: Box::new(self.prepare_expression(*left)?),
                op,
                right: Box::new(self.prepare_expression(*right)?),
            },
            Expr::CmpOp { left, op, right } => Expr::CmpOp {
                left: Box::new(self.prepare_expression(*left)?),
                op,
                right: Box::new(self.prepare_expression(*right)?),
            },
            Expr::Call { callable, mut args } => {
                // The callable is already resolved, just prepare the arguments and pass through
                args.prepare_args(|expr| self.prepare_expression(expr))?;
                Expr::Call { callable, args }
            }
            Expr::AttrCall { object, attr, mut args } => {
                let (object, is_new) = self.get_id(object);
                // Unlike regular name lookups, attribute calls require the object to already exist.
                // Calling a method on an undefined variable should fail at prepare-time, not runtime.
                // Example: `undefined_var.method()` should raise NameError here.
                if is_new {
                    let exc: ExceptionRaise = SimpleException::new(ExcType::NameError, Some(object.name.into())).into();
                    return Err(exc.into());
                }
                args.prepare_args(|expr| self.prepare_expression(expr))?;
                Expr::AttrCall { object, attr, args }
            }
            Expr::List(elements) => {
                let expressions = elements
                    .into_iter()
                    .map(|e| self.prepare_expression(e))
                    .collect::<ParseResult<_>>()?;
                Expr::List(expressions)
            }
            Expr::Tuple(elements) => {
                let expressions = elements
                    .into_iter()
                    .map(|e| self.prepare_expression(e))
                    .collect::<ParseResult<_>>()?;
                Expr::Tuple(expressions)
            }
            Expr::Subscript { object, index } => Expr::Subscript {
                object: Box::new(self.prepare_expression(*object)?),
                index: Box::new(self.prepare_expression(*index)?),
            },
            Expr::Dict(pairs) => {
                let prepared_pairs = pairs
                    .into_iter()
                    .map(|(k, v)| Ok((self.prepare_expression(k)?, self.prepare_expression(v)?)))
                    .collect::<ParseResult<_>>()?;
                Expr::Dict(prepared_pairs)
            }
        };

        // Optimization: Transform `(x % n) == value` with any constant right-hand side into a
        // specialized ModEq operator.
        // This is a common pattern in competitive programming (e.g., FizzBuzz checks like `i % 3 == 0`)
        // and can be executed more efficiently with a single modulo operation + comparison
        // instead of separate modulo, then equality check.
        if let Expr::CmpOp { left, op, right } = &expr {
            if op == &CmpOperator::Eq {
                if let Expr::Constant(Const::Int(value)) = right.expr {
                    if let Expr::Op {
                        left: left2,
                        op,
                        right: right2,
                    } = &left.expr
                    {
                        if op == &Operator::Mod {
                            let new_expr = Expr::CmpOp {
                                left: left2.clone(),
                                op: CmpOperator::ModEq(value),
                                right: right2.clone(),
                            };
                            return Ok(ExprLoc {
                                position: left.position,
                                expr: new_expr,
                            });
                        }
                    }
                }
            }
        }

        Ok(ExprLoc { position, expr })
    }

    /// Resolves an identifier to its namespace index, creating a new entry if needed.
    ///
    /// This is the core name resolution mechanism. If the name already exists in the name_map,
    /// its existing index is returned. If it's a new name, it's added to both the name_map
    /// and namespace (as Literal::Undefined), and assigned the next available index.
    ///
    /// # Returns
    /// A tuple of (resolved Identifier with id set, whether this is a new name).
    /// The boolean is true if this name was just added to the namespace.
    fn get_id<'c>(&mut self, ident: Identifier<'c>) -> (Identifier<'c>, bool) {
        let (id, is_new) = match self.name_map.entry(ident.name.clone()) {
            Entry::Occupied(e) => {
                let id = e.get();
                (*id, false)
            }
            Entry::Vacant(e) => {
                // New name: allocate next index and add Undefined placeholder
                let id = self.namespace.len();
                self.namespace.push(Const::Undefined);
                e.insert(id);
                (id, true)
            }
        };
        (
            Identifier {
                name: ident.name,
                id,
                position: ident.position,
            },
            is_new,
        )
    }
}
