# Gradient set files

Sue Paint's Gradient Map (Edit, Tonal Correction, Gradient Map, and the
Gradient Map correction layer) and its Gradient tool (the set row under
the bar of stops in Tool Property) offer gradients from sets. Sue ships one
set of its own; every other set is a plain text file that people make,
share, install and remove. This page says what such a file holds, so a
set can be written by hand or by another program.

## Where they live

Each set is one file ending in `.suegradients` in the `gradient_sets`
folder of Sue Paint's data folder. Sue reads every such file when a
gradient map dialog or the Gradient tool's Tool Property first shows
them. To install a set, put its file there; to remove one, delete its
file. Adding a gradient while Sue's own set is shown, or Keep in My
Gradients under the Gradient tool's bar, makes
`my_gradients.suegradients`.

## The format

UTF-8 text, one item a line:

```text
sue-gradient-set 1
name = Evening Skies
gradient = Warm evening = 0,0 1,1|0@1a1430ff@c;0.5@96406eff@c;1@ffecbeff@c
gradient = Drawing colors = 0,0 1,1|0@000000ff@m;1@ffffffff@s
```

- The first line that is not empty and not a comment (`#`) starts with
  `sue-gradient-set`; the number after it is the format's version.
- `name = ` gives the set's name, shown in the Gradient Set list.
- Each `gradient = <name> = <gradient>` line is one gradient, in the
  order the list shows them. A name holds no ` = `.
- Lines Sue does not know are skipped, so a file written by a newer Sue
  still opens.

A gradient is its mixing curve, a `|`, then its stops separated by `;`:

- The mixing curve bends brightness before the stops are looked up: its
  points as `input,output` pairs from 0 to 1, separated by spaces.
  `0,0 1,1` leaves brightness as it is.
- A stop is `place@rrggbbaa@kind`: its place from 0 (black) to 1 (white),
  its color and opacity as eight hex digits, and where its color comes
  from: `c` its own color, `m` the main drawing color, `s` the sub
  drawing color, `d` the color brushes draw with (the main color, or
  the sub color while it is the one in front), each when the gradient
  is used; the hex digits keep the color it showed last. A Sue from
  before 0.0.8 does not know `d` and skips a gradient that has it.
- A gradient has 2 to 16 stops; they are kept in order of place.

A stop's opacity mixes the gradient's color over the picture's own: a
stop at `00` opacity leaves the picture as it was there.

## In documents

A Gradient Map correction layer keeps its gradient in its setting as
`gradient:` followed by the same gradient text. A gradient of exactly two
opaque stops of their own color at 0 and 1 with a straight mixing curve
is written in the older form `gradient:<rrggbb>,<rrggbb>`, which every
Sue reads.

A gradient layer keeps its gradient the same way: its stops as
`gradient.stops` (the gradient text above, with every color filled in
when the layer was made), its shape as `gradient.shape` (`linear`,
`circle` or `ellipse`), its edge process as `gradient.edge` (`extend`,
`repeat`, `mirror` or `clip`) and an ellipse's short axis against its
long one as `gradient.aspect`. It also keeps its first and last colors
and whether it is round in the properties an older Sue reads, and a
gradient layer without `gradient.stops` runs from those two colors.

A set holds colors only. The shape, edge process and the rest belong
to the Gradient tool's sub tool, which keeps them with its other
settings.

## Legal

Sue ships no set copied from another program. Sets people share are
their own work; a set made from another program's gradients is that
program's, so check its licence before sharing it.
