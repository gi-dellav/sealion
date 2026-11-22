use std::{
    any::Any,
    cell::RefCell,
    panic::{AssertUnwindSafe, catch_unwind},
};

use crate::Error;

#[derive(Debug)]
pub(crate) struct Trap {
    message: String,
}

impl Trap {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl From<String> for Trap {
    fn from(message: String) -> Self {
        Self { message }
    }
}

impl From<&str> for Trap {
    fn from(message: &str) -> Self {
        Self::new(message)
    }
}

#[derive(Debug)]
pub(crate) enum UnwindReason {
    Trap(Trap),
    Panic(Box<dyn Any + Send>),
}

impl UnwindReason {
    pub(crate) fn into_error(self) -> Error {
        match self {
            Self::Trap(trap) => Error::Trap(trap.message),
            Self::Panic(payload) => Error::Trap(panic_message(&payload)),
        }
    }
}

thread_local! {
    static PENDING: RefCell<Option<UnwindReason>> = const { RefCell::new(None) };
}

pub(crate) fn record_unwind(reason: UnwindReason) {
    PENDING.with(|pending| {
        let mut pending = pending.borrow_mut();
        if pending.is_none() {
            *pending = Some(reason);
        }
    });
}

pub(crate) fn take_unwind() -> Option<UnwindReason> {
    PENDING.with(|pending| pending.borrow_mut().take())
}

pub(crate) fn has_unwind() -> bool {
    PENDING.with(|pending| pending.borrow().is_some())
}

pub(crate) fn catch_host_call<F, R>(func: F) -> Result<R, UnwindReason>
where
    F: FnOnce() -> Result<R, Trap>,
{
    match catch_unwind(AssertUnwindSafe(func)) {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(trap)) => Err(UnwindReason::Trap(trap)),
        Err(payload) => Err(UnwindReason::Panic(payload)),
    }
}

fn panic_message(payload: &Box<dyn Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "host function panicked".to_string()
    }
}
