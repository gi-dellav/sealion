#![allow(dead_code)]

mod bridge;
mod func;
mod linker;
mod slots;
mod store;
mod trap;
mod types;
mod vm;

#[cfg(test)]
mod tests;

pub(crate) use types::{FuncType, RegisteredType};

#[cfg(test)]
pub(crate) use func::{Caller, HostFunc, IntoFunc, WithoutCaller};
#[cfg(test)]
pub(crate) use linker::{Import, Instance, Linker, Module};
#[cfg(test)]
pub(crate) use slots::{ValRaw, ValueRet, ValueTyList, ValueType};
#[cfg(test)]
pub(crate) use store::Store;
#[cfg(test)]
pub(crate) use trap::{Trap, has_unwind};
#[cfg(test)]
pub(crate) use vm::{
    VMArrayCallFunction, VMArrayCallHostFuncContext, VMContext, VMFuncRef, VMFunctionImport,
    VMOpaqueContext,
};

#[derive(Default)]
pub(crate) struct RuntimeState {
    pub(crate) types: types::TypeRegistry,
}
