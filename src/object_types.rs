use std::borrow::Cow;
use std::fmt;

use crate::exceptions::{exc_err_fmt, internal_err, ExcType, InternalRunError, SimpleException};
use crate::heap::Heap;
use crate::parse_error::{ParseError, ParseResult};
use crate::run::RunResult;
use crate::{HeapData, Object};

// TODO use strum
#[derive(Debug, Clone)]
pub(crate) enum FunctionTypes {
    Print,
    Len,
}

#[derive(Debug, Clone)]
pub(crate) enum Types {
    BuiltinFunction(FunctionTypes),
    Exceptions(ExcType),
    Range,
}

impl fmt::Display for Types {
    // TODO replace with a strum
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BuiltinFunction(FunctionTypes::Print) => write!(f, "print"),
            Self::BuiltinFunction(FunctionTypes::Len) => write!(f, "len"),
            Self::Exceptions(exc) => write!(f, "{exc}"),
            Self::Range => write!(f, "range"),
        }
    }
}

impl Types {
    // TODO replace with a strum
    pub fn find(name: &str) -> ParseResult<'static, Self> {
        match name {
            "print" => Ok(Self::BuiltinFunction(FunctionTypes::Print)),
            "len" => Ok(Self::BuiltinFunction(FunctionTypes::Len)),
            "ValueError" => Ok(Self::Exceptions(ExcType::ValueError)),
            "TypeError" => Ok(Self::Exceptions(ExcType::TypeError)),
            "NameError" => Ok(Self::Exceptions(ExcType::NameError)),
            "range" => Ok(Self::Range),
            _ => Err(ParseError::Internal(format!("unknown builtin: `{name}`").into())),
        }
    }

    pub fn call_function<'c, 'd>(&self, heap: &Heap, args: Vec<Cow<'d, Object>>) -> RunResult<'c, Cow<'d, Object>> {
        match self {
            Self::BuiltinFunction(FunctionTypes::Print) => {
                for (i, object) in args.iter().enumerate() {
                    if i == 0 {
                        print!("{object}");
                    } else {
                        print!(" {object}");
                    }
                }
                println!();
                Ok(Cow::Owned(Object::None))
            }
            Self::BuiltinFunction(FunctionTypes::Len) => {
                if args.len() != 1 {
                    return exc_err_fmt!(ExcType::TypeError; "len() takes exactly exactly one argument ({} given)", args.len());
                }
                let object = &args[0];
                match object.len(heap) {
                    Some(len) => Ok(Cow::Owned(Object::Int(len as i64))),
                    None => exc_err_fmt!(ExcType::TypeError; "Object of type {} has no len()", object),
                }
            }
            Self::Exceptions(exc_type) => {
                if let Some(first) = args.first() {
                    if args.len() == 1 {
                        if let Object::Ref(object_id) = first.as_ref() {
                            if let HeapData::Str(s) = heap.get(*object_id) {
                                return Ok(Cow::Owned(Object::Exc(SimpleException::new(
                                    *exc_type,
                                    Some(s.to_owned().into()),
                                ))));
                            }
                        }
                    }
                    internal_err!(InternalRunError::TodoError; "Exceptions can only be called with zero or one string argument")
                } else {
                    Ok(Cow::Owned(Object::Exc(SimpleException::new(*exc_type, None))))
                }
            }
            Self::Range => {
                if args.len() == 1 {
                    let object = &args[0];
                    let size = object.as_int()?;
                    Ok(Cow::Owned(Object::Range(size)))
                } else {
                    internal_err!(InternalRunError::TodoError; "range() takes exactly one argument")
                }
            }
        }
    }
}
