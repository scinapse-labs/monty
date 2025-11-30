use crate::args::Args;
use crate::exceptions::{internal_err, ExcType, InternalRunError, SimpleException};
use crate::expressions::{ArgsExpr, Callable, Expr, ExprLoc, Identifier};
use crate::heap::Heap;
use crate::object::{Attr, Object};
use crate::operators::{CmpOperator, Operator};
use crate::run::RunResult;
use crate::values::{Dict, List, PyValue};
use crate::HeapData;

/// Evaluates an expression node and returns a value.
///
/// `namespace` provides the current frame bindings, while `heap` is threaded so any
/// future heap-backed objects can be created/cloned without re-threading plumbing later.
pub(crate) fn evaluate_use<'c, 'd>(
    namespace: &'d mut [Object],
    heap: &'d mut Heap,
    expr_loc: &'d ExprLoc<'c>,
) -> RunResult<'c, Object> {
    match &expr_loc.expr {
        Expr::Constant(literal) => Ok(literal.to_object(heap)),
        Expr::Name(ident) => {
            if let Some(object) = namespace.get(ident.id) {
                match object {
                    Object::Undefined => Err(InternalRunError::Undefined(ident.name.clone().into()).into()),
                    _ => Ok(object.clone_with_heap(heap)),
                }
            } else {
                let name = ident.name.clone();

                Err(SimpleException::new(ExcType::NameError, Some(name.into()))
                    .with_position(expr_loc.position)
                    .into())
            }
        }
        Expr::Call { callable, args } => Ok(call_function(namespace, heap, callable, args)?),
        Expr::AttrCall { object, attr, args } => Ok(attr_call(namespace, heap, expr_loc, object, attr, args)?),
        Expr::Op { left, op, right } => eval_op(namespace, heap, left, op, right),
        Expr::CmpOp { left, op, right } => Ok(cmp_op(namespace, heap, left, op, right)?.into()),
        Expr::List(elements) => {
            let objects = elements
                .iter()
                .map(|e| evaluate_use(namespace, heap, e))
                .collect::<RunResult<_>>()?;
            let object_id = heap.allocate(HeapData::List(List::new(objects)));
            Ok(Object::Ref(object_id))
        }
        Expr::Tuple(elements) => {
            let objects = elements
                .iter()
                .map(|e| evaluate_use(namespace, heap, e))
                .collect::<RunResult<_>>()?;
            let object_id = heap.allocate(HeapData::Tuple(objects));
            Ok(Object::Ref(object_id))
        }
        Expr::Subscript { object, index } => {
            let obj = evaluate_use(namespace, heap, object)?;
            let key = evaluate_use(namespace, heap, index)?;
            let result = obj.py_getitem(&key, heap);
            // Drop temporary references to object and key
            obj.drop_with_heap(heap);
            key.drop_with_heap(heap);
            result
        }
        Expr::Dict(pairs) => {
            let mut eval_pairs = Vec::new();
            for (key_expr, value_expr) in pairs {
                let key = evaluate_use(namespace, heap, key_expr)?;
                let value = evaluate_use(namespace, heap, value_expr)?;
                eval_pairs.push((key, value));
            }
            let dict = Dict::from_pairs(eval_pairs, heap)?;
            let dict_id = heap.allocate(HeapData::Dict(dict));
            Ok(Object::Ref(dict_id))
        }
    }
}

/// Evaluates an expression node and discard the returned value
///
/// `namespace` provides the current frame bindings, while `heap` is threaded so any
/// future heap-backed objects can be created/cloned without re-threading plumbing later.
pub(crate) fn evaluate_discard<'c, 'd>(
    namespace: &'d mut [Object],
    heap: &'d mut Heap,
    expr_loc: &'d ExprLoc<'c>,
) -> RunResult<'c, ()> {
    match &expr_loc.expr {
        Expr::Constant(_) => Ok(()),
        Expr::Name(ident) => {
            if let Some(object) = namespace.get(ident.id) {
                match object {
                    Object::Undefined => Err(InternalRunError::Undefined(ident.name.clone().into()).into()),
                    _ => Ok(()),
                }
            } else {
                let name = ident.name.clone();

                Err(SimpleException::new(ExcType::NameError, Some(name.into()))
                    .with_position(expr_loc.position)
                    .into())
            }
        }
        Expr::Call { callable, args } => call_function(namespace, heap, callable, args).map(|_| ()),
        Expr::AttrCall { object, attr, args } => attr_call(namespace, heap, expr_loc, object, attr, args).map(|_| ()),
        Expr::Op { left, op, right } => eval_op(namespace, heap, left, op, right).map(|_| ()),
        Expr::CmpOp { left, op, right } => cmp_op(namespace, heap, left, op, right).map(|_| ()),
        Expr::List(elements) => {
            for el in elements {
                evaluate_discard(namespace, heap, el)?;
            }
            Ok(())
        }
        Expr::Tuple(elements) => {
            for el in elements {
                evaluate_discard(namespace, heap, el)?;
            }
            Ok(())
        }
        Expr::Subscript { object, index } => {
            evaluate_discard(namespace, heap, object)?;
            evaluate_discard(namespace, heap, index)?;
            Ok(())
        }
        Expr::Dict(pairs) => {
            for (key_expr, value_expr) in pairs {
                evaluate_discard(namespace, heap, key_expr)?;
                evaluate_discard(namespace, heap, value_expr)?;
            }
            Ok(())
        }
    }
}

/// Specialized helper for truthiness checks; shares implementation with `evaluate`.
pub(crate) fn evaluate_bool<'c, 'd>(
    namespace: &'d mut [Object],
    heap: &'d mut Heap,
    expr_loc: &'d ExprLoc<'c>,
) -> RunResult<'c, bool> {
    if let Expr::CmpOp { left, op, right } = &expr_loc.expr {
        cmp_op(namespace, heap, left, op, right)
    } else {
        let obj = evaluate_use(namespace, heap, expr_loc)?;
        let result = obj.py_bool(heap);
        // Drop temporary reference
        obj.drop_with_heap(heap);
        Ok(result)
    }
}

/// Evaluates a binary operator expression (`+, -, %`, etc.).
fn eval_op<'c, 'd>(
    namespace: &'d mut [Object],
    heap: &'d mut Heap,
    left: &'d ExprLoc<'c>,
    op: &'d Operator,
    right: &'d ExprLoc<'c>,
) -> RunResult<'c, Object> {
    let left_object = evaluate_use(namespace, heap, left)?;
    let right_object = evaluate_use(namespace, heap, right)?;
    let op_object: Option<Object> = match op {
        Operator::Add => left_object.py_add(&right_object, heap),
        Operator::Sub => left_object.py_sub(&right_object, heap),
        Operator::Mod => left_object.py_mod(&right_object),
        _ => {
            // Drop temporary references before early return
            left_object.drop_with_heap(heap);
            right_object.drop_with_heap(heap);
            return internal_err!(InternalRunError::TodoError; "Operator {op:?} not yet implemented");
        }
    };
    match op_object {
        Some(object) => {
            // Drop temporary references to operands now that the operation is complete
            left_object.drop_with_heap(heap);
            right_object.drop_with_heap(heap);
            Ok(object)
        }
        None => SimpleException::operand_type_error(left, op, right, left_object, right_object, heap),
    }
}

/// Evaluates comparison operators, reusing `evaluate` so heap semantics remain consistent.
fn cmp_op<'c, 'd>(
    namespace: &'d mut [Object],
    heap: &'d mut Heap,
    left: &'d ExprLoc<'c>,
    op: &'d CmpOperator,
    right: &'d ExprLoc<'c>,
) -> RunResult<'c, bool> {
    let mut left_object = evaluate_use(namespace, heap, left)?;
    let mut right_object = evaluate_use(namespace, heap, right)?;

    // For ModEq, handle separately since error path consumes the objects
    if let CmpOperator::ModEq(v) = op {
        return match left_object.py_mod_eq(&right_object, *v) {
            Some(b) => {
                left_object.drop_with_heap(heap);
                right_object.drop_with_heap(heap);
                Ok(b)
            }
            None => SimpleException::operand_type_error(left, &Operator::Mod, right, left_object, right_object, heap),
        };
    }

    let result = match op {
        CmpOperator::Eq => left_object.py_eq(&right_object, heap),
        CmpOperator::NotEq => !left_object.py_eq(&right_object, heap),
        CmpOperator::Gt => left_object.gt(&right_object),
        CmpOperator::GtE => left_object.ge(&right_object),
        CmpOperator::Lt => left_object.lt(&right_object),
        CmpOperator::LtE => left_object.le(&right_object),
        CmpOperator::Is => left_object.is(heap, &mut right_object),
        CmpOperator::IsNot => !left_object.is(heap, &mut right_object),
        CmpOperator::ModEq(_) => unreachable!(), // Handled above
        _ => {
            left_object.drop_with_heap(heap);
            right_object.drop_with_heap(heap);
            return internal_err!(InternalRunError::TodoError; "Operator {op:?} not yet implemented");
        }
    };

    // Drop temporary references to operands
    left_object.drop_with_heap(heap);
    right_object.drop_with_heap(heap);
    Ok(result)
}

/// Evaluates callable function calls, collecting argument values via the shared heap.
///
/// Handles builtin functions, exception constructors, and (eventually) user-defined functions.
fn call_function<'c, 'd>(
    namespace: &'d mut [Object],
    heap: &'d mut Heap,
    callable: &'d Callable,
    args: &'d ArgsExpr<'c>,
) -> RunResult<'c, Object> {
    let args = evaluate_args(namespace, heap, args)?;
    match callable {
        Callable::Builtin(builtin) => builtin.call(heap, args),
        Callable::Exception(exc_type) => call_exception(heap, args, *exc_type),
        Callable::Ident(_) => {
            internal_err!(InternalRunError::TodoError; "User defined functions not yet implemented")
        }
    }
}

/// Handles attribute method calls like `list.append`, again threading the heap for safety.
fn attr_call<'c, 'd>(
    namespace: &'d mut [Object],
    heap: &'d mut Heap,
    expr_loc: &'d ExprLoc<'c>,
    object_ident: &Identifier<'c>,
    attr: &Attr,
    args: &'d ArgsExpr<'c>,
) -> RunResult<'c, Object> {
    // Evaluate arguments first to avoid borrow conflicts
    let args = evaluate_args(namespace, heap, args)?;

    let object = if let Some(object) = namespace.get_mut(object_ident.id) {
        match object {
            Object::Undefined => return Err(InternalRunError::Undefined(object_ident.name.clone().into()).into()),
            _ => object,
        }
    } else {
        let name = object_ident.name.clone();

        return Err(SimpleException::new(ExcType::NameError, Some(name.into()))
            .with_position(expr_loc.position)
            .into());
    };
    object.call_attr(heap, attr, args)
}

fn call_exception<'c>(heap: &mut Heap, args: Args, exc_type: ExcType) -> RunResult<'c, Object> {
    match args {
        Args::Zero => return Ok(Object::Exc(SimpleException::new(exc_type, None))),
        Args::One(Object::Ref(object_id)) => {
            if let HeapData::Str(s) = heap.get(object_id) {
                return Ok(Object::Exc(SimpleException::new(
                    exc_type,
                    Some(s.as_str().to_owned().into()),
                )));
            }
        }
        _ => {}
    }
    internal_err!(InternalRunError::TodoError; "Exceptions can only be called with zero or one string argument")
}

/// Evaluates function arguments into an Args, optimized for common argument counts.
#[inline]
fn evaluate_args<'c, 'd>(
    namespace: &'d mut [Object],
    heap: &'d mut Heap,
    args_expr: &'d ArgsExpr<'c>,
) -> RunResult<'c, Args> {
    match args_expr {
        ArgsExpr::Zero => Ok(Args::Zero),
        ArgsExpr::One(arg) => evaluate_use(namespace, heap, arg).map(Args::One),
        ArgsExpr::Two(arg1, arg2) => {
            let arg0 = evaluate_use(namespace, heap, arg1)?;
            let arg1 = evaluate_use(namespace, heap, arg2)?;
            Ok(Args::Two(arg0, arg1))
        }
        ArgsExpr::Args(args) => args
            .iter()
            .map(|a| evaluate_use(namespace, heap, a))
            .collect::<RunResult<_>>()
            .map(Args::Many),
        _ => todo!("Implement evaluation for kwargs"),
    }
}
