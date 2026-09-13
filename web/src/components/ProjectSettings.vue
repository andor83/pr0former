<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { X } from '@lucide/vue'
import { api } from '../api'
import type { Project } from '../types'
const props = defineProps<{ project: Project; active: boolean; editable: boolean }>()
const emit = defineEmits<{ close: []; saved: [project: Project] }>()
// Preserve the opened revision: concurrent edits must cause a conflict, not an overwrite.
const draft = ref<Project>(JSON.parse(JSON.stringify(props.project)))
const dialog = ref<HTMLDialogElement>()
const pending = ref(false), error = ref('')
async function save() {
  if (pending.value || props.active || !props.editable || props.project.revision !== draft.value.revision) return
  pending.value = true; error.value = ''
  try {
    draft.value.name = draft.value.name.trim()
    if (!draft.value.name) throw new Error('Enter a project name.')
    const saved = await api<Project>(`/projects/${draft.value.id}`, 'PUT', draft.value)
    emit('saved', saved)
  } catch (e) { error.value = e instanceof Error ? e.message : String(e) }
  finally { pending.value = false }
}
function cancel(event: Event) { if (pending.value) event.preventDefault(); else emit('close') }
onMounted(() => dialog.value?.showModal())
</script>
<template>
  <dialog ref="dialog" class="project-settings dialog-card" aria-labelledby="project-settings-title" @cancel="cancel" @close="$emit('close')">
    <form @submit.prevent="save">
      <header><h2 id="project-settings-title">Project settings</h2><button type="button" class="icon-button" aria-label="Close project settings" :disabled="pending" @click="$emit('close')"><X :size="20" /></button></header>
      <fieldset :disabled="pending || active || !editable">
        <label>Project name<input v-model="draft.name" maxlength="120" required autofocus></label>
        <label>Performance mode<select v-model="draft.mode" aria-label="Performance mode"><option value="structured">Structured — shared score timeline</option><option value="conducted">Conducted — conductor launches parts</option><option value="freeform">Freeform — assigned performers launch parts</option></select></label>
        <label><span class="field-title">Initial tempo (quarter-note BPM)<HelpNote label="Initial tempo">Initial tempo applies when the show is activated. During a show, conductors can change tempo using the transport.</HelpNote></span><input aria-label="Initial tempo (quarter-note BPM)" v-model.number="draft.bpm" type="number" min="1" max="400" step="any" required></label>
      </fieldset>
      <p v-if="active" role="status">Deactivate the show to change project settings.</p>

      <p v-if="project.revision !== draft.revision" class="field-error" role="status">The project changed after these settings opened. Close and reopen to use the latest revision.</p>
      <p v-if="error" class="field-error" role="alert">{{ error }}</p>
      <button class="button primary wide" :disabled="pending || active || !editable || project.revision !== draft.revision">{{ pending ? 'Saving…' : 'Save project settings' }}</button>
    </form>
  </dialog>
</template>
<style scoped>
.project-settings{color:var(--white);background:var(--panel);max-height:90dvh;overflow:auto}.project-settings::backdrop{background:#000a;backdrop-filter:blur(5px)}fieldset{border:0;padding:0;margin:0}select,input{width:100%}.button.wide{margin-top:20px}.field-error{margin-top:14px}header .icon-button{min-width:44px;min-height:44px}
</style>
