# Packages: optional downloads

Some features of Sue need files that Sue does not ship: they are large,
or they are someone else's work under its own licence. Such files come
as **packages**. A package is installed and removed by the person, in
Settings, Packages, or where a feature asks for it (Sue Paint's Hand
Scanner shows the card of the package it needs). Nothing is downloaded
until someone presses Install.

This page describes the package manifest, so that anyone can write one,
and what installing does. The code is `crates/sue-pkg/src/optional.rs`
(the manifest and the install), `crates/sue-platform/src/packages.rs`
(the network and the background job) and `crates/sue-wm/src/packages.rs`
(the card people see).

## The manifest

A package is described by one text file, its manifest. Its name ends in
`.suepkg`. It is lines of `key = value`; empty lines and lines that
start with `#` are passed over, and so are keys this version of Sue does
not know, so newer manifests still load.

```text
# A comment.
format = 1
id = hand-tracking
name = Hand tracking
version = 1.0.0
kind = ai-model
summary = Finds your hand in the camera's picture.
licence = Apache-2.0
author = MediaPipe Hands by Google, converted to ONNX by the OpenCV Zoo
homepage = https://github.com/opencv/opencv_zoo
file = palm_detection.onnx | 3905734 | 78ff51c3...57fcce7c | https://example.org/palm.onnx
file = hand_landmark.onnx | 4099621 | db0898ae...bef6d81c | files/hand.onnx
```

| Key | Needed | What it is |
| --- | --- | --- |
| `format` | yes | The manifest format: `1`. A manifest of another format is refused, with a message that says so. |
| `id` | yes | The package's id and the name of its folder: lower case letters, digits, `.`, `_` and `-`, starting with a letter or digit, at most 128 bytes. |
| `name` | no | What people read. Without it the id is shown. |
| `version` | yes | A SemVer version, such as `1.0.0`. |
| `kind` | no | What it installs: `ai-model` (the default), `codec`, `applet`, `rice-theme` or `core-update`. |
| `summary` | no | What it is for, in a sentence. |
| `licence` | no | Its licence as an SPDX expression (`license` is read too). |
| `author` | no | Who made what is in it. |
| `homepage` | no | The project's page. |
| `file` | yes, one or more | One file of the package: `name \| size \| sha256 \| source`. |

A `file` line has four parts, separated by `|`:

- **name**: the file's name in the package's folder. A plain file name:
  no folders, no drive, not starting with a dot, not ending in `.part`
  and not `package.suepkg`. Each name once.
- **size**: its size in bytes.
- **sha256**: its SHA-256, 64 hex digits.
- **source**: where it comes from. Either a link (`https://`; `http://`
  is accepted for a server on your own machine) or a path relative to
  the manifest's own folder, made of plain folder and file names (no
  `..`, no drive).

## Sharing a package

A manifest whose files are named by paths is a package you can hand to
someone as a folder: the `.suepkg` file and the files next to it. In
Settings, Packages, **Install from a File** takes the `.suepkg` file and
copies the files from its folder; no network is used.

A manifest whose files are links can be shared as the one small file:
Install from a File fetches the files it names.

A package from a file carries no signature. While **Only signed
packages** (Settings, Packages, Trust) is on, Sue refuses it and says
why; turn the setting off to install it. Install only packages from
people you trust: a package's files are checked against the manifest
(they are the files its author named), not against anything else.

Sue does not ship packages that hold another program's files, and
package makers must have the right to share what they put in one.

## What installing does

Packages live in Sue's packages folder (shown at the bottom of Settings,
Packages; `packages` in Sue's data folder, or under `SUE_HOME`), one
folder per package named by its id:

```text
packages/
  hand-tracking/
    package.suepkg        the manifest it was installed from
    palm_detection.onnx
    hand_landmark.onnx
  catalog/                manifests added from files, so they stay listed
  .staging/               downloads in progress (gone when none is)
```

Installing is all or nothing:

1. Each file is fetched into `.staging/<id>/` as `<name>.part`.
2. When a file is whole, its size and SHA-256 are checked against the
   manifest. A file that does not match is thrown away and the install
   fails with a message; it never reaches the package's folder.
3. When every file is whole and checked, the manifest is written next to
   them and the staging folder is moved to `packages/<id>/` in one step.

**Cancel** stops the download at its next piece and removes the staging
folder: nothing half installed is left. A **failure** (no network, the
server stopped answering) keeps the parts, and Try Again carries on from
where it stopped (an HTTP `Range` request; a server that cannot do that
sends the file from its start). **Uninstall** removes the package's
folder and anything a failed try left.

A package counts as installed when its folder holds its manifest and
every file the manifest names, each of the size it names.

## The packages Sue names itself

Their manifests are files in `crates/sue-platform/packages/`, compiled
into Sue; the files they name are not.

### Hand tracking (`hand-tracking`)

The two models Sue Paint's Hand Scanner runs to find a hand in the
camera's picture: 7.6 MiB together.

| File | Bytes | SHA-256 | From |
| --- | --- | --- | --- |
| `palm_detection.onnx` | 3,905,734 | `78ff51c38496b7fc8b8ebdb6cc8c1abb02fa6c38427c6848254cdaba57fcce7c` | `palm_detection_mediapipe_2023feb.onnx`, OpenCV Zoo, commit `233e619d` of `huggingface.co/opencv/palm_detection_mediapipe` |
| `hand_landmark.onnx` | 4,099,621 | `db0898ae717b76b075d9bf563af315b29562e11f8df5027a1ef07b02bef6d81c` | `handpose_estimation_mediapipe_2023feb.onnx`, OpenCV Zoo, commit `4b2a0b44` of `huggingface.co/opencv/handpose_estimation_mediapipe` |

Licence: both are MediaPipe Hands models by Google, released under the
Apache License 2.0, converted to ONNX and published by the OpenCV Zoo
(`github.com/opencv/opencv_zoo`), whose model folders carry the Apache
License 2.0 too. The links are the OpenCV Zoo's own model repositories,
pinned to one commit each, so the files cannot change under the hashes.
The same files are in the OpenCV Zoo's GitHub repository under
`models/palm_detection_mediapipe` and
`models/handpose_estimation_mediapipe`.

Sue runs them with `tract` on the processor (`sue_ai::hand`); the steps
around them follow MediaPipe's published pipeline and are Sue's own
code. The models see the camera's pictures in memory only. Nothing they
see or find leaves the computer.
