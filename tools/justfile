set dotenv-load := false

default:
    just --list

check:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    cargo test --workspace --all-features
    cargo bench --workspace --all-features --no-run

test:
    cargo test --workspace --all-features

ci:
    cargo test --workspace --all-features
