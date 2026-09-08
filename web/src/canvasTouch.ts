import { onBeforeUnmount, watch, type Ref } from 'vue'
import { useVueFlow } from '@vue-flow/core'

// Leave single-pointer node dragging and marquee selection to Vue Flow. Own
// multi-touch here because its mouse-button pan filter excludes touch events.
export function useCanvasTouch(canvas: Ref<HTMLElement | undefined>) {
  const { viewport, setViewport, getSelectedNodes, addSelectedNodes, removeSelectedNodes } = useVueFlow()
  let dispose = () => {}
  watch(canvas, element => {
    dispose()
    if (!element) return
    const points = new Map<number, { x: number; y: number; target: HTMLElement }>()
    let navigating = false
    let finishing = false
    let nativeTouches: Touch[] = []
    let previous: { x: number; y: number; distance: number } | undefined
    let selection: typeof getSelectedNodes.value = []
    const geometry = () => {
      const [a, b] = [...points.values()]
      if (!a || !b) return undefined
      const rect = element.getBoundingClientRect()
      return { x: (a.x + b.x) / 2 - rect.left, y: (a.y + b.y) / 2 - rect.top, distance: Math.hypot(b.x - a.x, b.y - a.y) }
    }
    const down = (event: PointerEvent) => {
      if (event.pointerType !== 'touch') return
      const target = event.target as HTMLElement
      if (!target.closest('.vue-flow') || target.closest('button,input,select,textarea,.vue-flow__handle,.vue-flow__controls')) return
      if (!points.size) selection = [...getSelectedNodes.value]
      points.set(event.pointerId, { x: event.clientX, y: event.clientY, target })
      if (points.size < 2 && !navigating) return
      if (!navigating) {
        // Finish Vue Flow's first-finger interaction before taking ownership.
        finishing = true
        for (const [id, point] of points) {
          if (id === event.pointerId) continue
          point.target.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: id, pointerType: 'touch', button: 0, clientX: point.x, clientY: point.y }))
          point.target.dispatchEvent(new TouchEvent('touchcancel', { bubbles: true, changedTouches: nativeTouches }))
        }
        finishing = false
        removeSelectedNodes(getSelectedNodes.value)
        addSelectedNodes(selection)
      }
      navigating = true
      previous = geometry()
      for (const id of points.keys()) element.setPointerCapture(id)
      event.preventDefault()
      event.stopPropagation()
    }
    const move = (event: PointerEvent) => {
      const point = points.get(event.pointerId)
      if (!point) return
      point.x = event.clientX; point.y = event.clientY
      if (!navigating) return
      event.preventDefault(); event.stopPropagation()
      const current = geometry()
      if (current && previous) {
        const view = viewport.value
        const zoom = Math.max(.2, Math.min(2, view.zoom * current.distance / Math.max(1, previous.distance)))
        const scale = zoom / view.zoom
        void setViewport({ x: current.x - (previous.x - view.x) * scale, y: current.y - (previous.y - view.y) * scale, zoom })
      }
      previous = current
    }
    const up = (event: PointerEvent) => {
      if (finishing || !points.has(event.pointerId)) return
      points.delete(event.pointerId)
      if (navigating) { event.preventDefault(); event.stopPropagation() }
      else if (event.type === 'pointercancel') {
        const target = event.target as HTMLElement
        target.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: event.pointerId, button: 0 }))
      }
      previous = geometry()
      // Never resume a one-finger drag after lifting one finger from a pinch.
      if (!points.size) navigating = false
    }
    const touch = (event: TouchEvent) => {
      if (navigating) { event.preventDefault(); event.stopPropagation() }
      else nativeTouches = Array.from(event.touches)
    }
    element.addEventListener('pointerdown', down, true)
    element.addEventListener('pointermove', move, true)
    element.addEventListener('pointerup', up, true)
    element.addEventListener('pointercancel', up, true)
    element.addEventListener('touchstart', touch, { capture: true, passive: false })
    element.addEventListener('touchmove', touch, { capture: true, passive: false })
    dispose = () => {
      element.removeEventListener('pointerdown', down, true)
      element.removeEventListener('pointermove', move, true)
      element.removeEventListener('pointerup', up, true)
      element.removeEventListener('pointercancel', up, true)
      element.removeEventListener('touchstart', touch, true)
      element.removeEventListener('touchmove', touch, true)
    }
  }, { flush: 'post' })
  onBeforeUnmount(() => dispose())
}
