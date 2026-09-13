const toggle = document.getElementById('toggle')
const port = document.getElementById('port')
const error = document.getElementById('error')
let enabled = false
function render(status) {
  enabled = !!status.enabled
  toggle.textContent = enabled ? 'Stop Hosting' : 'Start Hosting'
  port.disabled = enabled
  document.getElementById('details').hidden = !enabled
  document.getElementById('url').value = status.url || ''
  document.getElementById('setup').value = status.setup_url || ''
  document.getElementById('certificate').textContent = status.ca_path ? `Public CA certificate: ${status.ca_path}` : ''
  document.getElementById('fingerprint').textContent = status.ca_fingerprint || ''
  document.getElementById('server-fingerprint').textContent = status.server_fingerprint || ''
  error.textContent = status.error || ''
}
async function control(change) {
  toggle.disabled = true
  error.textContent = ''
  try { render(await window.__TAURI__.core.invoke('hosting_control', { enabled: change, port: Number(port.value) })) }
  catch (reason) { error.textContent = String(reason) }
  finally { toggle.disabled = false }
}
toggle.addEventListener('click', () => {
  if (!enabled && !port.reportValidity()) return
  void control(!enabled)
})
void control(null)
