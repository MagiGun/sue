# Sue Paint's Selection launcher file

The Selection launcher is the small bar of buttons Sue Paint shows under a
selection. Which buttons it has, and in what order, is kept in a plain
text file that a person can edit by hand, copy to another computer or
share with others.

## Where it is

`selection_launcher.txt` in Sue Paint's data folder (the `paint` folder of
Sue's data, beside `command_bar.txt` and `layout.txt`). Sue writes it each
time the buttons change in Selection Launcher Settings (the last button of
the launcher). Without the file the launcher has the buttons Sue ships.

## What it holds

One line per button, left to right:

```text
# Sue Paint selection launcher
select.none
edit.crop
select.invert
-
edit.transform.scale
layer.new.tone
```

- A button is the name of a Sue Paint command, as Keyboard Shortcuts and
  Auto Actions name them (`select.invert`, `edit.fill`). The button shows
  the command's own icon (from the icon set in use, so icon packs change
  it too), its name and its key in the tooltip, and is grayed while the
  command cannot run.
- A line holding only `-` is a separator between two groups of buttons.
- Lines that start with `#` and blank lines are notes and are skipped.
- Selection Launcher Settings is always the last button. It is not in the
  file and cannot be taken off, so the launcher can always be changed
  again.
- A name this Sue does not know (a command of a newer Sue, or a typing
  mistake) is kept in the file and simply not shown, so a file shared
  from a newer Sue loses nothing.

## The buttons Sue ships

Deselect, Crop, Invert Selected Area, Expand Selected Area, Shrink
Selected Area, Delete, Delete Outside Selection, Cut and Paste, Copy and
Paste, Scale/Rotate, Fill and New Tone: Clip Studio Paint's set, in its
order. Restore Default in Selection Launcher Settings puts them back.
