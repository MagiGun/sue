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
