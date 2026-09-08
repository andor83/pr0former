import { onBeforeUnmount, ref } from 'vue'

export type LibraryItem = { kind: string } | { library: string; version: number }

// Touch cannot rely on HTML drag-and-drop. A sideways drag places an item;
// vertical swipes scroll the list, and holding briefly enables any-direction drag.
export function useLibraryTouch(options: {
  context: () => string
  insert: (item: LibraryItem, point?: { x: number; y: number }) => void
}) {
  const preview = ref<{ label: string; x: number; y: number }>()
  let cleanup = () => {}
  function start(event: PointerEvent, item: LibraryItem, label: string) {
    if (event.pointerType !== 'touch' || !event.isPrimary) return
    const button = event.currentTarget as HTMLButtonElement
    if (button.disabled) return
    cleanup()
    event.preventDefault(); event.stopPropagation()
    button.setPointerCapture(event.pointerId)
    const context = options.context()
    const origin = { x: event.clientX, y: event.clientY }
    let previousY = origin.y
    let mode: 'pending' | 'drag' | 'scroll' = 'pending'
    const scroller = button.closest('.library-list,.subgraph-library-content') as HTMLElement | null
    const drag = (x: number, y: number) => { mode = 'drag'; preview.value = { label, x, y } }
    const hold = setTimeout(() => drag(origin.x, origin.y), 250)
    const move = (e: PointerEvent) => {
      if (e.pointerId !== event.pointerId) return
      e.preventDefault(); e.stopPropagation()
      const dx = e.clientX - origin.x, dy = e.clientY - origin.y
      if (mode === 'pending' && Math.hypot(dx, dy) > 8) {
        clearTimeout(hold)
        mode = Math.abs(dx) >= Math.abs(dy) ? 'drag' : 'scroll'
      }
      if (mode === 'drag') drag(e.clientX, e.clientY)
      if (mode === 'scroll' && scroller) scroller.scrollTop -= e.clientY - previousY
      previousY = e.clientY
    }
    const end = (e: PointerEvent) => {
      if (e.pointerId !== event.pointerId) return
      e.preventDefault(); e.stopPropagation()
      const complete = e.type === 'pointerup' && context === options.context()
      const tap = mode === 'pending'
      const drop = mode === 'drag' && document.elementFromPoint(e.clientX, e.clientY)?.closest('.graph-canvas .vue-flow')
      cleanup()
      if (complete && (tap || drop)) options.insert(item, drop ? { x: e.clientX, y: e.clientY } : undefined)
    }
    const anotherFinger = (e: PointerEvent) => { if (e.pointerType === 'touch' && e.pointerId !== event.pointerId) cleanup() }
    const suppressClick = (e: MouseEvent) => { e.preventDefault(); e.stopPropagation() }
    // Cancel the compatibility click so a drop cannot also insert at the center.
    button.addEventListener('click', suppressClick, true)
    window.addEventListener('pointermove', move, true)
    window.addEventListener('pointerup', end, true)
    window.addEventListener('pointercancel', end, true)
    window.addEventListener('pointerdown', anotherFinger, true)
    window.addEventListener('blur', cancel)
    function cancel() { cleanup() }
    cleanup = () => {
      clearTimeout(hold)
      preview.value = undefined
      if (button.hasPointerCapture(event.pointerId)) button.releasePointerCapture(event.pointerId)
      window.removeEventListener('pointermove', move, true)
      window.removeEventListener('pointerup', end, true)
      window.removeEventListener('pointercancel', end, true)
      window.removeEventListener('pointerdown', anotherFinger, true)
      window.removeEventListener('blur', cancel)
      setTimeout(() => button.removeEventListener('click', suppressClick, true), 0)
      cleanup = () => {}
    }
  }
  onBeforeUnmount(() => cleanup())
  return { start, preview }
}
