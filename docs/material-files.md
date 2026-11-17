# Material settings files

A picture registered with Edit, Register Material, Image is a PNG file
in the materials location: `materials/` for image materials, `tips/`
for brush tips. What the Register Material dialog says of it beyond the
picture is kept beside it in a small text file of the same name,
`<name>.suematerial`, so it travels with the picture when a person
copies or shares the two.

```text
sue-material 1
scale	0.5
tile	1
order	top
paper	1
```

The first line is `sue-material 1`. Each line after it is a key and a
value told apart by a tab:

| Key | Values | Meaning |
| --- | --- | --- |
| `scale` | a number, 0.01 to 10 | Its size when pasted against its own (Scaling ratio) |
| `tile` | `0` or `1` | Pasted, it is repeated over the whole canvas (Tiling) |
| `order` | `above-current` or `top` | Where its layer goes in the Layer palette |
| `paper` | `0` or `1` | Brushes may lay it as their paper texture |

A key Sue does not know is kept as it is when the file is written again.
A material without the file is pasted as before: once, in the middle of
the canvas, above the current layer. The search tags a person gives a
material are kept with the material library in the materials location,
as the Material palette keeps them.
