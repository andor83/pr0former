<script setup lang="ts">
import { computed } from 'vue'
const props=defineProps<{values?:Record<string,number>;stale:boolean;partName?:string;interactive?:boolean}>()
const emit=defineEmits<{open:[]}>()
const status=computed(()=>!props.partName?'Unassigned':props.stale?'Engine inactive':props.values?._pending?'Waiting for beat':props.values?._playing?(props.values?._repeating?'Repeating':'Playing'):'Stopped')
</script>
<template>
  <component :is="interactive ? 'button' : 'div'" class="part-player-readout" :class="{'nodrag nopan':interactive}" :type="interactive ? 'button' : undefined" :aria-label="interactive ? `View playback of ${partName}` : undefined" @pointerdown.stop @dblclick.stop @click.stop="interactive && emit('open')" @keydown.stop @keyup.stop>
    <span class="part-player-name" :title="partName">{{partName || 'Choose a part in Options'}}</span>
    <output aria-label="Part player position">{{!stale && values?._bar ? `Bar ${values._bar} · Beat ${values._beat}` : 'Bar — · Beat —'}}</output>
    <span aria-label="Part player status">{{status}}</span>
  </component>
</template>
<style scoped>
.part-player-readout{display:flex;flex-direction:column;gap:5px;padding:10px 12px;border:1px solid var(--line);border-radius:9px;background:var(--panel);font-size:11px;color:var(--muted)}
button.part-player-readout{cursor:pointer;text-align:left;font-family:inherit}button.part-player-readout:hover{border-color:var(--amber)}button.part-player-readout:focus-visible{outline:2px solid var(--amber);outline-offset:2px}
.part-player-name{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
output{font-variant-numeric:tabular-nums;font-size:15px;color:var(--amber)}
</style>
