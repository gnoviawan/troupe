/**
 * Launch flags for every Chromium that paints the React reference.
 *
 * - `--font-render-hinting=slight`: headless Chromium defaults to `full`,
 *   which lays text out on hinted whole-pixel advances, so a 13 px Inter line
 *   came out 2.2 % wider than the font's design widths. Desktop Chrome on
 *   Linux follows fontconfig (`slight` on stock GNOME), and macOS and Windows
 *   lay out fractional advances.
 * - `--disable-lcd-text`: grayscale anti-aliasing. Rune's headless frames
 *   draw grayscale text; LCD against grayscale alone measured a 0.5 to 0.7 %
 *   floor in the chat prototype.
 * - `--disable-partial-raster`: Chromium otherwise re-rasters only the
 *   damaged part of a tile, and the anti-aliased edge of a shape at a
 *   fractional position (the 714.5 px logo under the mention menu, a rounded
 *   box's corner) came out a few levels apart depending on which damage the
 *   session had seen. React against React differed by 1 to 37 pixels on 2 of
 *   25 surfaces with identical layout; whole-tile raster reads zero.
 * - The network flag lets a route-fulfilled page open sockets to 127.0.0.1.
 */
export const REFERENCE_ARGS = [
  '--disable-features=LocalNetworkAccessChecks,PrivateNetworkAccessChecks',
  '--font-render-hinting=slight',
  '--disable-lcd-text',
  '--disable-partial-raster'
]
