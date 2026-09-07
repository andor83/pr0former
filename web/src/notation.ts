/** Display durations in quarter-note beats; never change the stored score timing. */
export interface DurationGlyph { duration: string; beats: number; dots: number }
const bases = [4, 2, 1, 0.5, 0.25, 0.125, 0.0625]
const codes = ['w', 'h', 'q', '8', '16', '32', '64']
const glyphs: DurationGlyph[] = bases.flatMap((beats, i) => [0, 1, 2].map(dots => ({
  duration: codes[i]!, beats: beats * (2 - 2 ** -dots), dots,
}))).sort((a, b) => b.beats - a.beats)

/** null means a tuplet/custom or excessive duration requiring a different notation model. */
export function durationGlyphs(beats: number): DurationGlyph[] | null {
  if (!Number.isFinite(beats) || beats <= 0) return null
  const exact = glyphs.find(g => Math.abs(g.beats - beats) < 1e-9)
  if (exact) return [{ ...exact }]
  if (beats > 256 || Math.abs(beats * 16 - Math.round(beats * 16)) > 1e-9) return null
  let remaining = beats
  const result: DurationGlyph[] = []
  // Keep split fragments on the sixteenth-of-a-quarter grid.
  const candidates = glyphs.filter(g => Number.isInteger(g.beats * 16))
  while (remaining > 1e-9 && result.length < 64) {
    const glyph = candidates.find(g => g.beats <= remaining + 1e-9)
    if (!glyph) return null
    result.push({ ...glyph }); remaining -= glyph.beats
  }
  return remaining < 1e-9 ? result : null
}
