// WCAG 2.2 contrast between two `#rrggbb` colours
// (https://www.w3.org/TR/WCAG22/#dfn-contrast-ratio).

function channel(hex: string, at: number): number {
  const c = Number.parseInt(hex.slice(at, at + 2), 16) / 255;
  return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
}

function luminance(color: string): number {
  const hex = color.trim().toLowerCase();
  if (!/^#[0-9a-f]{6}$/.test(hex)) {
    throw new Error(`contrast: ${color} is not #rrggbb; READABLE_PAIRS takes only solid colours`);
  }
  return 0.2126 * channel(hex, 1) + 0.7152 * channel(hex, 3) + 0.0722 * channel(hex, 5);
}

export function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x) as [number, number];
  return (hi + 0.05) / (lo + 0.05);
}
