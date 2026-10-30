# Working alongside other agents

Several AI agents (Claude, Codex, Copilot, Cursor, Gemini, Aider and
others) may work on Sue at the same time, each in its own worktree and on
its own branch. This guide says how they find out what the others are
doing, keep out of each other's way, and settle it when they cannot.
AGENTS.md (Rule 9) makes it binding.

## The board

The board is a folder inside the repository's shared Git directory, so
every worktree of the same clone sees it, nothing on it is ever
committed, and no checkout or branch switch touches it:

```bash
git rev-parse --git-common-dir
```

That prints the Git directory (for example `C:/Users/fn/Projects/Sue/.git`);
the board is the `sue-agents` folder in it. Make it if it is missing.

```text
sue-agents/
  claims/<agent>.md        one file per agent, written only by that agent
  messages/<time>-<from>-to-<to>.md   one file per message, never edited
```

`<agent>` is a short name that stays the same for the whole piece of
work, made of the tool and the branch, such as `claude-csp-menus` or
`codex-languages`. `<time>` is `YYYYMMDD-HHMMSS` in local time, so the
messages sort in the order they were sent. `<to>` is an agent's name or
`all`.

Agents in the cloud (another clone) cannot see the board. They say what
they are doing in their branch name and pull request description instead,
and agents here read those before touching the same area.

## A claim

```markdown
agent: claude-csp-menus
tool: Claude Code
branch: csp-menus
worktree: .claude/worktrees/csp-menus
updated: 2026-10-30 15:20
doing: Sue Paint's menus and palettes, to match Clip Studio Paint

own:
- crates/sue-paint/src/menus.rs
- crates/sue-paint/src/dock.rs

shared:
- crates/sue-lang/locales/en/paint.ftl
- CHANGELOG.md
```

- `own` lists the files or folders this agent is changing and others must
  leave alone until the claim goes. Claim no more than the work needs.
- `shared` lists files several agents add to (see below), so the others
  know to expect additions there.
- Change `updated` whenever the claim changes, and at least once a day
  while the work goes on.
- Delete the claim when the branch is merged or the work is dropped.

## The rules

1. **Read before you start.** Read every claim and every message to you
   or to `all` that is newer than your last look. Then write or refresh
   your own claim, before changing any file.
2. **Stay out of what others own.** Never edit a path in another agent's
   `own` list. When your work needs it, send that agent a message saying
   what you need and why, work on the parts that do not overlap, and wait
   for an answer.
3. **Shared files are append only.** `CHANGELOG.md`, `HUMANS.md`,
   `HUMANZ.md`, the language files in `crates/sue-lang/locales/`, the
   settings files `default.suerice` and `rice.ftl`, and `Cargo.lock` may be
   changed by anyone, but only by adding lines (or replacing your own).
   Never reorder, reword or remove another agent's lines.
4. **Your checkout only.** Work only in your own worktree. Never switch
   branches, commit, merge, reset, stash or clean in a checkout another
   agent or the human is using; the main checkout counts as theirs unless
   the human gave it to you. `git worktree list` shows which branch every
   checkout holds.
5. **Never move a branch you do not own.** Before you fast-forward or
   otherwise move a branch (`main` included), check with
   `git worktree list` that no other checkout has it out, and move it with
   `git update-ref refs/heads/<branch> <new> <old>` (which fails if the
   branch moved meanwhile), never with `git merge` inside someone else's
   checkout. Pushing stays the human's call (Rule 8).
6. **Say what you did to others.** When anything you did touched another
   agent's branch, files or checkout, even by mistake, send them a message
   at once saying what happened, what you changed back and what is left.

## Settling a clash

Work through these in order and stop at the first that settles it.

1. **Overlapping claims.** The older claim (its first `updated`) keeps the
   paths. The newer one narrows its claim and sends a message about what
   it still needs.
2. **A file both need.** The owner makes the change the other needs, or
   hands the file over by moving it from its `own` list to the other's
   claim, saying so in a message. A message goes unanswered after a day
   when its sender says it is waiting on it; the sender then asks the
   human instead of taking the file.
3. **Git conflicts when merging.** The agent whose branch merges second
   rebases onto the current `main` in its own worktree and resolves the
   conflicts:
   - in shared files, keep both sides' lines;
   - in code, keep both behaviours, run the definition of done in
     AGENTS.md, and send the other agent a message saying how it was
     resolved.
4. **Real disagreement.** When the two changes cannot both stand (the same
   function made to do different things, two designs for one feature),
   stop, write both sides in a message to `all`, and ask the human to
   choose. Do not decide it by merging first.
5. **A stale claim.** A claim not updated for a day whose branch has no
   new commits may be stale. Message its agent; if there is no answer,
   ask the human before taking any of its paths.
