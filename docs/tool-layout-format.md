# Tool layout files (`.suetools`)

A tool layout says how Sue Paint's tool palette is arranged: which tools
it shows, in what order, which sub tool groups each tool has, and which
sub tools each group holds. It is a plain text file a person can write by
hand, share, install and remove. Sue Paint's own arrangement is such a
file too (`crates/sue-picture/src/tools/builtin.suetools`), so it is the
best example to start from.

A layout holds no brush settings. It names sub tools; their settings stay
with the sub tools themselves (Sue Paint's built-in ones, or ones you made
or installed). That is why a layout can be shared without sharing
anything else, and why loading one never changes how a brush paints.

## The file

- UTF-8 text, one entry per line. Lines may end in LF or CRLF.
- Fields are separated by one tab character. Names cannot hold a tab or a
  line break (Sue writes a space in their place).
- Empty lines and lines that start with `#` are notes and are skipped.
- The first other line is the header `sue-tool-layout 1`. A file with a
  higher number is read as far as this version understands it, and Sue
  says so.

```text
sue-tool-layout 1
# My palette: inks first.
tool	Inks	pen	P
group	Favourites
sub	pen	G-pen
sub	pen	Turnip pen
sub	brush	Gouache
group	Markers
sub	pen	Fineliner
tool	Liquify	blend	J
group	Liquify
sub	blend	Liquify
```

### Lines

| Line | Fields | Meaning |
| --- | --- | --- |
| `tool` | name, tool id, key | A button of the tool palette. The name is what people read. The tool id says which icon it shows (see below). The key is one character that picks it, or `-` for none; tools that share a key take turns, in the order of the file. |
| `group` | name | A sub tool group (a tab of the Sub Tool palette) of the tool above. |
| `sub` | tool id, name | A sub tool, in the group above, found by its tool and its name. A `sub` line straight after `tool` starts a group named as the tool. |

A tool's sub tools need not all be of the same tool: Sue Paint's own Fill
tool holds the Figure tool's Lasso fill, as Clip Studio Paint Ver. 4
does. What a sub tool does always follows its own tool id.

### Tool ids

These never change and are never translated:

`zoom`, `hand`, `operation`, `move_layer`, `selection`, `auto_select`,
`eyedropper`, `pen`, `pencil`, `brush`, `airbrush`, `decoration`,
`eraser`, `blend`, `fill`, `gradient`, `figure`, `frame_border`, `ruler`,
`text`, `balloon`, `correct_line`.

### Names

Sue Paint's own tools, groups and sub tools are written in English, as in
the built-in file, and show in the language Sue is set to. A name you
choose stays as you wrote it. Sub tool names Sue Paint has changed are
still found by their old names (`Real G-Pen` is now `Grainy G-pen`,
`Milli pen` is now `Fineliner`, and the four `Liquify: ...` sub tools are
now the one `Liquify` sub tool with modes).

## What Sue does with a layout

- Each `sub` line takes the first sub tool of that tool and name that no
  earlier line took. A name it cannot find is reported, and the rest of
  the file still applies.
- A tool whose sub tools were all missing is left out, and reported.
- Every sub tool the layout does not name keeps a place: it goes to the
  end of its own tool's button, so loading a layout never loses a sub
  tool, and sub tools added in a newer Sue still show.

## For pack makers

A layout may arrange only what Sue Paint and the person already have. Do
not copy another program's tool names, icons or brush images into a
layout or a pack; name things in your own words.
