import { test, expect, type WebSocketRoute } from '@playwright/test'

test('stale and late timing never rewind the running beat display, while Stop still resets', async ({ page }) => {
  const headers = { 'X-Pr0former': '1' }
  const status = await (await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap ? 'register' : 'login'}`, { headers, data: { username: 'browser-test', password: 'test1234' } })
  const project = await (await page.request.post('/api/projects', { headers, data: { name: 'Clock display', mode: 'freeform' } })).json()
  let route: WebSocketRoute | undefined, latest: any, hold = false
  await page.routeWebSocket(`**/api/projects/${project.id}/events`, socket => {
    route = socket
    const server = socket.connectToServer()
    server.onMessage(data => {
      const message = JSON.parse(String(data))
      if (message.type === 'telemetry') { latest = message; if (hold) return }
      socket.send(data)
    })
  })
  try {
  await page.goto('/')
  await page.getByLabel('Count in', { exact: true }).selectOption('0')
  await page.getByRole('button', { name: 'Play', exact: true }).click()
  await expect.poll(() => latest?.running).toBe(true)
  hold = true
  const source = { ...latest, sequence: latest.sequence + 10000, beat: 3.99, bpm: 120 }
  route!.send(JSON.stringify(source))
  const position = page.locator('.position-display strong')
  await expect(position).toHaveText('002:01')
  await expect(page.locator('.engine-label')).toContainText('ENGINE STALE')
  await expect(position).toHaveText('002:01')
  // Authoritative progress, but behind the already displayed estimate.
  route!.send(JSON.stringify({ ...source, sequence: source.sequence + 1, beat: 3.995, server_time: latest.server_time }))
  await expect(page.locator('.engine-label')).toContainText('ENGINE RUNNING')
  await expect(position).toHaveText('002:01')
  route!.send(JSON.stringify({ ...source, sequence: source.sequence + 2, beat: 0, running: false }))
  await expect(position).toHaveText('001:01')
  } finally {
    await page.request.post(`/api/projects/${project.id}/transport`, { headers, data: { action: 'deactivate' } })
    await page.request.post(`/api/projects/${project.id}/engine`, { headers, data: { enabled: false } })
  }
})
