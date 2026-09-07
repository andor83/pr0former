import { reactive, ref } from 'vue'

// Physical browser device IDs are local session choices, never project data.
export const browserInputs = ref<MediaDeviceInfo[]>([])
export const browserInputMessage = ref('Checking browser audio inputs…')
export const browserInputChoices = reactive<Record<string, string>>({})
export const browserInputBusy = reactive<Record<string, boolean>>({})
export function captureError(error: unknown): string {
  const name = error instanceof Error ? error.name : ''
  if (name === 'NotAllowedError' || name === 'SecurityError') return 'Microphone permission is denied or blocked by browser policy. Allow microphone access in this site’s browser settings.'
  if (name === 'NotFoundError') return 'No browser audio inputs are available. Connect an input and refresh.'
  if (name === 'OverconstrainedError') return 'The selected browser audio input is unavailable. Refresh and select another input.'
  if (name === 'NotReadableError') return 'The browser could not open this input. It may be in use or blocked by the operating system.'
  return error instanceof Error ? error.message : String(error)
}
export async function refreshBrowserInputs() {
  if (!window.isSecureContext) { browserInputs.value = []; browserInputMessage.value = 'Browser audio input requires trusted HTTPS or localhost.'; return }
  if (!navigator.mediaDevices?.enumerateDevices) { browserInputs.value = []; browserInputMessage.value = 'This browser does not support audio input discovery.'; return }
  try {
    browserInputs.value = (await navigator.mediaDevices.enumerateDevices()).filter(d => d.kind === 'audioinput')
    let permission = ''
    try { permission = (await navigator.permissions.query({ name: 'microphone' as PermissionName })).state } catch { /* Not supported by all browsers. */ }
    browserInputMessage.value = permission === 'denied' ? 'Microphone permission is denied or blocked by browser policy. Allow microphone access in this site’s browser settings.'
      : !browserInputs.value.length ? 'No browser audio inputs are available or exposed. Grant microphone access to check, or connect an input and refresh.'
      : browserInputs.value.some(d => !d.label) ? 'Microphone permission is needed to reveal device names and all available inputs.'
      : `${browserInputs.value.length} browser audio input${browserInputs.value.length === 1 ? '' : 's'} available.`
  } catch (error) { browserInputs.value = []; browserInputMessage.value = captureError(error) }
}
