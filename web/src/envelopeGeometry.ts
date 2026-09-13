export interface Envelope { attack: number; decay: number; sustain: number; release: number }
export interface Point { x: number; y: number }
/** Draggable points: attack peak, decay end (decay + sustain), release start (release + sustain). */
export type Handle = 'attack' | 'decay' | 'release'

export const TIME_LIMIT = 10000
/** Knee of the logarithmic time axis in ms: durations near it and below stay roughly linear. */
const T0 = 1
/** Smallest plateau share of the graph; drags are clamped so it never shrinks below this. */
export const MIN_HOLD = 0.08
/** Plateau share the automatic zoom leaves free. */
const AUTO_HOLD = 0.2

/**
 * Each timed stage is drawn with a width proportional to the log of its
 * duration, so short attacks and long releases are both visible. The whole
 * envelope always fits: the sustain plateau absorbs whatever width the timed
 * stages leave over. `scale` is the log width that spans the graph, the
 * zoom the user controls.
 */
export function logWidth(ms: number) { return Math.log2(1 + Math.max(0, ms) / T0) }
export function logTime(width: number) { return T0 * (2 ** Math.max(0, width) - 1) }
export function timedWidth(e: Envelope) { return logWidth(e.attack) + logWidth(e.decay) + logWidth(e.release) }

/** Log width of a graph that shows the full parameter range three times over: the zoom slider's upper end. */
export const MAX_SCALE = Math.ceil(3 * logWidth(TIME_LIMIT) / (1 - MIN_HOLD))
/** Log width of a graph for very short envelopes (about a quarter second per stage). */
const MIN_SCALE = 3 * logWidth(60) / (1 - AUTO_HOLD)
/** Smallest zoom at which this envelope still leaves the minimum plateau. */
export function minScale(e: Envelope) { return Math.max(MIN_SCALE, timedWidth(e) / (1 - MIN_HOLD)) }
/** Automatic zoom: the timed stages take four fifths of the graph. */
export function autoScale(e: Envelope) { return Math.max(MIN_SCALE, timedWidth(e) / (1 - AUTO_HOLD)) }
/** The zoom actually used: never tighter than the envelope needs. */
export function fitScale(e: Envelope, scale: number) { return Math.max(scale, minScale(e)) }

/** Envelope polyline in unit space: x over the graph (0–1), y as level (0–1, 1 at the top). The release always ends at x = 1. */
export function envelopePoints(e: Envelope, scale = autoScale(e)): Point[] {
  const s = fitScale(e, scale), x = (w: number) => w / s
  const a = logWidth(e.attack), d = logWidth(e.decay), r = logWidth(e.release)
  return [
    { x: 0, y: 0 },
    { x: x(a), y: 1 },
    { x: x(a + d), y: e.sustain },
    { x: 1 - x(r), y: e.sustain },
    { x: 1, y: 0 },
  ]
}

/**
 * Parameters implied by dragging a handle to a unit-space position, where
 * `unit.y` is 0 at the top of the graph. The zoom is frozen by the caller for
 * the duration of a drag, and drags are clamped so the plateau keeps its
 * minimum width: the graph never rescales because of a drag. The release
 * handle measures back from the right edge, so dragging it left lengthens
 * the release.
 */
export function dragHandle(e: Envelope, handle: Handle, unit: Point, scale: number): Partial<Envelope> {
  const w = Math.min(1, Math.max(0, unit.x)) * scale
  const level = Math.round(Math.min(1, Math.max(0, 1 - unit.y)) * 100) / 100
  const time = (width: number) => Math.round(Math.min(TIME_LIMIT, Math.max(0, logTime(width))))
  const a = logWidth(e.attack), d = logWidth(e.decay), r = logWidth(e.release), room = scale * (1 - MIN_HOLD)
  switch (handle) {
    case 'attack': return { attack: time(Math.min(w, room - d - r)) }
    case 'decay': return { decay: time(Math.min(w - a, room - a - r)), sustain: level }
    case 'release': return { release: time(Math.min(scale - w, room - a - d)), sustain: level }
  }
}

/** Short label for a duration, e.g. "120 ms" or "2.4 s". */
export function timeLabel(ms: number) { return ms >= 1000 ? `${(ms / 1000).toPrecision(ms >= 10000 ? 3 : 2)} s` : `${Math.round(ms)} ms` }

/** Shared descriptor defaults and live values for ADSR and sampler envelope views.
 * A connected parameter without telemetry has no trustworthy display value. */
export function readEnvelope(parameters: Record<string, number>, defaults: {id:string;default:number}[], live?: Record<string,number>, connected: string[] = []): Envelope | null {
  const result = {} as Envelope
  for (const key of ['attack','decay','sustain','release'] as const) {
    if (connected.includes(key) && live?.[key] === undefined) return null
    result[key] = live?.[key] ?? parameters[key] ?? defaults.find(p=>p.id===key)?.default ?? 0
  }
  return result
}
