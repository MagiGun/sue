# Movie export

How Sue writes movies, for people who build on it and for anyone who
makes presets that name a movie format or quality. Sue Paint's export
dialog (package P4-7 of the v0.0.8 plan) stands on what is described
here.

## The formats

| Stable id | Shown as | Where | Sound | Size |
| --- | --- | --- | --- | --- |
| `mp4-h264` | MP4 (H.264) | Windows, where Windows has an H.264 encoder | AAC-LC, 192 kbit/s, 44100 or 48000 Hz, mono or stereo; no sound track when the movie has none | even sides of 34 to 4096 pixels, at most 36864 blocks of 16 by 16 (4096 by 2304, 3840 by 2160 or any picture of that area); an odd side gains one pixel |
| `avi-mjpeg` | AVI (Motion JPEG) | everywhere | none | 1 to 32767 pixels a side |

The ids are saved in settings and presets and never change. A dialog
offers the formats in the order `sue_platform::movie::formats()` gives:
MP4 first where the system can write it, then the AVI, which is always
there.

### MP4 (H.264)

- Sue ships no H.264 and no AAC code. On Windows the encoders are
  Windows' own, reached through Media Foundation's Sink Writer in the
  audited crate `crates/sue-platform/windows` (module `media`), so the
  codec licence sits with the maker of the operating system.
- Video: H.264 High profile, 4:2:0, 8 bit, BT.709 colours in limited
  range with the matching colour tags, a constant frame rate (any
  rational rate, such as 24/1 or 30000/1001), a key frame every two
  seconds, quality based rate control, the software encoder.
- No B frames. With them Windows' encoder shows the first picture one
  frame after it is decoded, and Windows' own player ignores the edit
  list that corrects that, so the pictures ran one frame behind the
  sound (tried on Windows 11 build 26300). Every picture is now shown
  at the time it was pushed, in every player. Should a file still carry
  such an offset, `sue_io::mp4_index_first` adds the edit list
  (`edts`), which ffmpeg and browsers follow.
- The index (`moov`) comes before the picture data, so the file starts
  playing while it loads and upload sites take it as it is. Media
  Foundation's own setting for that (`MF_MPEG4SINK_MOOV_BEFORE_MDAT`)
  wrote files whose chunk offsets pointed into zeros on Windows 11 build
  26300, so Sue moves the index itself, in safe Rust
  (`sue_io::mp4_index_first`).
- Size limits were found by trying the encoder on Windows 11 build 26300:
  a side of 32 is refused and 34 is taken; past 36864 blocks (H.264
  level 5.2) it refuses unless a higher level is named, and at level 6
  it writes 7680 by 4320, but Windows' own decoder then refuses the
  file, so the Windows player could not play it. Sue stops at what
  Windows plays.
- Where Windows has no H.264 encoder (Windows N editions without
  Microsoft's free Media Feature Pack) MP4 is not offered and opening it
  reports "this system has no MP4 (H.264) encoder"; the AVI still is.
- Android (P7-3) and the other systems (Phase 8) have no MP4 route yet.

### AVI (Motion JPEG)

Sue's own writer (`sue_io::MjpegAvi`): each frame is a whole JPEG, every
frame a key frame, an `idx1` index at the end. It carries no sound.

## Qualities

| Stable id | Shown as | H.264 quality (1 to 100) | JPEG quality |
| --- | --- | --- | --- |
| `small` | Small file | 60 | 75 |
| `normal` | Normal | 75 | 90 |
| `high` | High quality | 90 | 97 |

Measured on Windows 11 build 26300 (the ignored test
`measure_movie_qualities` in `crates/sue-platform/src/movie.rs` prints
the table): two seconds of 1280 by 720 at 24 frames a second with a
stereo tone. "flat" is flat colour shapes and black lines moving over
paper, as cel animation is; "painted" is soft gradients with a fine
grain that changes every frame, about the hardest a painting gets.

| Clip | Quality | KiB | Mean luma PSNR (dB) | Worst frame (dB) |
| --- | --- | --- | --- | --- |
| flat | small | 117 | 51.1 | 45.9 |
| flat | normal | 157 | 53.2 | 37.4 |
| flat | high | 166 | 54.5 | 37.2 |
| painted | small | 698 | 34.1 | 33.9 |
| painted | normal | 4965 | 37.2 | 37.1 |
| painted | high | 9296 | 41.2 | 41.0 |

On flat colour every frame but one stays within a few dB of the mean;
the worst is the second frame, the first one predicted from the key
frame (37 dB at "normal" and "high", still clean to the eye). Looked at
two times enlarged, faint streaks trail the moving lines over flat
colour at "small" and "normal" and are hardly there at "high"; at
normal size they do not show. On grain that changes every frame the
encoder spends what it takes, so files grow fast with the quality.

## For code: the sink

`sue_io::MovieSink` is the trait every writer implements: begin (each
sink's constructor, from `MovieSettings`), `push_frame` (straight 8-bit
RGBA of the movie's size, transparent pixels over white), `push_sound`
(interleaved float samples; before, between or after the frames, the
sink keeps them in time order, and sound past the last frame is cut),
`finish` (the file's bytes, written by the caller with
`sue_platform::write_atomic`). `sue_platform::movie::open(format,
settings)` opens one; a sink stays on the thread that opened it.

`sue_io::rgba_to_nv12` is the picture conversion every system encoder
takes (BT.709, limited range, the chroma of each 2 by 2 block from its
average colour, odd sides padded by repeating the edge).
