## Files and formats (crates/sue-io).

codec-decode-jpeg-reconstruction = 解码，JPEG重构
codec-lossless-encode = 无损编码
codec-decode-encode = 解码，编码
codec-decode = 解码
codec-16-bit-pcm-encode = 16位PCM编码
codec-hex-and-byte-map = 十六进制与字节映射
codec-anything-else = 其他任意格式
codec-built-in = 内置

## Export operations (picture_ops.rs, audio.rs)

filter-lanczos = Lanczos (最清晰)
filter-bicubic = 双三次插值
filter-bilinear = 双线性插值 (柔和)
filter-nearest = 最近邻 (像素画)
the-picture-has-no-pixels = 图片没有像素
float-pictures-are-not-converted-yet = 浮点格式图片暂不支持转换
jpeg-xl-writing-not-build = 当前版本未包含 JPEG XL 写入支持
trimming-left-no-sound = 裁剪后未剩余有效音频
