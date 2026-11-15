# Sue Paint Auto Action files

Sue Paint keeps its Auto Actions in `auto_actions.txt` in its data
folder, and Export Set (the Auto Action palette's menu) writes one set to
a `.sueactions` file that anyone can import again with Import Set. Both
are the same plain text format, written in English whatever language Sue
shows, so a set can be made, read and shared without Sue.

## The format

```text
sue-auto-actions 1
set Inking
in-use
[Clean Up the Sketch]
id clean-up-the-sketch
step layer.duplicate
step off filter.grayscale
step layer.toggle.draft
[Flat Copy]
id flat-copy
off
step layer.merge_visible
```

- The first line is `sue-auto-actions 1`: the format and its version.
- `set <name>` starts a set. A file may hold several sets; an exported
  file holds one. `in-use` right after a `set` line marks the set Sue
  Paint shows (only in `auto_actions.txt`, only when there are several).
- `[<name>]` starts an action of the set above it. A name is one line
  without square brackets.
- `id <id>` is what the action's key is kept under: the key setting
  names the command `auto.run.<id>`. It is made of the name once (lower
  case letters, digits and dashes) and stays when the action is renamed,
  so a renamed action keeps its key. When two actions have the same id,
  Sue Paint adds `-2`, `-3` to the later ones as it reads the file.
- `off` turns the action off: it shows with its box unchecked and cannot
  be played until it is turned on again.
- `step <command>` is one step: the name of a Sue Paint command, the same
  names Shortcut Settings and the command bar use (`layer.new`,
  `filter.grayscale`, `view.fit`). `step off <command>` is a step turned
  off: it stays in the action but is skipped when the action plays.
- Lines Sue Paint does not know are left out when it reads the file.

## Older files

Sue Paint 0.0.75 wrote `auto_actions.txt` without the first line: each
action a `[name]` line and its command names under it, one a line. Such a
file still reads, as one set named Sue Paint Actions, and is written in
the format above the next time it is kept.

## What is recorded

While Record is on, every command flagged as recordable that runs is
kept as a step, wherever it was run from: a menu, a key, the command bar,
Quick Access or a palette's button. Commands that only change how Sue
Paint looks (Window, palettes) are not recorded.
