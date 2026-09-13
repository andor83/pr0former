import { ref } from 'vue'
const KEY = 'pr0former.help'
function load() { try { return localStorage.getItem(KEY) !== 'hidden' } catch { return true } }
/** Whether explanatory notes are shown in full or collapsed to info icons. Toggled globally with the i key. */
export const showHelp = ref(load())
export function toggleHelp() {
  showHelp.value = !showHelp.value
  try { localStorage.setItem(KEY, showHelp.value ? 'shown' : 'hidden') } catch { /* private mode */ }
}
