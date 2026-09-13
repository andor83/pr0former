/**
 * Per-project view memory in localStorage, so flipping between the Signal Graph
 * and Score tabs (which unmount each workspace) comes back to the same place.
 */
export interface GraphViewport {
  x: number
  y: number
  zoom: number
}
export interface ScoreView {
  focused?: string
  visible?: string[]
  collapsed?: boolean
  scrollLeft?: number
  scrollTop?: number
}
export const graphViewKey = (projectId: string) => `pr0former.graph.view.${projectId}`
export const scoreViewKey = (projectId: string) => `pr0former.score.view.${projectId}`
export function readView<T>(key: string, storage: Pick<Storage, 'getItem'> | undefined = safeStorage()): T | null {
  try {
    const raw = storage?.getItem(key)
    if (!raw) return null
    const value = JSON.parse(raw)
    return value && typeof value === 'object' ? (value as T) : null
  } catch {
    return null
  }
}
export function writeView(key: string, value: unknown, storage: Pick<Storage, 'setItem'> | undefined = safeStorage()) {
  try {
    storage?.setItem(key, JSON.stringify(value))
  } catch {
    /* private mode or quota */
  }
}
/** Graph viewports are kept per subgraph: `root` for the top level, else the parent node id. */
export function graphViewport(
  saved: Record<string, GraphViewport> | null,
  parent: string | null,
): GraphViewport | null {
  const v = saved?.[parent || 'root']
  return v && [v.x, v.y, v.zoom].every(Number.isFinite) && v.zoom > 0 ? v : null
}
/** Saved part choices only apply to parts that still exist; otherwise the defaults stand. */
export function restoreScoreParts(
  saved: ScoreView | null,
  partIds: string[],
  defaults: { focused: string; visible: Set<string> },
): { focused: string; visible: Set<string> } {
  const focused =
    saved?.focused && partIds.includes(saved.focused) ? saved.focused : defaults.focused
  const kept = Array.isArray(saved?.visible)
    ? saved.visible.filter((id) => partIds.includes(id))
    : null
  return { focused, visible: kept?.length ? new Set(kept) : defaults.visible }
}
function safeStorage(): Storage | undefined {
  try {
    return typeof localStorage === 'undefined' ? undefined : localStorage
  } catch {
    return undefined
  }
}
