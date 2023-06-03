use std::borrow::Cow;

use crate::evaluate::Evaluator;
use crate::exceptions::{exc, exc_err, Exception, InternalRunError, RunError, StackFrame};
use crate::object::Object;
use crate::parse::CodeRange;
use crate::types::{Exit, ExprLoc, Identifier, Node, Operator};

pub type RunResult<T> = Result<T, RunError>;

#[derive(Debug)]
pub(crate) struct RunFrame {
    namespace: Vec<Object>,
    parent: Option<StackFrame>,
    name: Cow<'static, str>,
}

impl RunFrame {
    pub fn new(namespace: Vec<Object>) -> Self {
        Self {
            namespace,
            parent: None,
            name: "<module>".into(),
        }
    }

    pub fn execute(&mut self, nodes: &[Node]) -> RunResult<Exit> {
        for node in nodes {
            match self.execute_node(node)? {
                Some(leave) => return Ok(leave),
                None => (),
            }
        }
        Ok(Exit::ReturnNone)
    }

    fn execute_node(&mut self, node: &Node) -> RunResult<Option<Exit>> {
        match node {
            Node::Pass => return exc_err!(InternalRunError::Error; "Unexpected `pass` in execution"),
            Node::Expr(expr) => {
                self.execute_expr(expr)?;
            }
            Node::Return(expr) => return Ok(Some(Exit::Return(self.execute_expr(expr)?.into_owned()))),
            Node::ReturnNone => return Ok(Some(Exit::ReturnNone)),
            Node::Assign { target, object } => {
                self.assign(target, object)?;
            }
            Node::OpAssign { target, op, object } => {
                self.op_assign(target, op, object)?;
            }
            Node::For {
                target,
                iter,
                body,
                or_else,
            } => self.for_loop(target, iter, body, or_else)?,
            Node::If { test, body, or_else } => self.if_(test, body, or_else)?,
        };
        Ok(None)
    }

    fn execute_expr<'a>(&'a self, expr: &'a ExprLoc) -> RunResult<Cow<Object>> {
        // TODO: does creating this struct harm performance, or is it optimised out?
        Evaluator::new(&self.namespace)
            .evaluate(expr)
            .map_err(|e| self.set_name(e))
    }

    fn execute_expr_bool(&self, expr: &ExprLoc) -> RunResult<bool> {
        Evaluator::new(&self.namespace)
            .evaluate_bool(expr)
            .map_err(|e| self.set_name(e))
    }

    fn assign(&mut self, target: &Identifier, object: &ExprLoc) -> RunResult<()> {
        self.namespace[target.id] = self.execute_expr(&object)?.into_owned();
        Ok(())
    }

    fn op_assign(&mut self, target: &Identifier, op: &Operator, object: &ExprLoc) -> RunResult<()> {
        let right_object = self.execute_expr(&object)?.into_owned();
        if let Some(target_object) = self.namespace.get_mut(target.id) {
            let r = match op {
                Operator::Add => target_object.add_mut(right_object),
                _ => return exc_err!(InternalRunError::TodoError; "Assign operator {op:?} not yet implemented"),
            };
            if let Err(right) = r {
                let target_type = target_object.to_string();
                let right_type = right.to_string();
                let e = exc!(Exception::TypeError; "unsupported operand type(s) for {op}: '{target_type}' and '{right_type}'");
                Err(e.with_frame(self.stack_frame(&object.position)).into())
            } else {
                Ok(())
            }
        } else {
            // TODO stack_frame once position is added to identifier
            Err(Exception::NameError(target.name.clone().into()).into())
        }
    }

    fn for_loop(&mut self, target: &Identifier, iter: &ExprLoc, body: &[Node], _or_else: &[Node]) -> RunResult<()> {
        let range_size = match self.execute_expr(iter)?.as_ref() {
            Object::Range(s) => *s,
            _ => return exc_err!(InternalRunError::TodoError; "`for` iter must be a range"),
        };

        for object in 0i64..range_size {
            self.namespace[target.id] = Object::Int(object);
            self.execute(body)?;
        }
        Ok(())
    }

    fn if_(&mut self, test: &ExprLoc, body: &[Node], or_else: &[Node]) -> RunResult<()> {
        if self.execute_expr_bool(test)? {
            self.execute(body)?;
        } else {
            self.execute(or_else)?;
        }
        Ok(())
    }

    fn stack_frame(&self, position: &CodeRange) -> StackFrame {
        StackFrame::new(position, &self.name, &self.parent)
    }

    fn set_name(&self, mut error: RunError) -> RunError {
        if let RunError::Exc(ref mut exc) = error {
            if let Some(ref mut stack_frame) = exc.frame {
                stack_frame.frame_name = Some(self.name.clone());
            }
        }
        error
    }
}
