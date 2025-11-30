use crate::{exceptions::ExcType, object::Object, run::RunResult};

/// Type for method call arguments.
///
/// Uses specific variants for common cases (0-2 arguments).
/// Most Python method calls have at most 2 arguments, so this optimization
/// eliminates the Vec heap allocation overhead for the vast majority of calls.
#[derive(Debug)]
pub enum Args {
    Zero,
    One(Object),
    Two(Object, Object),
    Many(Vec<Object>),
    // TODO kwarg types
}

impl Args {
    /// Checks that zero arguments were passed.
    pub fn check_zero_args<'c>(&self, name: &str) -> RunResult<'c, ()> {
        match self {
            Self::Zero => Ok(()),
            _ => Err(ExcType::type_error_no_args(name, self.count())),
        }
    }

    /// Checks that exactly one argument was passed, returning it.
    pub fn get_one_arg<'c>(self, name: &str) -> RunResult<'c, Object> {
        match self {
            Self::One(a) => Ok(a),
            _ => Err(ExcType::type_error_arg_count(name, 1, self.count())),
        }
    }

    /// Checks that exactly two arguments were passed, returning them as a tuple.
    pub fn get_two_args<'c>(self, name: &str) -> RunResult<'c, (Object, Object)> {
        match self {
            Self::Two(a1, a2) => Ok((a1, a2)),
            _ => Err(ExcType::type_error_arg_count(name, 2, self.count())),
        }
    }

    /// Checks that one or two arguments were passed, returning them as a tuple.
    pub fn get_one_two_args<'c>(self, name: &str) -> RunResult<'c, (Object, Option<Object>)> {
        match self {
            Self::One(a) => Ok((a, None)),
            Self::Two(a1, a2) => Ok((a1, Some(a2))),
            Self::Zero => Err(ExcType::type_error_at_least(name, 1, self.count())),
            Self::Many(_) => Err(ExcType::type_error_at_most(name, 2, self.count())),
        }
    }

    /// Returns the number of arguments.
    fn count(&self) -> usize {
        match self {
            Self::Zero => 0,
            Self::One(_) => 1,
            Self::Two(_, _) => 2,
            Self::Many(v) => v.len(),
        }
    }
}
