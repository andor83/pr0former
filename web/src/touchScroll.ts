import { onBeforeUnmount, onMounted } from 'vue'

// Contain swipes inside actual scroll panels. CSS overscroll containment alone
// does not cover every iOS fullscreen/rubber-band path, especially panel edges.
export function useTouchScrollContainment() {
  let previous: { id: number; x: number; y: number } | undefined
  const start = (event: TouchEvent) => {
    const touch = event.touches[0]
    previous = touch ? { id: touch.identifier, x: touch.clientX, y: touch.clientY } : undefined
  }
  const move = (event: TouchEvent) => {
    const touch = event.touches[0]
    if (!touch || !previous || touch.identifier !== previous.id) return
    const dx = previous.x - touch.clientX, dy = previous.y - touch.clientY
    previous = { id: touch.identifier, x: touch.clientX, y: touch.clientY }
    if (!event.cancelable) return
    if (event.touches.length > 1) { event.preventDefault(); return }
    // Custom canvas gestures and controls still receive the event; only the
    // browser's default scrolling is cancelled.
    const horizontal = Math.abs(dx) > Math.abs(dy)
    const delta = horizontal ? dx : dy
    let element = event.target instanceof Element ? event.target : null
    while (element && element !== document.body && element !== document.documentElement) {
      const style = getComputedStyle(element)
      const overflow = horizontal ? style.overflowX : style.overflowY
      const extent = horizontal ? element.scrollWidth - element.clientWidth : element.scrollHeight - element.clientHeight
      if (/(auto|scroll)/.test(overflow) && extent > 0) {
        const position = horizontal ? element.scrollLeft : element.scrollTop
        const available = delta > 0 ? extent - position : position
        if (available >= Math.abs(delta) && delta !== 0) return
        // Clamp the last partial swipe without handing it to the browser UI.
        if (horizontal) element.scrollLeft += delta
        else element.scrollTop += delta
        event.preventDefault()
        return
      }
      element = element.parentElement
    }
    event.preventDefault()
  }
  const reset = () => { previous = undefined }
  onMounted(() => {
    document.addEventListener('touchstart', start, { capture: true, passive: true })
    document.addEventListener('touchmove', move, { capture: true, passive: false })
    document.addEventListener('touchend', reset, true)
    document.addEventListener('touchcancel', reset, true)
  })
  onBeforeUnmount(() => {
    document.removeEventListener('touchstart', start, true)
    document.removeEventListener('touchmove', move, true)
    document.removeEventListener('touchend', reset, true)
    document.removeEventListener('touchcancel', reset, true)
  })
}
