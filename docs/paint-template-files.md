# Sue Paint templates

A template is a set of layers kept to start pictures with: frame
borders, guides, a layered sketch, a paper texture on a layer of its own.
File, New's Template row starts a new canvas with a template's layers,
and a template in the Material palette (the Templates folder) goes onto
the open canvas, dragged there or with Paste to Canvas, as one undo step
above the current layer.

## Sue Paint's own

Sue Paint brings a frame template for each frame layout it knows (One
panel, Two rows, Three rows, Four rows (4-koma), Two by two, Manga page
(2, 1, 2), Two by three): each puts a frame border layer of that layout
on the page, with the page's margin and gutters.

## Your own, as files

Edit, Register Material, Template keeps every layer of the open canvas
but its paper as a template named as you type. It is a layered `.clip`
file in the `templates` folder of the materials location (Settings, Sue
Paint, Saving, the materials folder; Sue Paint's own data folder when
that is empty), so a template can be shared as a file and opened by any
program that opens a `.clip`. Put a `.clip` file there yourself and it
is a template too.

When a template goes onto a canvas, its layers keep their place from
the top left corner of the canvas; its paper layer stays out, as the
canvas has its own. Delete removes a template you added from the
Material palette; Sue Paint's own stay.
