# Image plane fixtures (#459)

Original synthetic content generated for this repository; no third-party image. Licence: the
repository licence (MIT OR Apache-2.0).

- `interlaced_3x1.png`: a 3x1 Adam7-interlaced 8-bit RGB PNG whose pixels are red, green and blue
  left to right. Made from a 3x1 binary PPM of those pixels with ImageMagick
  (`magick in.ppm -interlace PNG -define png:color-type=2 -depth 8 interlaced_3x1.png`).
- `gradient.jpg`, `gradient_progressive.jpg`: 16x8 baseline and progressive JPEGs (no chroma
  subsampling, quality 95) of the synthetic image whose pixel (x, y) is (16x, 32y, 128). Made with
  libjpeg-turbo `cjpeg -quality 95 -sample 1x1 [-progressive]` from a PPM the tests' formula describes.
- `gradient_gray.jpg`: the 16x8 gray image whose pixel (x, y) is 12x + 8y (`cjpeg -grayscale`).
- `cmyk.jpg`: the same colour image converted to a 4-component CMYK JPEG
  (`magick in.ppm -colorspace CMYK -depth 8 cmyk.jpg`); the decoder must refuse it.
- `wide_9000x16.jpg`: a valid solid-colour 9000x16 JPEG (`cjpeg -quality 75`), wider than the
  plate limit and narrower than zune-jpeg's own 16384 limit.
