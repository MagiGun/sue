## Sue Writer (crates/sue-writer).

applet-name = Sue Writer
applet-summary = Write documents, novels, scripts and papers, then publish them as PDF, Word, EPUB and more.

## compile.rs

word-document = Word document
opendocument-text = OpenDocument text
epub-e-book = EPUB e-book
web-page = Web page
rich-text-format = Rich Text Format
markdown = Markdown
plain-text = Plain text
there-nothing-compile-every-document-left = there is nothing to compile: every document is left out
sue-writer-cannot-write-files = Sue Writer cannot write { $other } files
sue-writer-cannot-read-files-yet = Sue Writer cannot read .{ $other } files yet

## menus.rs

file = File
new-project-ellipsis = New Project...
new-document = New Document
new-folder = New Folder
import-a-document-ellipsis = Import a Document...
compile-ellipsis = Compile...
export-as = Export As
project-settings-ellipsis = Project Settings...
back-to-the-hub = Back to the Hub
edit = Edit
undo = Undo
redo = Redo
find-ellipsis = Find...
replace-ellipsis = Replace...
view = View
text = Text
corkboard = Corkboard
outliner = Outliner
page-view = Page View
draft-view = Draft View
binder = Binder
inspector = Inspector
composition-mode = Composition Mode
typewriter-scrolling = Typewriter Scrolling
zoom = Zoom
zoom-in = Zoom In
zoom-out = Zoom Out
actual-size = Actual Size
fit-to-window = Fit to Window
insert = Insert
page-break = Page Break
scene-break = Scene Break
footnote-ellipsis = Footnote...
link-ellipsis = Link...
picture-ellipsis = Picture...
todays-date = Today's Date
format = Format
bold = Bold
italic = Italic
underline = Underline
strikethrough = Strikethrough
superscript = Superscript
subscript = Subscript
small-capitals = Small Capitals
all-capitals = All Capitals
font-size = Font Size
grow-font = Grow Font
shrink-font = Shrink Font
formatting-mark = Formatting Mark
non-breaking-space = Non-breaking Space
non-breaking-hyphen = Non-breaking Hyphen
soft-hyphen = Soft Hyphen
narrow-non-breaking-space = Narrow Non-breaking Space
zero-width-space = Zero Width Space
code = Code
text-color = Text Color
automatic = Automatic
red = Red
blue = Blue
green = Green
gray = Gray
highlight = Highlight
yellow = Yellow
pink = Pink
none = None
align-text = Align Text
left = Left
center = Center
right = Right
justify = Justify
line-spacing = Line Spacing
single = Single
double = Double
lists = Lists
bulleted-list = Bulleted List
numbered-list = Numbered List
increase-indent = Increase Indent
decrease-indent = Decrease Indent
clear-direct-formatting = Clear Direct Formatting
project = Project
split-at-selection = Split at Selection
merge-with-next = Merge With Next
duplicate = Duplicate
move-to-trash = Move to Trash
empty-trash = Empty Trash
take-snapshot = Take Snapshot
include-in-compile = Include in Compile
status = Status
no-status = No Status
label = Label
no-label = No Label
writing-statistics-ellipsis = Writing Statistics...
help = Help
keyboard-shortcuts = Keyboard Shortcuts

## project.rs

character-sketch = Character Sketch
name = Name
role-in-the-story = Role in the story
what-they-want = What they want
what-stands-in-the-way = What stands in the way
how-they-change = How they change
copy-sheet-each-character = Copy this sheet for each character.
setting-sketch = Setting Sketch
place = Place
sights-sounds-and-smells = Sights, sounds and smells
why-it-matters = Why it matters
copy-sheet-each-place = Copy this sheet for each place.
chapter-1 = Chapter 1
scene-1 = Scene 1
what-happens-line-two = What happens, in a line or two.
chapter-2 = Chapter 2
characters = Characters
places = Places
notes = Notes
story = Story
act-one = Act One
int-location-day = INT. LOCATION - DAY
action = Action.
act-two = Act Two
act-three = Act Three
introduction = Introduction
part-one = Part One
conclusion = Conclusion
sources = Sources
paper = Paper
title = Title
author = Author
discussion = Discussion
untitled-poem = Untitled Poem
untitled = Untitled
ideas = Ideas
letter = Letter
your-name = Your Name
your-address = Your Address
date = Date
dear = Dear ,
kind-regards = Kind regards,
report = Report
summary = Summary
background = Background
findings = Findings
next-steps = Next Steps
document = Document
new = New { $title }
manuscript = Manuscript
research = Research
trash = Trash
move = Move
copy = { $entity } (copy)
split = Split
merge = Merge
restore-snapshot = Restore Snapshot
before-restoring = Before restoring

## studio.rs

made = Made { $title }
open-a-project-first = Open a project first
put-picture-text = Put the picture in the text
added-to-research = Added to Research
imported = Imported
could-not-read = Could not read { $path }: { $error }
import = Import
imported-2 = Imported { $name }
nothing-to-compile = Nothing to compile
kib = { $name }, { $value } KiB
could-not-write = Could not write { $path }: { $error }
export-as-2 = Export as { $name }
import-a-document = Import a Document
raise-or-lower = Raise or Lower
paragraph-style = Paragraph Style
alignment = Alignment
list = List
list-level = List Level
indent = Indent
select-word-footnote-follows = Select the word the footnote follows
insert-a-picture = Insert a Picture
snapshot = Snapshot { $taken }
took-a-snapshot = Took a snapshot
not-found = Not found
replace-all = Replace All
replaced = Replaced { $total }
heading-1 = Heading 1
heading-2 = Heading 2
heading-3 = Heading 3
body-text = Body Text
find-next = Find Next
find-previous = Find Previous
writing = Writing
sue-writer-new-project-ellipsis = Sue Writer: New Project...
sue-writer-import-a-document-ellipsis = Sue Writer: Import a Document...
sue-writer-compile-ellipsis = Sue Writer: Compile...
sue-writer-export-as-pdf-ellipsis = Sue Writer: Export as PDF...
sue-writer-composition-mode = Sue Writer: Composition Mode
sue-writer-writing-statistics = Sue Writer: Writing Statistics
sue-writer-keyboard-shortcuts = Sue Writer: Keyboard Shortcuts
picture = Picture
could-not-read-the-picture = Could not read the picture: { $error }

## ui.rs

menus = Menus
find = Find
find-and-replace-ctrl-f = Find and replace (Ctrl+F)
undo-ctrl-z = Undo (Ctrl+Z)
redo-ctrl-shift-z = Redo (Ctrl+Shift+Z)
bold-ctrl-b = Bold (Ctrl+B)
italic-ctrl-i = Italic (Ctrl+I)
underline-ctrl-u = Underline (Ctrl+U)
alignment-next = Alignment (next: { $next })
align-left-ctrl-l = Align left (Ctrl+L)
center-ctrl-e = Center (Ctrl+E)
align-right-ctrl-r = Align right (Ctrl+R)
justify-ctrl-j = Justify (Ctrl+J)
bulleted-list-ctrl-shift-l = Bulleted list (Ctrl+Shift+L)
numbered-list-2 = Numbered list
decrease-indent-2 = Decrease indent
increase-indent-2 = Increase indent
search-fonts = Search fonts
finding-installed-fonts-ellipsis = Finding installed fonts...
styles-font = Style's font
font = Font
size = Size
folder = + Folder
new-folder-ctrl-shift-n = New folder (Ctrl+Shift+N)
text-2 = + Text
new-document-ctrl-n = New document (Ctrl+N)
rename = Rename
leave-out-of-compile = Leave Out of Compile
open-in-sue-paint = Open in Sue Paint
nothing-write-here-yet = Nothing to write in here yet.
start-something-new-open-project-sue = Start something new, or open a project from this Sue project.
new-2 = New
in-this-project = In this project
words = { $name }    { $words } words
word-markdown-html-plain-text = Word, Markdown, HTML or plain text
no-cards-here-yet-new-documents = No cards here yet. New documents and folders appear as cards.
synopsis = Synopsis
words-3 = Words
compile = Compile
what-happens-here-line-two = What happens here, in a line or two
include-in-compile-2 = Include in compile
word-target = Word target
keywords = Keywords
comma-separated = comma, separated
statistics = Statistics
words-characters-paragraphs = { $words } words, { $count } characters, { $paragraphs } paragraphs
selected-words = Selected: { $value } words
about-minutes-to-read = About { $value } minutes to read
notes-for-yourself = Notes for yourself
snapshots = Snapshots
take = Take
keep-copy-document-as-now-ctrl = Keep a copy of this document as it is now (Ctrl+5)
none-yet-take-one-before-big = None yet. Take one before a big rewrite.
snapshot-words = { $saved }  ({ $words } words)
restore = Restore
put-version-back-what-there-now = Put this version back (what is there now is kept as a snapshot)
outline = Outline
of-words = { $selected } of { $doc_words } words
draft-percent = Draft { $draft } / { $target } ({ $value }%)
draft-words = Draft { $draft } words
today = Today { $session } / { $goal }
today-2 = Today { $session }
page-of = Page { $value } of { $value2 }
zoom-in-2 = Zoom in
fit-percent = Fit { $value }%
fit-page-window-show-size = Fit the page to the window, or show it at its size
zoom-out-2 = Zoom out
draft = Draft
page = Page
previous-shift-f3 = Previous (Shift+F3)
next-f3 = Next (F3)
replace-with = Replace with
replace = Replace
match-case = Match case
whole-words = Whole words
whole-project = Whole project
close-esc = Close (Esc)
words-today-esc-to-leave = { $words } words    |    today { $session }    |    Esc to leave
new-project = New Project
project-settings = Project Settings
link = Link
footnote = Footnote
writing-statistics = Writing Statistics
create = Create
cancel = Cancel
what = What
the-whole-manuscript = The whole manuscript
what-is-open-now = What is open now
titles = Titles
folders-become-chapter-titles = Folders become chapter titles
folders-and-documents-both = Folders and documents both
no-titles-just-the-text = No titles, just the text
between-documents = Between documents
a-scene-break = A scene break (#)
a-new-page = A new page
nothing = Nothing
a-title-page-first = A title page first
each-chapter-new-page = Each chapter on a new page
export-ellipsis = Export...
apply = Apply
web-address = Web address
remove-link = Remove Link
note-printed-end = The note, printed at the end
add-footnote = Add Footnote
close = Close
change-shortcuts-ellipsis = Change Shortcuts...
settings-keymaps-any-number-keys-each = Settings, Keymaps: any number of keys for each
styles = Styles
standard = Standard
margins-inches = Margins (inches)
body-font = Body font
heading-font = Heading font
body-size-pt = Body size (pt)
running-head = Running head
manuscript-target-words = Manuscript target (words)
words-a-day = Words a day
still-finding-installed-fonts-ellipsis = Still finding installed fonts...
this-sitting = This sitting
the-manuscript = The manuscript
everything-with-research = Everything, with research
target = Target
words-percent-there = { $target } words, { $value }% there
days-you-write-show-here-once = Days you write on show here once a sitting ends.
words-by-day = Words by day

## Statuses and labels (projects keep them in English; these are their names)

status-to-do = To Do
status-first-draft = First Draft
status-revised-draft = Revised Draft
status-final-draft = Final Draft
status-done = Done
label-concept = Concept
label-chapter = Chapter
label-scene = Scene
label-idea = Idea
label-character = Character
label-location = Location
label-research = Research

## Kinds of new projects

template-blank = Blank Document
template-blank-about = One document on pages, as a word processor opens.
template-novel = Novel
template-novel-about = Chapters of scenes, with research for characters and places.
template-short-story = Short Story
template-short-story-about = One story in manuscript format.
template-screenplay = Screenplay
template-screenplay-about = Scenes in screenplay format, Enter and Tab moving between elements.
template-nonfiction = Non-fiction Book
template-nonfiction-about = Parts and chapters with notes and sources.
template-essay = Essay or Paper
template-essay-about = A titled paper with headings and notes.
template-poetry = Poetry Collection
template-poetry-about = A poem to a document.
template-letter = Letter
template-letter-about = A letter on one page.
template-report = Report
template-report-about = A report with a title, a summary and sections.

## Word counts

word-count = { $count ->
    [one] { $count } word
   *[other] { $count } words
}
word-count-change = { $change } words

## menus.rs

table = Table
insert-table-ellipsis = Insert Table...
rows-above = Rows Above
rows-below = Rows Below
columns-before = Columns Before
columns-after = Columns After
delete = Delete
rows = Rows
columns = Columns
heading-row = Heading Row
convert = Convert
text-to-table = Text to Table
table-to-text = Table to Text

## studio.rs

click-in-a-table-first = Click in a table first

## ui.rs

heading-row-repeated-each-page = Heading row, repeated on each page
insert-table = Insert Table

## menus.rs

header-and-footer-ellipsis = Header and Footer...
table-of-contents-ellipsis = Table of Contents...
change-case = Change Case

## ui.rs

regular-expressions = Regular expressions
patterns-such-as-d-chapter-1 = Patterns such as \d+ or ^Chapter; $1 in the replacement puts back what a group caught
header-and-footer = Header and Footer
table-of-contents = Table of Contents
heading-levels = Heading levels
lists-headings-their-pages-keeps-up = It lists the headings with their pages and keeps up by itself.
middle = Middle
header = Header
footer = Footer
different-first-page-none-title-page = Different first page (none on a title page)

## menus.rs

review = Review
new-comment-ellipsis = New Comment...
previous-comment = Previous Comment
next-comment = Next Comment
resolve-comment = Resolve Comment
delete-comment = Delete Comment
delete-all-comments = Delete All Comments
comments = Comments

## studio.rs

select-words-comment = Select the words to comment on
reply = Reply
reopen-comment = Reopen Comment
no-comments = No comments
put-caret-comments-words-first = Put the caret in a comment's words first

## ui.rs

no-comments-select-some-words-choose = No comments. Select some words and choose Review, New Comment.
reopen = Reopen
resolve = Resolve
show-resolved-comments = Show resolved comments
new-comment = New Comment
by = By { $value }
your-comment = Your comment
add-comment = Add Comment

## Spelling

check-spelling = Check Spelling While Typing
next-misspelling = Next Misspelling
no-misspellings = No misspelled words
add-to-dictionary = Add to Dictionary
added-to-dictionary = Added { $word } to the project's dictionary

## Track changes

track-changes = Track Changes
record-changes = Record Changes
stop-recording-changes = Stop Recording Changes
previous-change = Previous Change
next-change = Next Change
accept-change = Accept Change
reject-change = Reject Change
accept-all = Accept All
reject-all = Reject All
accept-all-changes = Accept All Changes
reject-all-changes = Reject All Changes
recording-changes = Recording changes
no-longer-recording-changes = No longer recording changes
no-changes = No changes
no-changes-here = No changes here
accepted-count = Accepted { $count }
rejected-count = Rejected { $count }
