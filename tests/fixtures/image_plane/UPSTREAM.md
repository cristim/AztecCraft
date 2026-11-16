# Image plane fixtures (#459)

Original synthetic content generated for this repository; no third-party image. Licence: the
repository licence (MIT OR Apache-2.0).

- `interlaced_3x1.png`: a 3x1 Adam7-interlaced 8-bit RGB PNG whose pixels are red, green and blue
  left to right. Made from a 3x1 binary PPM of those pixels with ImageMagick
  (`magick in.ppm -interlace PNG -define png:color-type=2 -depth 8 interlaced_3x1.png`).
