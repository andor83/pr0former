export interface Envelope { attack: number; decay: number; sustain: number; release: number }
export interface Point { x: number; y: number }
/** Draggable points: attack peak, decay end (decay + sustain), release start (release + sustain). */
export type Handle = 'attack' | 'decay' | 'release'

export const TIME_LIMIT = 10000
/** Time axis steps: 250 ms doubling up to 64 s, so the plot only rescales when the envelope outgrows it. */
const SCALES = Array.from({ length: 9 }, (_, i) => 250 * 2 ** i)
/** The timed stages never take more than this share of the axis; the rest is the sustain plateau. */
const FILL = 0.8

export function timedStages(e: Envelope) { return e.attack + e.decay + e.release }
/** Milliseconds spanned by the whole graph: the smallest step that leaves room for a plateau. */
export function timeScale(e: Envelope) {
  const needed = timedStages(e) / FILL
  return SCALES.find(scale => scale >= needed) ?? SCALES[SCALES.length - 1]!
}
/** Width of the drawn sustain plateau at a given time scale. */
export function holdTime(e: Envelope, total = timeScale(e)) { return Math.max(0, total - timedStages(e)) }

/** Envelope polyline in unit space: x over `total` ms (0–1), y as level (0–1, 1 at the top). The release always ends at x = 1. */
export function envelopePoints(e: Envelope, total = timeScale(e)): Point[] {
  const hold = holdTime(e, total), x = (ms: number) => Math.min(1, ms / Math.max(1e-9, total))
  return [
    { x: 0, y: 0 },
    { x: x(e.attack), y: 1 },
    { x: x(e.attack + e.decay), y: e.sustain },
    { x: x(e.attack + e.decay + hold), y: e.sustain },
    { x: 1, y: 0 },
  ]
}

/**
 * Parameters implied by dragging a handle to a unit-space position, where
 * `unit.y` is 0 at the top of the graph. The time scale is frozen by the
 * caller for the duration of a drag so the graph does not rescale under the
 * pointer. The release handle measures back from the right edge, so dragging
 * it left lengthens the release.
 */
export function dragHandle(e: Envelope, handle: Handle, unit: Point, total: number): Partial<Envelope> {
  const ms = Math.min(1, Math.max(0, unit.x)) * total
  const level = Math.round(Math.min(1, Math.max(0, 1 - unit.y)) * 100) / 100
  const time = (value: number) => Math.round(Math.min(TIME_LIMIT, Math.max(0, value)))
  switch (handle) {
    case 'attack': return { attack: time(ms) }
    case 'decay': return { decay: time(ms - e.attack), sustain: level }
    case 'release': return { release: time(total - ms), sustain: level }
  }
}

/** Short label for the time axis span, e.g. "500 ms" or "2 s". */
export function scaleLabel(total: number) { return total >= 1000 ? `${total / 1000} s` : `${total} ms` }
