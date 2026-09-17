// Geometry and configuration helpers for the Granular Field node. Mirrors the Rust
// descriptor: eight sample slots (sample_N + source_N_*) and two live inputs
// (live_N_*) that exist as sources only while a cable feeds their audio port.
import type { Descriptor, GraphEdge, GraphNode } from './types'

export const FIELD_MAX_SAMPLES = 8
export const FIELD_MAX_LIVE = 2
export const FIELD_STEP = 0.01
export const FIELD_STEP_LARGE = 0.1
/** Fresh slot positions, identical to the descriptor defaults: a ring of radius 0.7. */
const SLOT_RING: [number, number][] = [[0.7, 0], [0.49, 0.49], [0, 0.7], [-0.49, 0.49], [-0.7, 0], [-0.49, -0.49], [0, -0.7], [0.49, -0.49]]
const LIVE_DEFAULTS: [number, number][] = [[0, 0], [0.3, 0.3]]

export interface Point { x: number; y: number }
export interface FieldSource {
  /** 1-based telemetry index: samples 1..8, live inputs 9..10. */
  index: number
  kind: 'sample' | 'live'
  /** 1-based slot within its kind. */
  slot: number
  label: string
  asset?: number
  /** The node input that feeds this source: `sample_N` (asset id) or `live_N` (audio). */
  port: string
  keys: { x: string; y: string; tune: string; gain: string; buffer?: string }
  weightKey: string
  missingKey?: string
}

export function slotKeys(slot: number) {
  return { sample: `sample_${slot}`, x: `source_${slot}_x`, y: `source_${slot}_y`, tune: `source_${slot}_tune`, gain: `source_${slot}_gain` }
}
/** Default of any field parameter, matching the Rust descriptor. */
export function fieldDefault(key: string): number {
  let m = /^source_(\d+)_(x|y)$/.exec(key)
  if (m) return SLOT_RING[Number(m[1]) - 1]?.[m[2] === 'x' ? 0 : 1] ?? 0
  m = /^live_(\d+)_(x|y)$/.exec(key)
  if (m) return LIVE_DEFAULTS[Number(m[1]) - 1]?.[m[2] === 'x' ? 0 : 1] ?? 0
  if (/_gain$/.test(key)) return 1
  if (/_buffer_ms$/.test(key)) return 500
  if (key === 'focus') return 0.5
  return 0
}
export function fieldValue(node: GraphNode, key: string, live?: Record<string, number>) {
  return live?.[key] ?? node.parameters[key] ?? fieldDefault(key)
}

/** Configured sources: every sample slot plus each live input whose audio port is connected. */
export function fieldSources(node: GraphNode, connected: string[] = []): FieldSource[] {
  const sources: FieldSource[] = (node.sample_choices ?? []).slice(0, FIELD_MAX_SAMPLES).map((choice, i) => {
    const slot = i + 1, keys = slotKeys(slot)
    return { index: slot, kind: 'sample', slot, label: choice.nickname || choice.name, asset: choice.asset, port: keys.sample, keys, weightKey: `_source_${slot}_weight`, missingKey: `_sample_${slot}_missing` }
  })
  for (let slot = 1; slot <= FIELD_MAX_LIVE; slot++) {
    if (!connected.includes(`live_${slot}`)) continue
    sources.push({ index: FIELD_MAX_SAMPLES + slot, kind: 'live', slot, label: `Live ${slot}`, port: `live_${slot}`, keys: { x: `live_${slot}_x`, y: `live_${slot}_y`, tune: `live_${slot}_tune`, gain: `live_${slot}_gain`, buffer: `live_${slot}_buffer_ms` }, weightKey: `_source_${FIELD_MAX_SAMPLES + slot}_weight` })
  }
  return sources
}
export const clampField = (v: number) => Math.max(-1, Math.min(1, v))
export const roundField = (v: number) => Math.round(v * 100) / 100
export function sourcePoint(node: GraphNode, source: FieldSource, live?: Record<string, number>): Point {
  return { x: clampField(fieldValue(node, source.keys.x, live)), y: clampField(fieldValue(node, source.keys.y, live)) }
}
/** The control point; null while an axis is cabled and no live value has arrived, so nothing is invented. */
export function controlPoint(node: GraphNode, live?: Record<string, number>, connected: string[] = []): Point | null {
  if (live && live._x !== undefined && live._y !== undefined) return { x: clampField(live._x), y: clampField(live._y) }
  if (connected.includes('x') || connected.includes('y')) return null
  return { x: clampField(node.parameters.x ?? 0), y: clampField(node.parameters.y ?? 0) }
}
/** Field coordinates (-1..1, y up) to pixels inside a square of `size` with `pad` margins. */
export function toPixels(p: Point, size: number, pad: number): Point {
  const span = size - 2 * pad
  return { x: pad + (p.x + 1) / 2 * span, y: pad + (1 - p.y) / 2 * span }
}
export function fromPixels(px: Point, size: number, pad: number): Point {
  const span = size - 2 * pad
  return { x: clampField((px.x - pad) / span * 2 - 1), y: clampField(1 - (px.y - pad) / span * 2) }
}
/** Selection weights, the engine's formula: exp(-(distance/focus)^2), normalised to sum 1. */
export function sourceWeights(control: Point, points: Point[], focus: number): number[] {
  const f = Math.max(0.05, focus)
  const raw = points.map(p => Math.exp(-((control.x - p.x) ** 2 + (control.y - p.y) ** 2) / (f * f)))
  const total = raw.reduce((a, b) => a + b, 0)
  if (!points.length) return []
  return total < 1e-12 ? points.map(() => 1 / points.length) : raw.map(w => w / total)
}
const signed = (v: number) => (v < 0 ? '−' : '') + Math.abs(v).toFixed(2)
export function describePoint(p: Point) { return `x ${signed(p.x)}, y ${signed(p.y)}` }
/** Arrow-key movement; null for keys the pad does not handle. */
export function nudge(p: Point, key: string, shift: boolean): Point | null {
  const step = shift ? FIELD_STEP_LARGE : FIELD_STEP
  switch (key) {
    case 'ArrowLeft': return { x: roundField(clampField(p.x - step)), y: p.y }
    case 'ArrowRight': return { x: roundField(clampField(p.x + step)), y: p.y }
    case 'ArrowUp': return { x: p.x, y: roundField(clampField(p.y + step)) }
    case 'ArrowDown': return { x: p.x, y: roundField(clampField(p.y - step)) }
    default: return null
  }
}
function slotValues(params: Record<string, number>, slot: number) {
  const keys = slotKeys(slot)
  return Object.fromEntries(Object.values(keys).map(key => [key, params[key] ?? fieldDefault(key)])) as Record<string, number>
}
function writeSlot(params: Record<string, number>, slot: number, values: Record<string, number>) {
  const keys = slotKeys(slot)
  const source = Object.keys(values)
  Object.values(keys).forEach((key, i) => { params[key] = values[source[i]!]! })
}
/** Swap the settings of two slots so a reordered sample keeps its position, tune and gain. */
export function swapSlotParameters(params: Record<string, number>, a: number, b: number): Record<string, number> {
  const next = { ...params }, va = slotValues(params, a), vb = slotValues(params, b)
  writeSlot(next, a, vb); writeSlot(next, b, va)
  return next
}
/** Shift the slots above a removed one down by one and reset the freed last slot to defaults. */
export function removeSlotParameters(params: Record<string, number>, slot: number, count: number): Record<string, number> {
  const next = { ...params }
  for (let s = slot; s < count; s++) writeSlot(next, s, slotValues(params, s + 1))
  for (const key of Object.values(slotKeys(count))) delete next[key]
  return next
}
/** Edges into sample setters beyond the configured slots. */
export function pruneGranularEdges(edges: GraphEdge[], nodeId: string, slots: number): GraphEdge[] {
  return edges.filter(e => {
    if (e.target !== nodeId) return true
    const m = /^sample_(\d+)$/.exec(e.target_port)
    return !m || Number(m[1]) <= slots
  })
}
/** Ports and parameters the graph should show: per-source settings are edited in the field
 * pad and sources dialog, and a sample setter exists only for a configured slot. */
export function granularFieldDescriptor(base: Descriptor, node: GraphNode): Descriptor {
  const slots = Math.min(FIELD_MAX_SAMPLES, node.sample_choices?.length ?? 0)
  return {
    ...base,
    parameters: base.parameters.filter(p => {
      if (/^source_\d+_/.test(p.id) || /^live_\d+_(x|y|tune|gain|buffer_ms)$/.test(p.id)) return false
      const m = /^sample_(\d+)$/.exec(p.id)
      return !m || Number(m[1]) <= slots
    }),
  }
}
