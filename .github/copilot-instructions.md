# Instructions for GitHub Copilot

Read and follow AGENTS.md at the root of this repository before you change
anything. It holds the rules that bind every contributor, human or AI: one
data model in the Arena, every change a reversible transaction, formats and
operators registered at runtime, versioned files with round trip tests,
shared buffers, OS code only in crates/sue-platform, no unsafe code, tests
with every backbone change, and the Git rules (commit only when asked, never
push unless asked, never force push).

Before you say a change is done, run all four with zero warnings:
cargo fmt --all --check, cargo clippy --workspace --all-targets -- -D warnings,
cargo test --workspace, and cargo run -p sue -- --self-check.

Docs, interface text and comments use no em dashes, en dashes or middle dots.
CONTRIBUTING.md says where each kind of change goes.
