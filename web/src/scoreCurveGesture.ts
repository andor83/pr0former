import type { Project, StaffCurve } from './types'
import { isHairpin, retimeHairpin, updateCurve } from './scoreCurves'
export interface CurveGesture {
  part: string
  staff: string
  original: StaffCurve
  base: Project
  handle: 'start' | 'end' | 'shape'
}
/** Pure preview from the gesture origin. Neither saved drafts nor undo history are touched. */
export function previewCurveGesture(gesture: CurveGesture, beat: number, dy: number, note: string | null): Project {
  const { original: c, handle } = gesture
  const hairpin = isHairpin(c.kind)
  const patch: Partial<StaffCurve> = handle === 'shape'
    ? { height: hairpin ? Math.max(2,c.height-dy) : c.height+dy }
    : hairpin ? { lift: c.lift+dy, end_lift: (c.end_lift??0)+dy }
    : handle === 'start' ? { start_beat:beat,start_note:note,lift:c.lift+dy }
    : { end_beat:beat,end_note:note,end_lift:(c.end_lift??0)+dy }
  return { ...gesture.base, parts: gesture.base.parts.map(part => {
    if (part.id !== gesture.part) return part
    const next = hairpin && handle !== 'shape'
      ? retimeHairpin(part,gesture.staff,c,handle==='start'?beat:c.start_beat,handle==='end'?beat:c.end_beat)
      : part
    return updateCurve(next,gesture.staff,c.id,patch)
  }) }
}
