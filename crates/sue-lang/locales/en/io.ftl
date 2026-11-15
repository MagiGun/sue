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

filter-lanczos = Lanczos (sharpest)
filter-bicubic = Bicubic
filter-bilinear = Bilinear (soft)
filter-nearest = Nearest (pixel art)
the-picture-has-no-pixels = the picture has no pixels
float-pictures-are-not-converted-yet = float pictures are not converted yet
jpeg-xl-writing-not-build = JPEG XL writing is not in this build

## Brush files and the brush library (brushes/sut.rs, brushes/library.rs)

not-a-clip-studio-paint-sub-tool = not a Clip Studio Paint sub tool
the-file-holds-no-sub-tool = the file holds no sub tool
a-picture-the-brush-uses-is-missing = a picture the brush uses is missing
the-file-holds-no-brushes = the file holds no brushes
edit-brushes = Edit brushes
brushes-library = Brushes
custom-brushes = Custom

## The groups Sue's built-in brushes are listed under (sue_brush::presets)

brush-group-pen = Pen
brush-group-marker = Marker
brush-group-pencil = Pencil
brush-group-pastel = Pastel
brush-group-watercolor = Watercolor
brush-group-thick-paint = Thick paint
brush-group-ink = Ink
brush-group-airbrush = Airbrush
brush-group-effect = Effect
brush-group-flower = Flower
brush-group-clothing = Clothing
brush-group-lines = Lines
brush-group-eraser = Eraser
trimming-left-no-sound = trimming left no sound

## Names of the formats exports write (sue_io::PICTURE_SAVE_TYPES and
## the sound page)

png-picture = PNG picture
jpeg-xl-picture = JPEG XL picture
jpeg-picture = JPEG picture
webp-picture = WebP picture
tiff-picture = TIFF picture
bmp-picture = BMP picture
tga-picture = TGA picture
gif-picture = GIF picture
qoi-picture = QOI picture
ico-picture = ICO picture
ppm-picture = PPM picture
wav-sound = WAV sound
flac-sound = FLAC sound
aiff-sound = AIFF sound
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
movie-bad-sound = the sound's rate or channel count does not fit the format
movie-no-pixels = the frame has no 8-bit RGBA pixels
movie-no-frames = the movie has no frames
movie-encoder-failed = the system encoder failed
