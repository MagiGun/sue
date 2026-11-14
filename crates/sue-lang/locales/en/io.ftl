## Files and formats (crates/sue-io), as Settings and the tools show them.
## The texts stay English in the code; they are read here through
## sue_lang::translate("io", ...).

## Built-in codecs

codec-decode-jpeg-reconstruction = decode, JPEG reconstruction
codec-lossless-encode = lossless encode
codec-decode-encode = decode, encode
codec-decode = decode
codec-16-bit-pcm-encode = 16-bit PCM encode
codec-hex-and-byte-map = hex and byte map
codec-anything-else = Anything else
codec-built-in = built in

## Export operations (picture_ops.rs, audio.rs)

the-picture-has-no-pixels = the picture has no pixels
float-pictures-are-not-converted-yet = float pictures are not converted yet
jpeg-xl-writing-not-build = JPEG XL writing is not in this build

## Movie export (video.rs, and sue_platform::movie)

movie-format-mp4-h264 = MP4 (H.264)
movie-format-avi-mjpeg = AVI (Motion JPEG)
movie-quality-small = Small file
movie-quality-normal = Normal
movie-quality-high = High quality
movie-padded-note = MP4 needs an even width and height: one pixel is added
movie-no-encoder = this system has no MP4 (H.264) encoder
movie-no-sound-track = this movie format carries no sound
movie-wrong-size = every frame must be the movie's size
movie-bad-size = the movie size is outside what the format allows
movie-bad-rate = the frame rate is not usable
movie-bad-sound = the sound must be 1 or 2 channels at 8000 to 192000 Hz
movie-no-pixels = the frame has no 8-bit RGBA pixels
movie-no-frames = the movie has no frames
movie-encoder-failed = the system encoder failed
