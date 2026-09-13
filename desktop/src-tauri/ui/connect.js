const form = document.getElementById('connect-form')
const address = document.getElementById('address')
const button = document.getElementById('connect')
const error = document.getElementById('error')
let certificate = null
const trust = document.getElementById('trust')
address.addEventListener('input', () => { certificate = null; document.getElementById('certificate').hidden = true })
trust.addEventListener('click', () => { if (certificate) void connect(certificate.fingerprint) })
form.addEventListener('submit', event => { event.preventDefault(); void connect(null) })
async function connect(trustFingerprint) {
  error.textContent = ''
  button.disabled = true
  trust.disabled = true
  button.textContent = 'Connecting…'
  try {
    const value = address.value.trim()
    await window.__TAURI__.core.invoke('connect_server', { address: value, trustFingerprint })
  } catch (reason) {
    error.textContent = reason?.message || String(reason)
    certificate = reason?.certificate || null
    document.getElementById('certificate').hidden = !certificate
    if (certificate) {
      document.getElementById('certificate-origin').textContent = certificate.origin
      document.getElementById('fingerprint').textContent = certificate.fingerprint
      trust.textContent = certificate.changed ? 'Trust Changed Certificate' : 'Trust This Server'
    }
  } finally {
    button.disabled = false
    trust.disabled = false
    button.textContent = 'Connect'
  }
}
