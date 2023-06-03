use std::borrow::Cow;
use std::fmt;

use crate::parse::CodeRange;

#[derive(Debug, Clone, PartialEq)]
pub enum Exception {
    ValueError(Cow<'static, str>),
    TypeError(Cow<'static, str>),
    NameError(Cow<'static, str>),
}

impl fmt::Display for Exception {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ValueError(s) => write!(f, "{s}"),
            Self::TypeError(s) => write!(f, "{s}"),
            Self::NameError(s) => write!(f, "{s}"),
        }
    }
}

impl Exception {
    pub(crate) fn str_with_type(&self) -> String {
        format!("{}: {self}", self.type_str())
    }

    // TODO should also be replaced by ObjectType enum
    pub(crate) fn type_str(&self) -> &'static str {
        match self {
            Self::ValueError(_) => "ValueError",
            Self::TypeError(_) => "TypeError",
            Self::NameError(_) => "NameError",
        }
    }

    pub(crate) fn with_frame(self, frame: StackFrame) -> ExceptionRaise {
        ExceptionRaise {
            exc: self,
            frame: Some(frame),
        }
    }

    pub(crate) fn with_position(self, position: &CodeRange) -> ExceptionRaise {
        ExceptionRaise {
            exc: self,
            frame: Some(StackFrame::from_position(position)),
        }
    }
}

macro_rules! exc {
    ($error_type:expr; $msg:tt) => {
        $error_type(format!($msg).into())
    };
    ($error_type:expr; $msg:tt, $( $msg_args:expr ),+ ) => {
        $error_type(format!($msg, $( $msg_args ),+).into())
    };
}
pub(crate) use exc;

macro_rules! exc_err {
    ($error_type:expr; $msg:tt) => {
        Err(crate::exceptions::exc!($error_type; $msg).into())
    };
    ($error_type:expr; $msg:tt, $( $msg_args:expr ),+ ) => {
        Err(crate::exceptions::exc!($error_type; $msg, $( $msg_args ),+).into())
    };
}
pub(crate) use exc_err;

#[derive(Debug, Clone)]
pub struct ExceptionRaise {
    pub(crate) exc: Exception,
    // first in vec is closes "bottom" frame
    pub(crate) frame: Option<StackFrame>,
}

impl fmt::Display for ExceptionRaise {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref frame) = self.frame {
            writeln!(f, "Traceback (most recent call last):")?;
            write!(f, "{}", frame)?;
        }
        write!(f, "{}", self.exc.str_with_type())
    }
}

impl From<Exception> for ExceptionRaise {
    fn from(exc: Exception) -> Self {
        ExceptionRaise { exc, frame: None }
    }
}

#[derive(Debug, Clone)]
pub struct StackFrame {
    pub(crate) position: CodeRange,
    pub(crate) frame_name: Option<Cow<'static, str>>,
    pub(crate) parent: Option<Box<StackFrame>>,
}

impl fmt::Display for StackFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref parent) = self.parent {
            write!(f, "{}", parent)?;
        }

        self.position.traceback(f, self.frame_name.as_ref())
    }
}

impl StackFrame {
    pub(crate) fn new(position: &CodeRange, frame_name: &Cow<'static, str>, parent: &Option<StackFrame>) -> Self {
        Self {
            position: position.clone(),
            frame_name: Some(frame_name.clone()),
            parent: parent.clone().map(|s| Box::new(s)),
        }
    }

    fn from_position(position: &CodeRange) -> Self {
        Self {
            position: position.clone(),
            frame_name: None,
            parent: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum InternalRunError {
    Error(Cow<'static, str>),
    TodoError(Cow<'static, str>),
    // could be NameError, but we don't always have the name
    Undefined(Cow<'static, str>),
}

impl fmt::Display for InternalRunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Error(s) => write!(f, "Internal Error: {s}"),
            Self::TodoError(s) => write!(f, "Internal Error TODO: {s}"),
            Self::Undefined(s) => match s.is_empty() {
                true => write!(f, "Internal Error: accessing undefined object"),
                false => write!(f, "Internal Error: accessing undefined object `{s}`"),
            },
        }
    }
}

#[derive(Debug, Clone)]
pub enum RunError {
    Internal(InternalRunError),
    Exc(ExceptionRaise),
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Internal(s) => write!(f, "{s}"),
            Self::Exc(s) => write!(f, "{s}"),
        }
    }
}

impl From<InternalRunError> for RunError {
    fn from(internal_error: InternalRunError) -> Self {
        Self::Internal(internal_error)
    }
}

impl From<ExceptionRaise> for RunError {
    fn from(exc: ExceptionRaise) -> Self {
        Self::Exc(exc)
    }
}

impl From<Exception> for RunError {
    fn from(exc: Exception) -> Self {
        Self::Exc(exc.into())
    }
}
