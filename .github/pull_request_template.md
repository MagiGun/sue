## What changed and why

<!-- In plain words: what does this change, and why is it needed? -->

## How it was checked

- [ ] `cargo fmt --all --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] `cargo run -p sue -- --self-check`
- [ ] Tried in the app (say what you did)

## Backbone

- [ ] This does not touch `sue-core`, `sue-model`, `sue-io` or `sue-platform`
- [ ] Or it does, and it has tests for what it promises (round trips, undo
      and redo, the invariants it relies on)

## Docs

- [ ] `HUMANZ.md` and `HUMANS.md` say how this works and how to use it
- [ ] `AGENTS.md` and `CLAUDE.md` are still identical
