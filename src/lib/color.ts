/** Colour helpers. Settings store colours as #rrggbb or #rrggbbaa. */

const hex2 = (n: number) => Math.round(Math.min(255, Math.max(0, n))).toString(16).padStart(2, "0");

const RGB = /^rgba?\(\s*([\d.]+)[\s,]+([\d.]+)[\s,]+([\d.]+)(?:[\s,/]+([\d.]+%?))?\s*\)$/;

/** Normalise #rgb, #rrggbb, #rrggbbaa, rgb(), rgba() to hex. Null if unknown. */
export function toHex(input: unknown): string | null {
  if (typeof input !== "string") return null;
  const s = input.trim().toLowerCase();
  if (/^#[0-9a-f]{3}$/.test(s)) return "#" + [...s.slice(1)].map((c) => c + c).join("");
  if (/^#([0-9a-f]{6}|[0-9a-f]{8})$/.test(s)) return s;
  const m = s.match(RGB);
  if (!m) return null;
  const rgb = "#" + hex2(+m[1]) + hex2(+m[2]) + hex2(+m[3]);
  if (m[4] === undefined) return rgb;
  const a = m[4].endsWith("%") ? parseFloat(m[4]) / 100 : parseFloat(m[4]);
  return a >= 1 ? rgb : rgb + hex2(a * 255);
}

/** Split #rrggbb[aa] into the 6-digit colour and 0..1 alpha. */
export function splitAlpha(hex: string): { rgb: string; alpha: number } {
  const h = toHex(hex) ?? "#000000";
  return { rgb: h.slice(0, 7), alpha: h.length === 9 ? parseInt(h.slice(7), 16) / 255 : 1 };
}

export function withAlpha(rgb: string, alpha: number): string {
  const base = (toHex(rgb) ?? "#000000").slice(0, 7);
  return alpha >= 1 ? base : base + hex2(alpha * 255);
}
