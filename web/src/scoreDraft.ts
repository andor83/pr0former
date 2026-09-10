import { computed, ref, shallowRef } from 'vue'
import type { Project } from './types'

/** Project-owned draft: survives editor unmounts and serializes all score saves. */
export function createScoreDraft(source: () => Project, save: (project: Project) => Promise<void>) {
  const draft = shallowRef<Project | null>(null)
  const conflict = shallowRef<Project | null>(null)
  const error = ref('')
  const flushing = ref(false)
  const accepted = ref(0)
  const doc = computed(() => draft.value ?? source())
  const pending = computed(() => !!draft.value || flushing.value)
  let timer: ReturnType<typeof setTimeout> | undefined
  let flight: Promise<void> | undefined
  function flush(): Promise<void> {
    clearTimeout(timer)
    if (flight) return flight
    if (conflict.value) return Promise.reject(new Error(error.value || 'Resolve the retained score draft first.'))
    flushing.value = true
    flight = (async () => {
      while (draft.value) {
        const submitted = draft.value
        try {
          await save(submitted)
          if (draft.value === submitted) draft.value = null
          else draft.value = { ...draft.value!, revision: source().revision }
          accepted.value++
          error.value = ''
        } catch (cause) {
          error.value = cause instanceof Error ? cause.message : String(cause)
          // Retain edits made during the request, including validation failures.
          conflict.value = draft.value ?? submitted
          draft.value = null
          throw cause
        }
      }
    })().finally(() => { flushing.value = false; flight = undefined })
    return flight
  }
  function schedule(delay = 250) {
    clearTimeout(timer)
    timer = setTimeout(() => { void flush().catch(() => {}) }, delay)
  }
  return { draft, conflict, error, flushing, accepted, doc, pending, flush, schedule }
}
export type ScoreDraft = ReturnType<typeof createScoreDraft>
