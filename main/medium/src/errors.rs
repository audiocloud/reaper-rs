use derive_more::*;
use std::fmt::{Debug, Display};

/// An error which can occur when executing a REAPER function.
///
/// In most cases this is an error reported by REAPER itself. When using some of the convenience
/// functions, the error could also originate from *reaper-rs*.
///
/// The error message is not very specific most of the time because REAPER functions usually don't
/// give information about the cause of the error.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Display, Error)]
#[display(fmt = "REAPER function failed: {}", message)]
pub struct ReaperFunctionError {
    message: &'static str,
}

impl ReaperFunctionError {
    pub(crate) const fn new(message: &'static str) -> ReaperFunctionError {
        ReaperFunctionError { message }
    }

    /// Returns the error message.
    pub const fn message(&self) -> &'static str {
        self.message
    }
}

impl From<ReaperFunctionError> for &'static str {
    fn from(e: ReaperFunctionError) -> Self {
        e.message
    }
}

impl From<&'static str> for ReaperFunctionError {
    fn from(e: &'static str) -> Self {
        Self::new(e)
    }
}

pub(crate) type ReaperFunctionResult<T> = Result<T, ReaperFunctionError>;

/// An error which can occur when converting from a type with a greater value range to one with a
/// smaller one.
///
/// This error is caused by *reaper-rs*, not by REAPER itself.
#[derive(Debug, Clone, Eq, PartialEq, Display)]
#[display(fmt = "conversion from value [{}] failed: {}", value, message)]
pub struct TryFromGreaterError<V> {
    message: &'static str,
    value: V,
}

impl<V: Copy> TryFromGreaterError<V> {
    pub(crate) fn new(message: &'static str, value: V) -> TryFromGreaterError<V> {
        TryFromGreaterError { message, value }
    }
}

impl<R: Copy + Display + Debug> std::error::Error for TryFromGreaterError<R> {}
