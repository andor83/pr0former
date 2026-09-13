<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, useId, watch } from 'vue'
import { showHelp } from '../help'
// Native popovers join the top layer above the active modal. z-index alone
// cannot lift a body tooltip above dialog.showModal().
defineOptions({ inheritAttrs: false })
defineProps<{ variant?: string; label?: string }>()
const id = useId()
const icon = ref<HTMLElement>()
const host = ref<HTMLElement | string>('body')
const nativePopover = 'showPopover' in HTMLElement.prototype
let closeTimer: ReturnType<typeof setTimeout> | undefined
const popup = ref<HTMLElement>()
const open = ref(false)
const inlineHost = ref<HTMLElement>()
// Keep the icon beside its title, but put expanded prose back in the surrounding
// content area, outside headings and form labels. This mount point belongs only
// to this HelpNote; Vue owns its contents through Teleport.
onMounted(() => {
  const anchor = icon.value
  if (!anchor) return
  const container = document.createElement('div')
  container.className = 'help-inline-container'
  container.hidden = !showHelp.value
  const node = anchor.closest('.patch-node')
  if (node) node.append(container)
  else {
    const title = anchor.closest('label,h1,h2,h3,h4,summary,.help-section-title')
    const heading = anchor.closest('header,.modal-header,.parameter-heading,.section-heading,.debug-heading,.management-heading')
    const position = heading ?? title ?? anchor.parentElement!
    if (position.matches('.modal-header')) container.classList.add('help-modal-description')
    position.after(container)
  }
  inlineHost.value = container
})
async function show() {
  clearTimeout(closeTimer)
  host.value = icon.value?.closest('dialog') ?? 'body'
  open.value = true
  await nextTick()
  const button = icon.value, tip = popup.value
  if (!button || !tip || !open.value) return
  tip.showPopover?.()
  const anchor = button.getBoundingClientRect(), box = tip.getBoundingClientRect()
  tip.style.left = `${Math.max(12, Math.min(innerWidth - box.width - 12, anchor.left))}px`
  const below = anchor.bottom + 8
  tip.style.top = `${Math.max(12, Math.min(innerHeight - box.height - 12, below + box.height <= innerHeight - 12 ? below : anchor.top - box.height - 8))}px`
}
function hide() { clearTimeout(closeTimer); popup.value?.hidePopover?.(); open.value = false }
function leave() { closeTimer = setTimeout(hide, 160) }
function stay() { clearTimeout(closeTimer) }
watch(showHelp, value => {
  hide()
  if (inlineHost.value) inlineHost.value.hidden = !value
})
function dismiss(event: Event) {
  if (event.type === 'scroll' && popup.value?.contains(event.target as Node)) return
  if (event.type === 'keydown' && (event as KeyboardEvent).key !== 'Escape') return
  if (event.type === 'pointerdown' && (icon.value?.contains(event.target as Node) || popup.value?.contains(event.target as Node))) return
  hide()
}
// Close stale anchors on scrolling/resizing and touch popups on outside taps.
window.addEventListener('scroll', dismiss, true)
window.addEventListener('resize', dismiss)
window.addEventListener('pointerdown', dismiss)
window.addEventListener('keydown', dismiss)
onBeforeUnmount(() => {
  hide()
  inlineHost.value?.remove()
  window.removeEventListener('scroll', dismiss, true)
  window.removeEventListener('resize', dismiss)
  window.removeEventListener('pointerdown', dismiss)
  window.removeEventListener('keydown', dismiss)
})
</script>
<template>
  <span class="help-anchor" v-bind="$attrs">
    <span ref="icon" role="button" tabindex="0" class="help-icon nodrag nopan" :aria-label="label ? `About ${label}` : 'Show explanation'" :aria-expanded="open" :aria-describedby="open ? id : undefined" @pointerenter="event => event.pointerType === 'mouse' && show()" @pointerleave="event => event.pointerType === 'mouse' && leave()" @focus="show" @blur="leave" @click.stop.prevent="show" @pointerdown.stop.prevent @dblclick.stop @keydown.enter.stop.prevent="show" @keydown.space.stop.prevent="show" @keydown.escape.stop.prevent="hide"></span>
    <Teleport v-if="inlineHost && showHelp" :to="inlineHost"><div class="help-inline" :class="variant" role="note" @click.stop @pointerdown.stop @dblclick.stop><slot /></div></Teleport>
    <Teleport :to="host"><span v-if="open" :id="id" ref="popup" :popover="nativePopover ? 'manual' : undefined" class="help-tip" role="tooltip" @pointerenter="stay" @pointerleave="leave"><slot /></span></Teleport>
  </span>
</template>
<style scoped>
.help-inline{padding:8px 0 12px;color:#aababc;font:400 12px/1.65 'DM Sans',sans-serif;letter-spacing:normal;text-transform:none;white-space:normal;overflow-wrap:anywhere;text-align:left}
.help-anchor{display:inline-flex;vertical-align:middle;flex:none;margin-inline-start:4px}
.help-icon{display:inline-flex;align-items:center;justify-content:center;width:24px;height:24px;border-radius:50%;color:#a0b4b6;font:italic 600 12px/1 'Space Grotesk',serif;cursor:help;user-select:none;flex:none;border:0;background:transparent}
.help-icon::before{content:'i'}.help-icon::after{content:'';position:absolute;width:14px;height:14px;border:1px solid currentColor;border-radius:50%}
.help-icon:hover,.help-icon[aria-expanded="true"]{color:var(--cyan)}.help-icon:focus-visible{outline:2px solid var(--cyan);outline-offset:1px}
.help-tip{position:fixed;inset:auto;margin:0;box-sizing:border-box;width:max-content;max-width:min(360px,calc(100vw - 24px));max-height:calc(100dvh - 24px);overflow:auto;padding:10px 13px;border-radius:6px;background:#1c2425;border:1px solid #3b4a4c;box-shadow:0 10px 30px #0008;color:#c5d2d2;font:400 12px/1.55 'DM Sans',sans-serif;white-space:normal;z-index:10000}
@media(pointer:coarse){.help-icon{width:36px;height:36px}}
</style>
