use super::*;

mod build;
mod diagnostic;
mod reachability;
mod resolve;

fn client(path: impl Into<Utf8PathBuf>) -> RscModuleInput {
    RscModuleInput::new(path, ModuleEnvironment::Client)
}

fn server(path: impl Into<Utf8PathBuf>) -> RscModuleInput {
    RscModuleInput::new(path, ModuleEnvironment::Server)
}

fn actions(path: impl Into<Utf8PathBuf>) -> RscModuleInput {
    RscModuleInput::new(path, ModuleEnvironment::ServerActions)
}
