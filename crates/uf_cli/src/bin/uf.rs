#![cfg_attr(test, allow(clippy::disallowed_macros))]

#[global_allocator]
static GLOBAL_ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> std::process::ExitCode {
    uf_cli::main()
}
