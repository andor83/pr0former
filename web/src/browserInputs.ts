import { reactive, ref } from 'vue'

// Physical browser device IDs are local session choices, never project data.
export const browserInputs = ref<MediaDeviceInfo[]>([])
export const browserInputMessage = ref('Checking local audio inputs…')
export const browserInputChoices = reactive<Record<string, string>>({})
export const browserInputState = reactive<Record<string, string>>({})
export const browserInputBusy = reactive<Record<string, boolean>>({})
export function captureError(error: unknown): string {
  const name = error instanceof Error ? error.name : ''
  if (name === 'NotAllowedError' || name === 'SecurityError') return 'Microphone permission is denied or blocked by application policy. Allow microphone access in your browser or app settings.'
  if (name === 'NotFoundError') return 'No local audio inputs are available. Connect an input and refresh.'
  if (name === 'OverconstrainedError') return 'The selected local audio input is unavailable. Refresh and select another input.'
  if (name === 'NotReadableError') return 'This session could not open the input. It may be in use or blocked by the operating system.'
  return error instanceof Error ? error.message : String(error)
}
export async function refreshBrowserInputs() {
  if (!window.isSecureContext) { browserInputs.value = []; browserInputMessage.value = 'Local audio input requires trusted HTTPS or localhost.'; return }
  if (!navigator.mediaDevices?.enumerateDevices) { browserInputs.value = []; browserInputMessage.value = 'This browser or app session does not support audio input discovery.'; return }
  try {
    browserInputs.value = (await navigator.mediaDevices.enumerateDevices()).filter(d => d.kind === 'audioinput')
    let permission = ''
    try { permission = (await navigator.permissions.query({ name: 'microphone' as PermissionName })).state } catch { /* Not supported by all browsers. */ }
    browserInputMessage.value = permission === 'denied' ? 'Microphone permission is denied or blocked by application policy. Allow microphone access in your browser or app settings.'
      : !browserInputs.value.length ? 'No local audio inputs are available or exposed. Grant microphone access to check, or connect an input and refresh.'
      : browserInputs.value.some(d => !d.label) ? 'Microphone permission is needed to reveal device names and all available inputs.'
      : `${browserInputs.value.length} local audio input${browserInputs.value.length === 1 ? '' : 's'} available.`
  } catch (error) { browserInputs.value = []; browserInputMessage.value = captureError(error) }
}

export interface LocalAudioAccess {userId?:string;assignments:Record<string,string>;members:import('./types').Member[];canAssign:boolean}
export interface RemoteAudioInput {sending:boolean;project_id:string;node:string;user_name:string;machine_name:string;state:string}
export const remoteAudioInputs=ref<RemoteAudioInput[]>([])
const nativeName=(window as Window & {__PR0_MACHINE_NAME__?:string}).__PR0_MACHINE_NAME__
export const localMachineName=ref((()=>{try{return localStorage.getItem('pr0former.machine-name')||nativeName||(/iPad/.test(navigator.userAgent)?'iPad':/Mac/.test(navigator.userAgent)?'Mac browser':/Windows/.test(navigator.userAgent)?'Windows browser':'Browser device')}catch{return nativeName||'Browser device'}})())
export function saveMachineName(value:string){localMachineName.value=value.trim().slice(0,80)||'Browser device';try{localStorage.setItem('pr0former.machine-name',localMachineName.value)}catch{}}
