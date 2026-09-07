#![deny(missing_docs)]
//! Runtime contracts for uniflowed app execution.

mod contract;
mod host;
mod kind;
pub mod permissions;

pub use crate::contract::{CapabilityList, HostList, RuntimeContract};
pub use crate::host::{HOSTS, HostSupport, SupportLevel};
pub use crate::kind::{
    EventLoopModel, JavaScriptEngine, NativeIoModel, RuntimeCapability, RuntimeHost,
    RuntimeLanguage, RuntimeStandard,
};
pub use crate::permissions::{Permission, PermissionError, Permissions, ToolchainAccess};

#[cfg(test)]
mod tests;
