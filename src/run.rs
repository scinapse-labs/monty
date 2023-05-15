use std::borrow::Cow;

use crate::prepare::{RunExpr, RunNode};
use crate::types::{Builtins, CmpOperator, Expr, Node, Operator, Value};

pub type RunResult<T> = Result<T, Cow<'static, str>>;

pub(crate) fn run(namespace_size: usize, nodes: &[RunNode]) -> RunResult<()> {
    let mut frame = Frame::new(namespace_size);
    frame.execute(nodes)
}

#[derive(Debug)]
struct Frame {
    namespace: Vec<Value>,
}

impl Frame {
    fn new(namespace_size: usize) -> Self {
        Self {
            namespace: vec![Value::Undefined; namespace_size],
        }
    }

    fn execute(&mut self, nodes: &[RunNode]) -> RunResult<()> {
        for node in nodes {
            self.execute_node(node)?;
        }
        Ok(())
    }

    fn execute_node(&mut self, node: &RunNode) -> RunResult<()> {
        match node {
            Node::Pass => return Err("Unexpected `pass` in execution".into()),
            Node::Expr(expr) => {
                self.execute_expr(expr)?;
            },
            Node::Assign { target, value } => {
                self.assign(*target, value)?;
            },
            Node::OpAssign { target, op, value } => {
                self.op_assign(*target, op, &value)?;
            },
            Node::For {
                target,
                iter,
                body,
                or_else,
            } => self.for_loop(target, iter, body, or_else)?,
            Node::If { test, body, or_else } => self.if_(test, body, or_else)?,
        };
        Ok(())
    }

    fn execute_expr<'a>(&'a self, expr: &'a RunExpr) -> RunResult<Cow<Value>> {
        match expr {
            Expr::Constant(value) => Ok(Cow::Borrowed(value)),
            Expr::Name(id) => {
                if let Some(value) = self.namespace.get(*id) {
                    match value {
                        Value::Undefined => Err(format!("name '{}' is not defined", id).into()),
                        _ => Ok(Cow::Borrowed(value)),
                    }
                } else {
                    Err(format!("name '{}' is not defined", id).into())
                }
            }
            Expr::Call { func, args } => self.call_function(func, args),
            Expr::Op { left, op, right } => self.op(left, op, right),
            Expr::CmpOp { left, op, right } => Ok(Cow::Owned(self.cmp_op(left, op, right)?.into())),
            Expr::List(elements) => {
                let values = elements
                    .iter()
                    .map(|e| match self.execute_expr(e) {
                        Ok(Cow::Borrowed(value)) => Ok(value.clone()),
                        Ok(Cow::Owned(value)) => Ok(value),
                        Err(e) => Err(e),
                    })
                    .collect::<RunResult<_>>()?;
                Ok(Cow::Owned(Value::List(values)))
            }
        }
    }

    fn execute_expr_bool(&self, expr: &RunExpr) -> RunResult<bool> {
        match expr {
            Expr::CmpOp { left, op, right } => self.cmp_op(left, op, right),
            _ => {
                let value = self.execute_expr(expr)?;
                value.as_ref().bool().ok_or_else(|| Cow::Owned(format!("Cannot convert {} to bool", value.as_ref())))
            }
        }
    }

    fn assign(&mut self, target: usize, value: &RunExpr) -> RunResult<()> {
        self.namespace[target] = match self.execute_expr(value)? {
            Cow::Borrowed(value) => value.clone(),
            Cow::Owned(value) => value,
        };
        Ok(())
    }

    fn op_assign(&mut self, target: usize, op: &Operator, value: &RunExpr) -> RunResult<()> {
        let right_value = match self.execute_expr(value)? {
            Cow::Borrowed(value) => value.clone(),
            Cow::Owned(value) => value,
        };
        if let Some(target_value) = self.namespace.get_mut(target) {
            let ok = match op {
                Operator::Add => target_value.add_mut(right_value),
                _ => return Err(format!("Assign operator {op:?} not yet implemented").into()),
            };
            match ok {
                true => Ok(()),
                false => Err(format!("Cannot apply assign operator {op:?} {value:?}").into()),
            }
        } else {
            Err(format!("name '{target}' is not defined").into())
        }
    }

    fn call_function(&self, builtin: &Builtins, args: &[RunExpr]) -> RunResult<Cow<Value>> {
        match builtin {
            Builtins::Print => {
                for (i, arg) in args.iter().enumerate() {
                    let value = self.execute_expr(arg)?;
                    if i == 0 {
                        print!("{value}");
                    } else {
                        print!(" {value}");
                    }
                }
                println!();
                Ok(Cow::Owned(Value::None))
            }
            Builtins::Range => {
                if args.len() != 1 {
                    Err("range() takes exactly one argument".into())
                } else {
                    let value = self.execute_expr(&args[0])?;
                    match value.as_ref() {
                        Value::Int(size) => Ok(Cow::Owned(Value::Range(*size))),
                        _ => Err("range() argument must be an integer".into()),
                    }
                }
            },
            Builtins::Len => {
                if args.len() != 1 {
                    Err(format!("len() takes exactly exactly one argument ({} given)", args.len()).into())
                } else {
                    let value = self.execute_expr(&args[0])?;
                    match value.len() {
                        Some(len) => Ok(Cow::Owned(Value::Int(len as i64))),
                        None => Err(format!("Object of type {value} has no len()").into()),
                    }
                }
            }
        }
    }

    fn for_loop(
        &mut self,
        target: &RunExpr,
        iter: &RunExpr,
        body: &[RunNode],
        _or_else: &[RunNode],
    ) -> RunResult<()> {
        let target_id = match target {
            Expr::Name(id) => *id,
            _ => return Err("For target must be a name".into()),
        };
        let range_size = match self.execute_expr(iter)?.as_ref() {
            Value::Range(s) => *s,
            _ => return Err("For iter must be a range".into()),
        };

        for value in 0i64..range_size {
            self.namespace[target_id] = Value::Int(value);
            self.execute(body)?;
        }
        Ok(())
    }

    fn if_(&mut self, test: &RunExpr, body: &[RunNode], or_else: &[RunNode]) -> RunResult<()> {
        if self.execute_expr_bool(test)? {
            self.execute(body)?;
        } else {
            self.execute(or_else)?;
        }
        Ok(())
    }

    fn op(&self, left: &RunExpr, op: &Operator, right: &RunExpr) -> RunResult<Cow<Value>> {
        let left_value = self.execute_expr(left)?;
        let right_value = self.execute_expr(right)?;
        let op_value: Option<Value> = match op {
            Operator::Add => left_value.add(&right_value),
            Operator::Sub => left_value.sub(&right_value),
            Operator::Mod => left_value.modulo(&right_value),
            _ => return Err(format!("Operator {op:?} not yet implemented").into()),
        };
        match op_value {
            Some(value) => Ok(Cow::Owned(value)),
            None => Err(format!("Cannot apply operator {left:?} {op:?} {right:?}").into()),
        }
    }

    fn cmp_op(&self, left: &RunExpr, op: &CmpOperator, right: &RunExpr) -> RunResult<bool> {
        let left_value = self.execute_expr(left)?;
        let right_value = self.execute_expr(right)?;
        let op_value: Option<bool> = match op {
            CmpOperator::Eq => left_value.as_ref().eq(&right_value),
            CmpOperator::NotEq => match left_value.as_ref().eq(&right_value) {
                Some(value) => Some(!value),
                None => None,
            },
            CmpOperator::Gt => Some(left_value.gt(&right_value)),
            CmpOperator::GtE => Some(left_value.ge(&right_value)),
            CmpOperator::Lt => Some(left_value.lt(&right_value)),
            CmpOperator::LtE => Some(left_value.le(&right_value)),
            _ => return Err(format!("CmpOperator {op:?} not yet implemented").into()),
        };
        match op_value {
            Some(value) => Ok(value),
            None => Err(format!("Cannot apply comparison operator {left:?} {op:?} {right:?}").into()),
        }
    }
}
