"""Converts MakeHuman's base mesh, rig and morph targets (CC0 assets) to
Sue's compact figure asset, `human.sfig`.

Usage: python -I convert_makehuman.py <makehuman/data> <out.sfig>

Sue's own script: it reads only MakeHuman's asset files (base.obj,
default.mhskel, default_weights.mhw and .target files), never its program
code. The result holds the body and eye vertices, the quads, skin weights
merged onto Sue's joints, the joints, and named morph targets with the
change each makes to every joint. See LICENSE-ASSETS.md beside it.

Joints whose names start with "+" are helpers, not posed by people: the
skin that MakeHuman hangs on a bone between two of Sue's joints (the cap
of the shoulder, the side of the hip), which Sue turns part of the way
with the limb so the joint keeps its volume.

Layout, little endian, the whole file zlib compressed:
  "SUEFIG1\\0", u32 vertices, u32 quads, u32 joints, u32 morphs,
  f32 meters per unit,
  vertices x 3 f32, quads x 4 u16,
  vertices x (4 u8 joint, 4 u8 weight),
  joints x (u8 length, name, i16 parent, 3 f32 head, 3 f32 tail),
  morphs x (u8 length, name, f32 step, u32 count, count x u16 vertex,
            count x 3 i16 change in steps, joints x 6 f32 head and tail
            change).
"""

import json
import struct
import sys
import zlib

BODY = 13380
EYES = (14598, 14742)

# Sue's joints: name, parent, where its head and tail are (a MakeHuman
# bone and which end), and the MakeHuman bones whose weights it takes
# (a trailing * takes the bone and everything hanging from it).
def side_joints(side, s):
    fingers = []
    for name, number in (("index", 2), ("middle", 3), ("ring", 4), ("little", 5), ("thumb", 1)):
        label = f"{side} {name} finger" if name != "thumb" else f"{side} thumb"
        parent = f"{side} hand"
        for part, at in (("base", 1), ("middle", 2), ("tip", 3)):
            bone = f"finger{number}-{at}.{s}"
            fingers.append((f"{label} {part}", parent, (bone, "head"), (bone, "tail"), [bone]))
            parent = f"{label} {part}"
    return [
        (f"{side} shoulder", "Chest", (f"clavicle.{s}", "head"), (f"upperarm01.{s}", "head"),
         [f"clavicle.{s}"]),
        (f"{side} upper arm", f"{side} shoulder", (f"upperarm01.{s}", "head"), (f"lowerarm01.{s}", "head"),
         [f"upperarm01.{s}", f"upperarm02.{s}"]),
        (f"{side} forearm", f"{side} upper arm", (f"lowerarm01.{s}", "head"), (f"wrist.{s}", "head"),
         [f"lowerarm01.{s}", f"lowerarm02.{s}"]),
        (f"{side} hand", f"{side} forearm", (f"wrist.{s}", "head"), (f"finger3-1.{s}", "head"),
         [f"wrist.{s}"] + [f"metacarpal{n}.{s}" for n in range(1, 5)]),
        (f"{side} thigh", "Hips", (f"upperleg01.{s}", "head"), (f"lowerleg01.{s}", "head"),
         [f"upperleg01.{s}", f"upperleg02.{s}"]),
        (f"{side} shin", f"{side} thigh", (f"lowerleg01.{s}", "head"), (f"foot.{s}", "head"),
         [f"lowerleg01.{s}", f"lowerleg02.{s}"]),
        (f"{side} foot", f"{side} shin", (f"foot.{s}", "head"), (f"toe2-1.{s}", "head"), [f"foot.{s}"]),
        (f"{side} toes", f"{side} foot", (f"toe2-1.{s}", "head"), (f"toe2-3.{s}", "tail"),
         [f"toe{n}-1.{s}*" for n in range(1, 6)]),
    ] + fingers


def helpers(side, s):
    return [
        (f"+{side} shoulder cap", f"{side} shoulder", (f"upperarm01.{s}", "head"), (f"lowerarm01.{s}", "head"),
         [f"shoulder01.{s}"]),
        (f"+{side} hip cap", "Hips", (f"upperleg01.{s}", "head"), (f"lowerleg01.{s}", "head"),
         [f"pelvis.{s}"]),
    ]


JOINTS = [
    ("Hips", None, ("spine05", "head"), ("spine05", "tail"), ["root", "spine05"]),
    ("Spine", "Hips", ("spine04", "head"), ("spine02", "head"), ["spine04", "spine03"]),
    ("Chest", "Spine", ("spine02", "head"), ("neck01", "head"), ["spine02", "spine01", "breast.L", "breast.R"]),
    ("Neck", "Chest", ("neck01", "head"), ("head", "head"), ["neck01", "neck02", "neck03"]),
    ("Head", "Neck", ("head", "head"), ("head", "tail"), ["head*"]),
] + side_joints("Left", "L") + side_joints("Right", "R") + [
    # After the head, so they take their own weights back from it.
    ("Jaw", "Head", ("jaw", "head"), ("jaw", "tail"), ["jaw*"]),
    ("Left eye", "Head", ("eye.L", "head"), ("eye.L", "tail"), ["eye.L*"]),
    ("Right eye", "Head", ("eye.R", "head"), ("eye.R", "tail"), ["eye.R*"]),
] + helpers("Left", "L") + helpers("Right", "R")

SIDES = ("l-", "r-")


def both(name):
    return [f"armslegs/{side}{name}" for side in SIDES]


def pair(*stems):
    """The targets that raise and lower a slider: `stem-incr`, `stem-decr`."""
    return ([f"{stem}-incr" for stem in stems], [f"{stem}-decr" for stem in stems])


# Sliders: name, the targets added at +1, the targets added at -1.
SLIDERS = [
    ("head_width", *pair("head/head-scale-horiz")),
    ("head_length", *pair("head/head-scale-vert")),
    ("head_fat", *pair("head/head-fat")),
    ("neck_length", *pair("measure/measure-neck-height")),
    ("neck_width", *pair("measure/measure-neck-circ")),
    ("shoulders", *pair("measure/measure-shoulder-dist")),
    ("chest", *pair("measure/measure-bust-circ")),
    ("torso_length", *pair("measure/measure-napetowaist-dist")),
    ("torso_muscle", *pair("torso/torso-muscle-pectoral", "torso/torso-muscle-dorsi")),
    ("torso_fat", *pair("torso/torso-scale-depth", "torso/torso-scale-horiz")),
    ("waist", *pair("measure/measure-waist-circ")),
    ("hips", *pair("measure/measure-hips-circ")),
    ("arm_length", *pair("measure/measure-upperarm-length", "measure/measure-lowerarm-length")),
    ("arm_width", *pair("measure/measure-upperarm-circ", *both("lowerarm-scale-horiz"), *both("lowerarm-scale-depth"))),
    ("arm_fat", *pair(*both("upperarm-fat"), *both("lowerarm-fat"))),
    ("arm_muscle", *pair(*both("upperarm-muscle"), *both("lowerarm-muscle"), *both("upperarm-shoulder-muscle"))),
    ("hand_size", *pair(*both("hand-scale"))),
    ("finger_length", *pair(*both("hand-fingers-length"))),
    ("finger_width", *pair(*both("hand-fingers-diameter"))),
    ("leg_length", *pair("measure/measure-upperleg-height", "measure/measure-lowerleg-height")),
    ("leg_width", *pair("measure/measure-thigh-circ", "measure/measure-calf-circ")),
    ("leg_fat", *pair(*both("upperleg-fat"), *both("lowerleg-fat"))),
    ("leg_muscle", *pair(*both("upperleg-muscle"), *both("lowerleg-muscle"))),
    ("foot_size", *pair(*both("foot-scale"))),
]


def sides(folder, name):
    return [f"{folder}/{side}{name}" for side in SIDES]


# The face's sliders: name, the targets added at +1, the targets added
# at -1 (none for a slider that only goes one way).
FACE = [
    ("face_depth", *pair("head/head-scale-depth")),
    ("face_age", *pair("head/head-age")),
    ("forehead", *pair("forehead/forehead-scale-vert")),
    ("temples", *pair("forehead/forehead-temple")),
    ("cheeks", *pair(*sides("cheek", "cheek-volume"))),
    ("cheekbones", *pair(*sides("cheek", "cheek-bones"))),
    ("jaw", *pair("chin/chin-bones")),
    ("chin", *pair("chin/chin-height")),
    ("chin_width", *pair("chin/chin-width")),
    ("chin_forward", *pair("chin/chin-prominent")),
    ("eye_size", *pair(*sides("eyes", "eye-scale"))),
    ("eye_spacing", sides("eyes", "eye-trans-out"), sides("eyes", "eye-trans-in")),
    ("eye_height", sides("eyes", "eye-trans-up"), sides("eyes", "eye-trans-down")),
    ("eye_slant", sides("eyes", "eye-corner2-up"), sides("eyes", "eye-corner2-down")),
    ("brows", ["eyebrows/eyebrows-trans-up"], ["eyebrows/eyebrows-trans-down"]),
    ("nose_length", *pair("nose/nose-scale-vert")),
    ("nose_width", *pair("nose/nose-scale-horiz")),
    ("nose_depth", *pair("nose/nose-scale-depth")),
    ("nose_height", ["nose/nose-trans-up"], ["nose/nose-trans-down"]),
    ("nose_bridge", ["nose/nose-curve-convex"], ["nose/nose-curve-concave"]),
    ("mouth_width", *pair("mouth/mouth-scale-horiz")),
    ("mouth_height", ["mouth/mouth-trans-up"], ["mouth/mouth-trans-down"]),
    ("lips", *pair("mouth/mouth-scale-vert")),
    ("ear_size", *pair(*sides("ears", "ear-scale"))),
    ("ear_wing", *pair(*sides("ears", "ear-wing"))),
]

# Sue's own mix of targets for a drawn, anime-like head: large eyes set
# low, a small nose and mouth, a pointed chin, a round skull.
ANIME = [
    *[(t, 1.0) for t in sides("eyes", "eye-scale-incr")],
    *[(t, 0.35) for t in sides("eyes", "eye-trans-down")],
    ("nose/nose-scale-vert-decr", 0.8),
    ("nose/nose-scale-horiz-decr", 0.8),
    ("nose/nose-scale-depth-decr", 0.7),
    ("mouth/mouth-scale-horiz-decr", 0.7),
    ("chin/chin-triangle", 0.6),
    ("chin/chin-height-decr", 0.4),
    ("head/head-round", 0.4),
    ("forehead/forehead-scale-vert-incr", 0.5),
    ("head/head-age-decr", 0.5),
]

# Expressions, each Sue's own mix of MakeHuman's expression units (the
# mean of the three sets the units come in).
def unit(name, both_sides=False):
    if both_sides:
        return [name.format(side) for side in ("left", "right")]
    return [name]


EXPRESSIONS = [
    ("smile", [("mouth-corner-puller", 1.0), ("mouth-elevation", 0.25), ("eye-{}-slit", 0.35)]),
    ("angry", [("eyebrows-{}-down", 1.0), ("mouth-compression", 0.6), ("mouth-depression", 0.5),
               ("nose-{}-elevation", 0.5), ("eye-{}-slit", 0.3)]),
    ("surprised", [("eyebrows-{}-up", 1.0), ("eye-{}-opened-up", 1.0), ("mouth-open", 0.55)]),
    ("sad", [("eyebrows-{}-inner-up", 1.0), ("mouth-depression", 0.9), ("eye-{}-slit", 0.25)]),
    ("eyes_closed", [("eye-{}-closure", 1.0)]),
    ("mouth_open", [("mouth-open", 1.0)]),
]


def main():
    root, out = sys.argv[1], sys.argv[2]
    verts = []
    quads = []
    group = None
    for line in open(f"{root}/3dobjs/base.obj", encoding="utf-8"):
        if line.startswith("v "):
            verts.append([float(v) for v in line.split()[1:4]])
        elif line.startswith("g "):
            group = line.split()[1]
        elif line.startswith("f ") and group in ("body", "helper-l-eye", "helper-r-eye"):
            quads.append([int(p.split("/")[0]) - 1 for p in line.split()[1:]])
    keep = list(range(BODY)) + list(range(*EYES))
    slot = {v: i for i, v in enumerate(keep)}
    quads = [[slot[v] for v in quad] for quad in quads]
    assert all(len(quad) == 4 for quad in quads)

    skel = json.load(open(f"{root}/rigs/default.mhskel", encoding="utf-8"))
    bones = skel["bones"]
    refs = skel["joints"]

    def children(bone):
        found = [bone]
        for name, data in bones.items():
            if data["parent"] == bone:
                found += children(name)
        return found

    def place(ref, points):
        ids = refs[bones[ref[0]][ref[1]]]
        return [sum(points[i][a] for i in ids) / len(ids) for a in range(3)]

    # Skin weights, merged onto Sue's joints.
    raw = json.load(open(f"{root}/rigs/default_weights.mhw", encoding="utf-8"))["weights"]
    owner = {}
    for at, (_, _, _, _, takes) in enumerate(JOINTS):
        for bone in takes:
            for name in children(bone[:-1]) if bone.endswith("*") else [bone]:
                owner[name] = at
    per = [dict() for _ in keep]
    for bone, pairs in raw.items():
        joint = owner[bone]
        for vertex, weight in pairs:
            if vertex in slot:
                mine = per[slot[vertex]]
                mine[joint] = mine.get(joint, 0.0) + weight
    head = [j[0] for j in JOINTS].index("Head")
    skin = []
    for mine in per:
        best = sorted(mine.items(), key=lambda kv: -kv[1])[:4] or [(head, 1.0)]
        total = sum(w for _, w in best)
        bytes_ = [round(w / total * 255) for _, w in best]
        bytes_[0] += 255 - sum(bytes_)
        best = [(j, b) for (j, _), b in zip(best, bytes_)] + [(0, 0)] * (4 - len(best))
        skin.append(best)

    def read(name):
        change = {}
        for line in open(f"{root}/targets/{name}.target", encoding="utf-8"):
            if line.startswith("#") or not line.strip():
                continue
            parts = line.split()
            change[int(parts[0])] = [float(v) for v in parts[1:4]]
        return change

    def summed(parts):
        """Targets added together, each as (name, factor)."""
        total = {}
        for name, factor in parts:
            for vertex, delta in read(name).items():
                mine = total.setdefault(vertex, [0.0, 0.0, 0.0])
                for a in range(3):
                    mine[a] += delta[a] * factor
        return total

    macro = "macrodetails/universal-{}-young-{}muscle-{}weight"
    morphs = []
    for sex in ("male", "female"):
        base = [(macro.format(sex, "average", "average"), 1.0)] + [
            (f"macrodetails/{race}-{sex}-young", 1.0 / 3.0) for race in ("african", "asian", "caucasian")
        ]
        morphs.append((sex, summed(base)))
        average = (macro.format(sex, "average", "average"), -1.0)
        for name, muscle, weight in (
            ("fat+", "average", "max"),
            ("fat-", "average", "min"),
            ("muscle+", "max", "average"),
            ("muscle-", "min", "average"),
        ):
            morphs.append((f"{sex}:{name}", summed([(macro.format(sex, muscle, weight), 1.0), average])))
        ideal = f"macrodetails/proportions/{sex}-young-averagemuscle-averageweight-idealproportions"
        morphs.append((f"{sex}:ideal", summed([(ideal, 1.0)])))
    cup = "breast/female-young-averagemuscle-averageweight-{}cup-averagefirmness"
    for name, size in (("bust+", "max"), ("bust-", "min")):
        morphs.append((name, summed([(cup.format(size), 1.0)])))
    for name, up, down in SLIDERS + FACE:
        morphs.append((f"{name}+", summed([(t, 1.0) for t in up])))
        morphs.append((f"{name}-", summed([(t, 1.0) for t in down])))
    morphs.append(("anime+", summed(ANIME)))
    for sex in ("male", "female"):
        young = (macro.format(sex, "average", "average"), -1.0)
        races = ("african", "asian", "caucasian")
        for name, age in (("age+", "old"), ("age-", "child")):
            aged = macro.replace("young", age).format(sex, "average", "average")
            parts = [(aged, 1.0), young]
            for race in races:
                parts.append((f"macrodetails/{race}-{sex}-{age}", 1.0 / 3.0))
                parts.append((f"macrodetails/{race}-{sex}-young", -1.0 / 3.0))
            morphs.append((f"{sex}:{name}", summed(parts)))
    for name, units in EXPRESSIONS:
        parts = []
        for stem, factor in units:
            for one in unit(stem, "{}" in stem):
                for race in ("african", "asian", "caucasian"):
                    parts.append((f"expression/units/{race}/{one}", factor / 3.0))
        morphs.append((f"{name}+", summed(parts)))

    names = [j[0] for j in JOINTS]
    ends = [(place(j[2], verts), place(j[3], verts)) for j in JOINTS]
    data = bytearray()
    data += b"SUEFIG1\0"
    data += struct.pack("<IIIIf", len(keep), len(quads), len(JOINTS), len(morphs), 0.1)
    for v in keep:
        data += struct.pack("<3f", *verts[v])
    for quad in quads:
        data += struct.pack("<4H", *quad)
    for best in skin:
        data += bytes([j for j, _ in best]) + bytes([b for _, b in best])
    for (name, parent, _, _, _), (start, end) in zip(JOINTS, ends):
        text = name.encode()
        data += bytes([len(text)]) + text
        data += struct.pack("<h", -1 if parent is None else names.index(parent))
        data += struct.pack("<6f", *start, *end)
    for name, change in morphs:
        moved = [list(v) for v in verts]
        for vertex, delta in change.items():
            for a in range(3):
                moved[vertex][a] += delta[a]
        kept = sorted((slot[v], d) for v, d in change.items() if v in slot and max(map(abs, d)) > 1e-5)
        largest = max((abs(x) for _, d in kept for x in d), default=1.0)
        step = largest / 32767.0
        text = name.encode()
        data += bytes([len(text)]) + text
        data += struct.pack("<fI", step, len(kept))
        for vertex, _ in kept:
            data += struct.pack("<H", vertex)
        for _, delta in kept:
            data += struct.pack("<3h", *[round(x / step) for x in delta])
        for joint, (start, end) in zip(JOINTS, ends):
            new = place(joint[2], moved) + place(joint[3], moved)
            data += struct.pack("<6f", *[n - o for n, o in zip(new, start + end)])
        print(f"{name}: {len(kept)} vertices")
    packed = zlib.compress(bytes(data), 9)
    open(out, "wb").write(packed)
    print(f"{len(keep)} vertices, {len(quads)} quads, {len(JOINTS)} joints, {len(morphs)} morphs")
    print(f"{len(data)} bytes, {len(packed)} compressed")


if __name__ == "__main__":
    main()
