// Resolves the CSS colour forms Troupe's theme tokens use to the sRGB bytes
// Chromium paints: hex, rgb(), oklch(), oklab(), the keywords black, white and
// transparent, and color-mix() in srgb, oklab or oklch. Out-of-gamut colours
// clip per channel, as Chromium does when it paints them.

export interface Rgba {
  r: number
  g: number
  b: number
  a: number
}

/** OKLab with alpha; `h` is set only for colours parsed in OKLCH, `NaN` when the hue is missing. */
interface Lab {
  l: number
  a: number
  b: number
  alpha: number
  h?: number
}

const ACHROMATIC = 1e-4

function srgbToLinear(c: number): number {
  const abs = Math.abs(c)
  return abs <= 0.04045 ? c / 12.92 : Math.sign(c) * ((abs + 0.055) / 1.055) ** 2.4
}

function linearToSrgb(c: number): number {
  const abs = Math.abs(c)
  return abs <= 0.0031308 ? c * 12.92 : Math.sign(c) * (1.055 * abs ** (1 / 2.4) - 0.055)
}

type Mat3 = readonly [number, number, number, number, number, number, number, number, number]

function mul(m: Mat3, x: number, y: number, z: number): [number, number, number] {
  return [
    m[0] * x + m[1] * y + m[2] * z,
    m[3] * x + m[4] * y + m[5] * z,
    m[6] * x + m[7] * y + m[8] * z
  ]
}

function invert(m: Mat3): Mat3 {
  const [a, b, c, d, e, f, g, h, i] = m
  const det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g)
  return [
    (e * i - f * h) / det,
    (c * h - b * i) / det,
    (b * f - c * e) / det,
    (f * g - d * i) / det,
    (a * i - c * g) / det,
    (c * d - a * f) / det,
    (d * h - e * g) / det,
    (b * g - a * h) / det,
    (a * e - b * d) / det
  ]
}

// Chromium converts between OKLab and sRGB through XYZ D50
// (ui/gfx/color_conversions.cc): skcms's Bradford adaptation from the D65
// chromaticity to its D50 white, then skcms's sRGB gamut matrix. Its D50 is
// not the CSS spec's, so the spec's direct path lands one byte off on about 2%
// of the bundled tokens.
const OKLAB_TO_LMS: Mat3 = [
  1, 0.3963377773761749, 0.2158037573099136, 1, -0.1055613458156586, -0.0638541728258133, 1,
  -0.0894841775298119, -1.2914855480194092
]
const LMS_TO_XYZ_D65: Mat3 = [
  1.2268798758459243, -0.5578149944602171, 0.2813910456659647, -0.0405757452148008,
  1.112286803280317, -0.0717110580655164, -0.0763729366746601, -0.4214933324022432,
  1.5869240198367816
]
const SRGB_TO_XYZ_D50: Mat3 = [
  0.436065674, 0.385147095, 0.143066406, 0.222488403, 0.716873169, 0.06060791, 0.013916016,
  0.097076416, 0.714096069
]
const BRADFORD: Mat3 = [0.8951, 0.2664, -0.1614, -0.7502, 1.7135, 0.0367, 0.0389, -0.0685, 1.0296]

function compose(a: Mat3, b: Mat3): Mat3 {
  const out: number[] = []
  for (let i = 0; i < 3; i++) {
    for (let j = 0; j < 3; j++) {
      out.push(a[i * 3] * b[j] + a[i * 3 + 1] * b[3 + j] + a[i * 3 + 2] * b[6 + j])
    }
  }
  return out as unknown as Mat3
}

/** skcms_AdaptToXYZD50 for the D65 chromaticity (0.3127, 0.3290). */
function d65ToD50(): Mat3 {
  const [x, y] = [0.3127, 0.329]
  const src = mul(BRADFORD, x / y, 1, (1 - x - y) / y)
  const dst = mul(BRADFORD, 0.96422, 1, 0.82521)
  const scale: Mat3 = [dst[0] / src[0], 0, 0, 0, dst[1] / src[1], 0, 0, 0, dst[2] / src[2]]
  return compose(invert(BRADFORD), compose(scale, BRADFORD))
}

const LMS_TO_XYZ_D50 = compose(d65ToD50(), LMS_TO_XYZ_D65)
const XYZ_D50_TO_LMS = invert(LMS_TO_XYZ_D50)
const XYZ_D50_TO_SRGB = invert(SRGB_TO_XYZ_D50)
const LMS_TO_OKLAB = invert(OKLAB_TO_LMS)

function oklabToSrgb(c: Lab): Rgba {
  const [l, m, s] = mul(OKLAB_TO_LMS, c.l, c.a, c.b).map((v) => v ** 3)
  const [x, y, z] = mul(LMS_TO_XYZ_D50, l, m, s)
  const [r, g, b] = mul(XYZ_D50_TO_SRGB, x, y, z)
  return { r: linearToSrgb(r), g: linearToSrgb(g), b: linearToSrgb(b), a: c.alpha }
}

function srgbToOklab(c: Rgba): Lab {
  const xyz = mul(SRGB_TO_XYZ_D50, srgbToLinear(c.r), srgbToLinear(c.g), srgbToLinear(c.b))
  const [l, m, s] = mul(XYZ_D50_TO_LMS, ...xyz).map(Math.cbrt)
  const [L, a, b] = mul(LMS_TO_OKLAB, l, m, s)
  return { l: L, a, b, alpha: c.a }
}

function fromOklch(l: number, chroma: number, hue: number, alpha: number): Lab {
  const rad = (hue * Math.PI) / 180
  const missing = Number.isNaN(hue)
  return {
    l,
    a: missing ? 0 : chroma * Math.cos(rad),
    b: missing ? 0 : chroma * Math.sin(rad),
    alpha,
    h: hue
  }
}

function toOklch(c: Lab): { l: number; c: number; h: number; alpha: number } {
  const chroma = Math.hypot(c.a, c.b)
  let h = c.h !== undefined ? c.h : (Math.atan2(c.b, c.a) * 180) / Math.PI
  if (c.h === undefined && chroma < ACHROMATIC) h = Number.NaN
  if (h < 0) h += 360
  return { l: c.l, c: chroma, h, alpha: c.alpha }
}

/** Splits `s` on `sep` outside parentheses. */
export function splitTopLevel(s: string, sep: string): string[] {
  const out: string[] = []
  let depth = 0
  let start = 0
  for (let i = 0; i < s.length; i++) {
    const ch = s[i]
    if (ch === '(') depth++
    else if (ch === ')') depth--
    else if (depth === 0 && (sep === ' ' ? /\s/.test(ch) : ch === sep)) {
      out.push(s.slice(start, i))
      start = i + 1
    }
  }
  out.push(s.slice(start))
  return out.map((part) => part.trim()).filter((part) => part.length > 0)
}

function number(token: string, percentScale = 1): number {
  if (token === 'none') return Number.NaN
  if (token.endsWith('%')) return (Number.parseFloat(token) / 100) * percentScale
  const value = Number(token)
  if (!Number.isFinite(value)) throw new Error(`not a number: ${token}`)
  return value
}

/** `a b c [/ alpha]` or `a, b, c[, alpha]` inside a colour function. */
function channels(body: string): { values: string[]; alpha: string | undefined } {
  const [main, alpha] = body.includes('/') ? body.split('/') : [body, undefined]
  const values = main.includes(',')
    ? main.split(',').map((v) => v.trim())
    : main.trim().split(/\s+/)
  if (body.includes(',') && values.length === 4)
    return { values: values.slice(0, 3), alpha: values[3] }
  return { values, alpha: alpha?.trim() }
}

function parseHex(hex: string): Rgba {
  let h = hex.slice(1)
  if (h.length === 3 || h.length === 4) h = [...h].map((ch) => ch + ch).join('')
  if (h.length !== 6 && h.length !== 8) throw new Error(`bad hex colour: ${hex}`)
  const byte = (i: number) => Number.parseInt(h.slice(i, i + 2), 16) / 255
  return { r: byte(0), g: byte(2), b: byte(4), a: h.length === 8 ? byte(6) : 1 }
}

function parseLab(value: string): Lab {
  const v = value.trim().toLowerCase()
  if (v === 'transparent') return { l: 0, a: 0, b: 0, alpha: 0 }
  if (v === 'black') return srgbToOklab({ r: 0, g: 0, b: 0, a: 1 })
  if (v === 'white') return srgbToOklab({ r: 1, g: 1, b: 1, a: 1 })
  if (v.startsWith('#')) return srgbToOklab(parseHex(v))
  const fn = /^([a-z-]+)\((.*)\)$/s.exec(v)
  if (!fn) throw new Error(`unsupported colour: ${value}`)
  const [, name, body] = fn
  if (name === 'color-mix') return parseColorMix(body)
  const { values, alpha } = channels(body)
  const a = alpha === undefined ? 1 : number(alpha)
  if (name === 'rgb' || name === 'rgba') {
    const [r, g, b] = values.map((x) => number(x, 255) / 255)
    return srgbToOklab({ r, g, b, a })
  }
  if (name === 'oklch') {
    const [l, c, h] = values
    return fromOklch(number(l), number(c, 0.4), number(h), a)
  }
  if (name === 'oklab') {
    const [l, aa, b] = values
    return { l: number(l), a: number(aa, 0.4), b: number(b, 0.4), alpha: a }
  }
  throw new Error(`unsupported colour function: ${name}`)
}

function mixHue(h1: number, h2: number, t: number): number {
  if (Number.isNaN(h1)) return h2
  if (Number.isNaN(h2)) return h1
  let d = h2 - h1
  if (d > 180) d -= 360
  else if (d < -180) d += 360
  const h = h1 + d * t
  return ((h % 360) + 360) % 360
}

/** CSS Color 5 `color-mix()`, premultiplied, shorter hue arc. */
function parseColorMix(body: string): Lab {
  const [space, first, second] = splitTopLevel(body, ',')
  const method = /^in\s+(srgb|oklab|oklch)$/.exec(space)
  if (!method || !first || !second) throw new Error(`unsupported color-mix: ${body}`)
  const part = (s: string): { color: Lab; p: number | undefined } => {
    const tokens = splitTopLevel(s, ' ')
    const pct = tokens.find((t) => /^[\d.]+%$/.test(t))
    const color = tokens.filter((t) => t !== pct).join(' ')
    return {
      color: parseLab(color),
      p: pct === undefined ? undefined : Number.parseFloat(pct) / 100
    }
  }
  const one = part(first)
  const two = part(second)
  let p1 = one.p ?? (two.p === undefined ? 0.5 : 1 - two.p)
  let p2 = two.p ?? 1 - p1
  const sum = p1 + p2
  const alphaScale = sum < 1 ? sum : 1
  p1 /= sum
  p2 /= sum
  const a1 = one.color.alpha
  const a2 = two.color.alpha
  const alpha = a1 * p1 + a2 * p2
  const premix = (x: number, y: number) => (alpha === 0 ? 0 : (x * a1 * p1 + y * a2 * p2) / alpha)
  let mixed: Lab
  if (method[1] === 'oklab') {
    mixed = {
      l: premix(one.color.l, two.color.l),
      a: premix(one.color.a, two.color.a),
      b: premix(one.color.b, two.color.b),
      alpha
    }
  } else if (method[1] === 'oklch') {
    const x = toOklch(one.color)
    const y = toOklch(two.color)
    const h = mixHue(x.h, y.h, p2)
    mixed = fromOklch(premix(x.l, y.l), premix(x.c, y.c), h, alpha)
  } else {
    const x = oklabToSrgb(one.color)
    const y = oklabToSrgb(two.color)
    mixed = srgbToOklab({
      r: premix(x.r, y.r),
      g: premix(x.g, y.g),
      b: premix(x.b, y.b),
      a: alpha
    })
  }
  return { ...mixed, alpha: mixed.alpha * alphaScale }
}

/** Parses one CSS colour value. Throws on a form the theme tokens do not use. */
export function parseColor(value: string): Rgba {
  return oklabToSrgb(parseLab(value))
}

/** True when `value` parses as a single colour. */
export function isColor(value: string): boolean {
  try {
    parseColor(value)
    return true
  } catch {
    return false
  }
}

/** `#rrggbbaa`: channels clipped to sRGB and rounded to bytes, as Chromium paints them. */
export function toHex8(c: Rgba): string {
  const byte = (v: number) => Math.round(Math.min(1, Math.max(0, v)) * 255)
  return `#${[c.r, c.g, c.b, c.a]
    .map(byte)
    .map((v) => v.toString(16).padStart(2, '0'))
    .join('')}`
}
