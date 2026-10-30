# Sue Writer templates

Every file here is a template that Sue Writer offers for a new document:
adding a template is adding a file, and no list in the code names them.
Sue also reads the same kind of file from the templates folder in Sue's
data folder (`templates/writer`), with Word (`.docx`, `.dotx`),
OpenDocument (`.odt`, `.ott`), Markdown, HTML and plain text files beside
them. File, Save as Template writes the open document there.

A `.template` file is plain text. Lines that start with `#` are comments.

```text
name: tr:template-letter
about: tr:template-letter-about
group: template
order: 20
sheet: standard
page: auto
margin: 72
binder: false

draft:
  document tr:letter
    body {date}
    body.right tr:your-name
```

The header, one `key: value` a line:

| Key | Meaning |
| --- | --- |
| `name`, `about` | What people read. `tr:key` looks `key` up in the language database (Sue Writer's messages); other text is shown as written. |
| `group` | `template` (a card on the start page) or `preset` (in the Presets list). |
| `order` | Where it sorts in its group, smallest first. |
| `sheet` | The style sheet: `standard`, `manuscript` or `screenplay`. |
| `page` | A page size by name (`Letter`, `A4`, `A5`, ...), or `auto` for the size people use where they live. |
| `orientation` | `portrait` (the default) or `landscape`, the long edge across. |
| `margin` | The page margins in points. |
| `font` | `body`, `mono` or a font family: the body font. |
| `size` | The body size in points. |
| `header`, `footer` | Running heads: left, middle and right parted by `\|`, with `{page}`, `{pages}`, `{title}`, `{author}` and `{date}`. |
| `different-first` | `true`: no header or footer on the first page. |
| `target` | The words the whole work aims at. |
| `session-target` | The words to write in one sitting. |
| `binder` | `true` shows the binder of chapters and notes; `false` is one document, as a word processor shows a file. |

Then `draft:` (the work itself) and `research:` (notes beside it), each a
tree indented by two spaces:

- `folder <name>` and `document <name>` make a folder or a document;
- `synopsis <text>` under either gives it a synopsis;
- any other line in a document is a paragraph: its style key (`body`,
  `title`, `subtitle`, `heading1` to `heading6`, `quote`, `code`,
  `caption`, `chapter`, `scene`, `character`, `dialogue`, ...) and its
  text. Add `.bullet` or `.number` for a list item, `.center`, `.right`
  or `.justify` to align it, `.bold` or `.italic` for its whole text, and
  `.break` to start it on a new page, as in `heading1.center.break`.

Text may hold `{date}`, filled with the day the document is made.
