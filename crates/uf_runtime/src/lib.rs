#![deny(missing_docs)]
//! Runtime contracts for uniflowed app execution.

mod contract;
mod kind;

pub use crate::contract::{CapabilityList, HostList, RuntimeContract};
pub use crate::kind::{
    EventLoopModel, JavaScriptEngine, NativeIoModel, RuntimeCapability, RuntimeHost,
    RuntimeLanguage, RuntimeStandard,
};

#[cfg(test)]
mod tests;
