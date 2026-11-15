# Body shape files

A body shape material is how a 3D drawing figure is built, kept to use
again (Tool Property, Body shape, Register body shape). Sue Paint keeps
each as a plain text file ending in `.body` in the `bodies` folder of
its materials location, and lists them in the Material palette under
Body shapes. Anyone can write one by hand, share it, or put one there.

## Format

UTF-8 text, one `key=value` per line. Unknown keys and lines without
`=` are skipped, so a later Sue may add keys. A key that is missing
keeps the figure's own value; a file with no key Sue reads is refused.

| Key | Value | Range |
| --- | --- | --- |
| `build` | `male` or `female` | anything else reads as `male` |
| `height` | centimeters | 50 to 250 |
| `heads` | head to body ratio | 2 to 10 |
| `shape` | `v1;` then five `thin to fat,muscle` pairs joined by `;`: full body, head, upper body, arms, legs | each value -1 to 1 |
| `follow` | `true` when the ratio follows the height | `true` or `false` |

Values outside the range are brought into it.

## Example

```text
build=female
height=158
heads=6.5
shape=v1;0.2,0;0,0;0,0;0,0.4;0,0.4
follow=false
```

The same values live on the figure in the document as the properties
`figure.build`, `figure.height`, `figure.heads`, `figure.shape` and
`figure.follow_height` of its model entity.
