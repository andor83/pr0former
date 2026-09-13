<script setup lang="ts">
import { ref } from 'vue'
import { showHelp } from '../help'
// An explanatory note. With help shown it is an ordinary paragraph; with help
// hidden (toggle with the i key) it collapses to an info icon whose text
// appears in a fixed-position tooltip on hover or focus, teleported to the
// body so scrolling modals cannot clip it.
defineOptions({ inheritAttrs: false })
defineProps<{ variant?: string }>()
const icon = ref<HTMLElement>()
const open = ref(false)
const tip = ref({ x: 0, y: 0, below: true })
function show() {
  const r = icon.value?.getBoundingClientRect()
  if (!r) return
  const below = r.bottom + 180 < innerHeight
  tip.value = { x: Math.min(innerWidth - 16, Math.max(16, r.left + r.width / 2)), y: below ? r.bottom + 6 : r.top - 6, below }
  open.value = true
}
function hide() { open.value = false }
</script>
<template>
  <p v-if="showHelp" v-bind="$attrs" :class="variant ?? 'feature-note'"><slot /></p>
  <span v-else ref="icon" class="help-icon" v-bind="$attrs" role="button" tabindex="0" aria-label="Show explanation" :aria-expanded="open" @mouseenter="show" @mouseleave="hide" @focus="show" @blur="hide" @click.stop.prevent="open ? hide() : show()" @keydown.enter.prevent="open ? hide() : show()" @keydown.escape="hide">
    i
    <Teleport to="body"><span v-if="open" class="help-tip" :class="{ above: !tip.below }" role="tooltip" :style="{ left: `${tip.x}px`, top: `${tip.y}px` }"><slot /></span></Teleport>
  </span>
</template>
<style scoped>
.help-icon{display:inline-flex;align-items:center;justify-content:center;width:15px;height:15px;border-radius:50%;border:1px solid #5a6e70;color:#8fa3a5;font:italic 600 10px/1 'Space Grotesk',serif;vertical-align:middle;margin:2px 4px 0 0;cursor:help;user-select:none;flex:none}
.help-icon:hover,.help-icon[aria-expanded="true"]{border-color:var(--cyan);color:var(--cyan)}.help-icon:focus-visible{outline:2px solid var(--cyan);outline-offset:2px}
.help-tip{position:fixed;z-index:1000;transform:translateX(-50%);max-width:min(360px,calc(100vw - 32px));padding:9px 12px;border-radius:6px;background:#1c2425;border:1px solid #3b4a4c;box-shadow:0 10px 30px #0008;color:#c5d2d2;font-size:11px;line-height:1.55;pointer-events:none;white-space:normal}
.help-tip.above{transform:translate(-50%,-100%)}
</style>
