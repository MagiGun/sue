# Sue Writer parity with WordCraft and LibreOffice Writer

Everything WordCraft and LibreOffice Writer can do, and where Sue Writer
stands on each. The list comes from WordCraft's feature catalog
(`crates/engine/src/catalog.rs`, 520 commands modeled on Word's ribbon)
and LibreOffice Writer's menus (`sw/uiconfig/swriter/menubar/menubar.xml`),
its help and its source under `sw/`. Both live under `Projects/refs`.

- **Source**: `W` WordCraft, `L` LibreOffice Writer, `WL` both.
- **Sue**: `done`, `partial` (some of it works; the note says what is
  missing) or `missing`.
- Where a feature belongs to all of Sue (spelling, equations, tables,
  shapes) the note names the shared crate it lives in or should.

Rows turn `done` as features land; the order below is by area, the order
of work is by how much people miss each one (the plan at the end).

## Text and character formatting

| Feature | Source | Sue | Notes |
| --- | --- | --- | --- |
| Bold, italic, underline, strikethrough | WL | done | |
| Superscript, subscript | WL | done | |
| Small capitals | WL | done | |
| All capitals | WL | missing | |
| Hidden text | WL | missing | |
| Double underline, underline color | WL | missing | |
| Double strikethrough | W | missing | |
| Overline | L | missing | |
| Font family | WL | done | Installed fonts from `sue_wm::fonts` |
| Font size, grow and shrink | WL | partial | Size box; no grow and shrink commands |
| Text color | WL | done | Five swatches; no full color picker |
| Highlight | WL | done | |
| Character shading and border | WL | missing | |
| Outline, shadow, emboss, engrave | WL | missing | |
| Character spacing, scale, position | WL | missing | |
| Change case (upper, lower, sentence, title, toggle) | WL | done | Shared `sue_type::case` |
| Clear direct formatting | WL | done | |
| Format painter (clone formatting) | WL | missing | |
| Inline code | WL | done | |
| Phonetic guide (ruby) | WL | missing | |
| Enclosed characters, half and full width, kana | WL | missing | |
| Language of text | WL | missing | |
| Character styles (Emphasis, Strong, Source) | WL | missing | |
| Spotlight direct and style formatting | L | missing | |

## Paragraphs and styles

| Feature | Source | Sue | Notes |
| --- | --- | --- | --- |
| Align left, center, right, justify | WL | done | |
| Distributed alignment, vertical alignment | WL | missing | |
| Line spacing (single, 1.15, 1.5, double) | WL | done | |
| Exact and at least line spacing | WL | missing | |
| Space before and after, increase and decrease | WL | partial | In styles; no commands |
| Left, right, first-line and hanging indent | WL | partial | Marks and styles; no dialog or ruler |
| Tabs and tab stops, leaders | WL | missing | |
| Paragraph borders and shading | WL | missing | |
| Horizontal line | WL | missing | |
| Keep with next, keep lines together | WL | partial | Keep with next in styles and exports only |
| Page break before | WL | done | |
| Widow and orphan control | WL | missing | |
| Outline level | WL | partial | From headings only |
| Drop caps | WL | missing | |
| Right-to-left paragraphs | WL | missing | |
| Asian typography | WL | missing | |
| Paragraph dialog | WL | missing | |
| Show formatting marks | WL | missing | |
| Paragraph styles: apply | WL | done | Style box and Ctrl+1 to 3 style shortcuts |
| Style sheets (standard, manuscript, screenplay) | - | done | Sue's own |
| Create, modify, delete a style | WL | missing | |
| Update style to match, new style from selection | WL | missing | |
| Styles pane, style inspector | WL | missing | |
| Load styles from another document | L | missing | |
| Themes, style sets, document fonts and colors | WL | missing | |
| Sort paragraphs | WL | missing | |
| Move paragraph up and down | WL | missing | |

## Lists

| Feature | Source | Sue | Notes |
| --- | --- | --- | --- |
| Bulleted list | WL | done | |
| Numbered list | WL | done | |
| Nested levels, increase and decrease | WL | done | |
| Lettered and roman numbering | WL | missing | |
| Restart and continue numbering, start value | WL | missing | |
| Custom bullets and number formats | WL | missing | |
| Multilevel list library, chapter numbering | WL | missing | |
| Outline numbering of headings | WL | missing | |
| Checklists | W | missing | |

## Pages, sections and columns

| Feature | Source | Sue | Notes |
| --- | --- | --- | --- |
| Page size and margins | WL | done | Project settings, from page presets |
| Orientation | WL | partial | Through the page size only |
| Page break | WL | done | |
| Line break inside a paragraph | WL | missing | |
| Column break | WL | missing | |
| Page columns | WL | missing | |
| Sections with their own columns and protection | WL | missing | |
| Page styles (first, left, right), landscape page in a portrait document | L | missing | |
| Mirrored margins, gutter | WL | missing | |
| Page color, page border | WL | missing | |
| Watermark | WL | missing | |
| Line numbering | WL | missing | |
| Hyphenation | WL | missing | |
| Document grid, baseline grid | WL | missing | |
| Cover page, title page | WL | partial | Compile adds a title page |
| Blank page | WL | missing | |
| Print layout and web (draft) layout | WL | done | Page view, draft view |
| Book view (facing pages), multiple pages per row | WL | missing | |
| Zoom: in, out, 100%, fit width | WL | done | |
| Zoom: whole page, multiple pages | WL | missing | |
| Rulers | WL | missing | |
| Navigation pane (headings) | WL | partial | The binder; no in-document heading outline pane |
| Focus mode, full screen | WL | done | Composition mode, typewriter scrolling |
| Read mode | W | missing | |
| Split window, new window | WL | missing | |

## Headers, footers and footnotes

| Feature | Source | Sue | Notes |
| --- | --- | --- | --- |
| Header | WL | done | Left, middle and right parts with fields; edited in a dialog, not on the page |
| Footer | WL | done | As the header; the page number by default |
| Page numbers | WL | done | Live fields in Word, OpenDocument and RTF |
| Page number format (roman, letters), start at | WL | missing | |
| Different first page, different odd and even | WL | partial | Different first page; not odd and even |
| Footnotes | WL | done | |
| Endnotes | WL | missing | |
| Footnote and endnote options (numbering, place) | WL | missing | |
| Next and previous footnote | WL | missing | |

## Tables

| Feature | Source | Sue | Notes |
| --- | --- | --- | --- |
| Insert a table | WL | done | Shared `sue_model::Table`, an entity shown by an `embed` mark |
| Insert and delete rows and columns | WL | done | Tab in the last cell adds a row |
| Select cell, row, column, table | WL | missing | |
| Merge and split cells | WL | missing | |
| Split and merge tables | WL | missing | |
| Column width, row height, distribute, autofit | WL | partial | Rows fit their text; widths are read from the table (Word import) but cannot be dragged yet |
| Header rows repeated on each page | WL | done | On screen, in PDF, Word, OpenDocument, RTF |
| Borders and shading, table styles and autoformat | WL | partial | Every table ruled, its heading row shaded; no styles yet |
| Cell alignment, margins and text direction | WL | missing | |
| Convert text to table and table to text | WL | done | Tabs, else commas |
| Sort | WL | missing | |
| Formulas and number formats | WL | missing | |
| Nested tables | WL | missing | |
| Draw table, eraser | W | missing | |
| Spreadsheet object | WL | missing | |

## Images, shapes and frames

| Feature | Source | Sue | Notes |
| --- | --- | --- | --- |
| Insert picture from a file | WL | done | |
| Picture from Sue Paint (Send To) | - | done | Sue's own |
| Resize, crop, rotate pictures | WL | missing | |
| Picture filters and corrections | WL | missing | Paint's filters could serve |
| Alt text | WL | missing | |
| Anchoring (as character, to paragraph, to page) | WL | partial | As character only |
| Text wrap (none, square, tight, through) | WL | missing | |
| Arrange (front, back, forward, backward), group | WL | missing | |
| Shapes (lines, basic, arrows, callouts, flowchart) | WL | missing | |
| Text boxes and frames, linked frames | WL | missing | |
| WordArt and Fontwork | WL | missing | |
| Charts | WL | missing | |
| SmartArt | W | missing | |
| Drawing and ink | W | missing | |
| Captions | WL | missing | |
| Image maps | L | missing | |
| QR and bar codes | L | missing | |
| Audio and video | L | missing | |
| OLE and other objects | WL | missing | |
| Signature line, digital signatures | WL | missing | |

## Fields and cross references

| Feature | Source | Sue | Notes |
| --- | --- | --- | --- |
| Date and time | WL | partial | Inserts today's date as text, not a field |
| Page number, page count fields | WL | partial | In headers and footers; not in the body text |
| Document properties (title, author, word count) | WL | missing | |
| Field dialog, update fields, show field codes | WL | missing | |
| Bookmarks | WL | missing | |
| Cross-references (headings, bookmarks, figures, notes) | WL | missing | |
| Hyperlinks | WL | done | |
| Remove and edit link | WL | partial | Edit through Insert Link again |
| Variables, conditional text, hidden paragraphs | L | missing | |
| Document properties dialog | WL | partial | Title and author in project settings |

## Table of contents, index and bibliography

| Feature | Source | Sue | Notes |
| --- | --- | --- | --- |
| Table of contents | WL | done | Live in the text with pages; Word's own TOC field, OpenDocument's index |
| Update and remove table of contents | WL | done | Always up to date; deleted like any object |
| Alphabetical index, mark entry | WL | missing | |
| Table of figures, tables, objects | WL | missing | |
| Table of authorities | W | missing | |
| Citations and bibliography (APA, MLA, Chicago, IEEE) | WL | missing | |
| Manage sources, bibliography database | WL | missing | |

## Track changes, comments, compare and merge

| Feature | Source | Sue | Notes |
| --- | --- | --- | --- |
| Comments: add, reply, resolve, delete | WL | missing | `comment` mark kind exists in `sue-model` |
| Next and previous comment, show comments | WL | missing | |
| Track changes: record | WL | missing | |
| Show markup, accept and reject one or all | WL | missing | |
| Next and previous change, reviewing pane | WL | missing | |
| Protect tracked changes | WL | missing | |
| Compare documents | WL | missing | Snapshots can be compared by eye only |
| Combine (merge) documents | WL | missing | |
| Version history | WL | partial | Snapshots of a document, restorable |
| Restrict editing, read-only sections | WL | missing | |
| Redaction | L | missing | |

## Mail merge, envelopes and labels

| Feature | Source | Sue | Notes |
| --- | --- | --- | --- |
| Mail merge fields from a data source | WL | missing | Shared `Table` as the source |
| Recipients: select, edit, filter, sort | WL | missing | |
| Address block, greeting line, rules | WL | missing | |
| Preview results, next and previous record | WL | missing | |
| Finish and merge to documents or print | WL | missing | |
| Envelopes and labels | WL | missing | |

## Spelling, grammar, thesaurus and autocorrect

| Feature | Source | Sue | Notes |
| --- | --- | --- | --- |
| Spelling as you type | WL | missing | Belongs to all of Sue |
| Spelling dialog, add to dictionary, ignore | WL | missing | |
| Grammar checking | WL | missing | |
| Thesaurus | WL | missing | |
| Hyphenation dictionaries | WL | missing | |
| Autocorrect: replace as you type | WL | missing | |
| Smart quotes | WL | done | |
| Autocomplete of words | L | missing | |
| Autoformat (lists, rules, headings while typing) | WL | missing | |
| Word count, character count | WL | done | Status bar and statistics |
| Chinese and Hangul conversion | L | missing | |
| Read aloud | W | missing | |
| Translate | W | missing | |

## Find and replace

| Feature | Source | Sue | Notes |
| --- | --- | --- | --- |
| Find, find next and previous | WL | done | |
| Replace, replace all | WL | done | |
| Match case, whole words | WL | done | |
| Across the project | - | done | Sue's own |
| Regular expressions | WL | done | Shared `sue_type::find`, `$1` in replacements |
| Find formatting and styles | WL | missing | |
| Find all, select all matches | WL | missing | |
| Go to page | WL | missing | |
| Similarity search | L | missing | |

## Master documents, templates and forms

| Feature | Source | Sue | Notes |
| --- | --- | --- | --- |
| Master documents and subdocuments | L | partial | A project's binder plays this part |
| Templates: new from, save as | WL | partial | Built-in project templates; no save as template |
| Text from file (insert document) | WL | missing | |
| AutoText, Quick Parts, building blocks | WL | missing | |
| Form controls (check box, list, text field) | WL | missing | |
| Content controls | WL | missing | |
| Form design mode | WL | missing | |

## Macros and automation

| Feature | Source | Sue | Notes |
| --- | --- | --- | --- |
| Record and run macros | WL | missing | |
| Scripting | L | missing | |
| Command palette | W | done | Sue's palette lists Writer's commands |
| Repeat last command | WL | missing | |
| Customize keyboard | WL | done | Sue's Settings, Shortcuts |
| Customize menus and toolbars | WL | missing | |

## Math

| Feature | Source | Sue | Notes |
| --- | --- | --- | --- |
| Equation editor | WL | missing | `math` mark kind exists; editor should be shared |
| LaTeX input | W | missing | |
| Structures (fractions, scripts, radicals, matrices) | WL | missing | |
| Inline and display equations, numbering | WL | missing | |
| Symbols and special characters | WL | missing | |
| Non-breaking space, hyphen, soft hyphen | WL | missing | |

## File formats

| Feature | Source | Sue | Notes |
| --- | --- | --- | --- |
| Sue project (.sue) | - | done | |
| DOCX export | WL | done | Styles, lists, footnotes, links, pictures |
| DOCX import | WL | partial | Text, styles, lists, footnotes, links; no pictures or tables |
| ODT export | L | done | |
| ODT import | L | missing | |
| RTF export | WL | done | |
| RTF import | WL | missing | |
| PDF export | WL | done | Typeset with the project's fonts |
| PDF import | L | missing | |
| EPUB export | L | done | |
| HTML export and import | WL | done | |
| Markdown export and import | WL | done | |
| Plain text export and import | WL | done | |
| DOC (Word 97) | WL | missing | |
| FODT (flat OpenDocument) | L | missing | |
| DocBook, XHTML | L | missing | |
| Export as image | W | missing | |
| Password protection and encryption | WL | missing | |

## Printing and output

| Feature | Source | Sue | Notes |
| --- | --- | --- | --- |
| Print | WL | partial | Through PDF; no system print dialog |
| Print preview | WL | partial | Page view |
| Print markup, comments | W | missing | |
| Brochure and multiple pages per sheet | L | missing | |
| Compile (one book from many documents) | - | done | Sue's own |

## Accessibility and review

| Feature | Source | Sue | Notes |
| --- | --- | --- | --- |
| Accessibility check | WL | missing | |
| Alt text | WL | missing | |
| Tagged PDF | L | missing | |
| Document inspector, compatibility check | W | missing | |
| Autosave and recovery | WL | done | Sue's project autosave |

## Editing and navigation

| Feature | Source | Sue | Notes |
| --- | --- | --- | --- |
| Undo and redo | WL | done | Sue's one history |
| Undo list | W | missing | |
| Cut, copy, paste | WL | done | |
| Paste as text, paste special | WL | missing | |
| Word, line, paragraph and document movement | WL | done | |
| Select word, sentence, paragraph | WL | partial | Double and triple click; no sentence |
| Block (column) selection | WL | missing | |
| Delete previous and next word | WL | done | |
| Insert Unicode by code (Alt+X) | WL | missing | |
| Spike | W | missing | |
| Go back to the last edit | W | missing | |

## Sue's own, beyond both

| Feature | Sue | Notes |
| --- | --- | --- |
| Binder of chapters and scenes | done | |
| Corkboard and outliner | done | |
| Synopses, notes, labels, status, targets | done | |
| Snapshots | done | |
| Writing statistics | done | |

## The plan

Work goes by how much people miss each feature:

1. Tables in the text, on the shared `Table` model.
2. Headers and footers with page number and page count fields.
3. Regular expressions in find and replace, and change case.
4. Table of contents in the text.
5. Comments, then track changes with accept and reject.
6. Spelling as you type, shared by all of Sue.
7. Bookmarks and cross-references, endnotes.
8. ODT and RTF import, pictures and tables in DOCX import.
9. More character and paragraph formats (all caps, double underline,
   tabs, borders, keep together), custom styles.
10. The equation editor, shared by all of Sue.

Then the rest, row by row.
