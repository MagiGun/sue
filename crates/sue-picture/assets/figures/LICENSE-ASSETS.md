# The figure assets and where they come from

`human.sfig` in this folder is the mesh Sue Paint's 3D drawing figures,
hands and head are made of. It was converted by Sue's own script,
`convert_makehuman.py` (also here), from asset files of the MakeHuman
project:

- the base mesh, `makehuman/data/3dobjs/base.obj` (the body and the eyes);
- the default skeleton and its skin weights,
  `makehuman/data/rigs/default.mhskel` and `default_weights.mhw`;
- morph targets from `makehuman/data/targets/` (macrodetails, breast,
  head, measure, torso and armslegs; for the face: forehead, cheek,
  chin, eyes, eyebrows, nose, mouth and ears; for the expressions: the
  units in expression/units).

Source: <https://github.com/makehumancommunity/makehuman>

## Licence

MakeHuman's `LICENSE.md` says of its bundled assets (the base mesh and
proxies, targets and modifiers, textures, clothes, poses and
expressions): "These assets have been released under CC0 1.0 Universal."
The full legal text is MakeHuman's `LICENSE.ASSETS.md`, the Creative
Commons CC0 1.0 Universal dedication:
<https://creativecommons.org/publicdomain/zero/1.0/legalcode>

The files named above carry the notice themselves, for example
`default_weights.mhw`: "license": "CC0", "(c) 2021 Data Collection AB,
Joel Palmius, Jonas Hauquier"; the target files: "This asset was
explicitly released as CC0 in september 2020", original copyright
Manuel Bastioni, transferred to the MakeHuman team in 2016.

MakeHuman's program code is under the AGPL. None of it is used here:
the converter reads only the asset files, and is Sue's own.

## Credit

Thanks to the MakeHuman team (Manuel Bastioni, Joel Palmius, Jonas
Hauquier, Data Collection AB and the MakeHuman community) for giving
these assets to the public domain.

What Sue adds on top is Sue's own and under Sue's licence: the joints
people pose, the rest stance, the hand and head figures cut from the
body, the guide lines, the veins, tendons and knuckles of the hands,
every pose, the drawn style (Sue's own mix of the face targets), the
expressions (Sue's own mixes of the expression units) and the irises
of the eyes.
