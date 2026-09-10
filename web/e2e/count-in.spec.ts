import { test, expect } from '@playwright/test'

for (const feed of ['', 'cue']) {
  test(`count-in reaches ${feed || 'master'} browser monitor before playback`, async ({ page }) => {
    const headers = { 'X-Pr0former': '1' }
    const status = await (await page.request.get('/api/status')).json()
    await page.request.post(`/api/${status.bootstrap ? 'register' : 'login'}`, { headers, data: { username: 'browser-test', password: 'test1234' } })
    let project = await (await page.request.post('/api/projects', { headers, data: { name: `Count-in ${feed || 'master'}`, mode: 'structured' } })).json()
    project.bpm = 60; project.beats_per_bar = 6; project.beat_unit = 8; project.parts = []
    // Silent patch: any received audio energy during the count comes from clicks.
    project.graph = { nodes: [{ id: 'cue', kind: 'monitor_output', label: 'Cue', x: 0, y: 0, channels: 2, parameters: {} }], edges: [] }
    project = await (await page.request.put(`/api/projects/${project.id}`, { headers, data: project })).json()
    // Exercise both resampling directions and the smallest DSP block size.
    expect((await page.request.put(`/api/projects/${project.id}/system/audio`, {headers,data:{sample_rate:feed ? 96000 : 44100,block_size:32,interfaces:[],input_interfaces:[]}})).ok()).toBe(true)
    const transport = (action: string, count_in_beats = 0) => page.request.post(`/api/projects/${project.id}/transport`, { headers, data: { action, count_in_beats } })
    await page.addInitScript(() => {
      const Native = RTCPeerConnection
      window.RTCPeerConnection = class extends Native { constructor(config?: RTCConfiguration) { super(config); (window as any).__countPeer = this } }
    })
    let latest: any
    const packets: any[] = []
    page.on('websocket', socket => socket.on('framereceived', ({ payload }) => {
      const event = JSON.parse(String(payload))
      if (event.type === 'telemetry') { latest = event; packets.push(event) }
    }))
    await page.goto('/')
    await expect(page.getByLabel('Count in', { exact: true })).toHaveValue('bar')
    await expect(page.getByRole('option', { name: '1 bar (6 beats)' })).toHaveCount(1)
    await page.getByRole('button', { name: 'Enable audio engine', exact: true }).click()
    await page.getByRole('button', { name: 'Monitor', exact: true }).click()
    await page.getByLabel('Monitor feed').selectOption(feed)
    await page.getByRole('button', { name: 'Connect monitor', exact: true }).click()
    await expect(page.locator('.browser-monitor .mode-pill')).toHaveText('CONNECTED', { timeout: 20000 })
    const energy = () => page.evaluate(async () => {
      let energy = 0
      ;(await (window as any).__countPeer.getStats()).forEach((report: any) => {
        if (report.type === 'inbound-rtp' && report.kind === 'audio') energy = report.totalAudioEnergy || 0
      })
      return energy
    })
    const before = await energy()
    await page.getByRole('button', { name: 'Play', exact: true }).click()
    await expect.poll(() => latest?.count_in_remaining).toBe(6)
    expect(latest.beat).toBe(0); expect(latest.running).toBe(false)
    await expect(page.locator('.count-in-position')).toContainText('COUNT IN')
    await expect(page.getByLabel('Count in', { exact: true })).toBeDisabled()
    await expect.poll(energy).toBeGreaterThan(before)
    expect(latest.running).toBe(false)
    await expect.poll(() => latest?.running).toBe(true)
    expect(packets.filter(p => p.count_in_remaining != null).every(p => p.beat === 0 && !p.running)).toBe(true)
    expect(latest.count_in_remaining).toBeNull()
    // Pause/resume does not insert another count-in once the score has advanced.
    await page.getByRole('button', { name: 'Pause', exact: true }).click()
    await expect.poll(() => latest?.running).toBe(false)
    await page.getByRole('button', { name: 'Play', exact: true }).click()
    await expect.poll(() => latest?.running).toBe(true)
    expect(latest.count_in_remaining).toBeNull()
    await page.getByRole('button', { name: 'Stop', exact: true }).click()
    await expect.poll(() => latest?.beat).toBe(0)
    await page.getByLabel('Count in', { exact: true }).selectOption('2')
    await page.getByRole('button', { name: 'Play', exact: true }).click()
    await expect.poll(() => latest?.count_in_remaining).toBe(2)
    await page.getByRole('button', { name: 'Pause', exact: true }).click()
    await expect.poll(() => latest?.count_in_remaining).toBeNull()
    await page.waitForTimeout(1100)
    expect(latest.running).toBe(false); expect(latest.beat).toBe(0)
    for (const count of [-1, 1.5, 33, 256]) expect((await transport('play', count)).ok()).toBe(false)
    await page.getByRole('button', { name: 'Disable audio engine', exact: true }).click()
    await expect(page.getByRole('button', { name: 'Enable audio engine', exact: true })).toBeVisible()
    await page.getByLabel('Count in', { exact: true }).selectOption('0')
    await page.getByRole('button', { name: 'Play', exact: true }).click()
    await expect.poll(() => latest?.running).toBe(true)
    expect(latest.count_in_remaining).toBeNull()
    await transport('deactivate')
  })
}
