import { test, expect } from '@playwright/test'

test('the header warns when the server round trip is too slow to perform', async ({ page }) => {
  test.setTimeout(120000)
  page.setDefaultTimeout(15000)
  // Hold every server→browser frame by `delay` ms so the pong round trip grows.
  let delay = 0
  await page.routeWebSocket(/\/api\/projects\/.*\/events/, ws => {
    const server = ws.connectToServer()
    ws.onMessage(message => server.send(message))
    server.onMessage(message => { if (delay) setTimeout(() => ws.send(message), delay); else ws.send(message) })
  })
  const headers = { 'X-Pr0former': '1' }
  const status = await (await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap ? 'register' : 'login'}`, { headers, data: { username: 'browser-test', password: 'test1234' } })
  const created = await page.request.post('/api/projects', { headers, data: { name: 'Link quality', mode: 'freeform' } })
  expect(created.ok(), await created.text()).toBeTruthy()
  await page.goto('/')
  const indicator = page.locator('.server-indicator')
  await expect(indicator).toHaveText('Server connected')
  await expect(indicator).not.toHaveClass(/lagging/)
  delay = 400
  await expect(indicator).toHaveClass(/lagging/, { timeout: 20000 })
  await expect(indicator).toHaveText(/^Slow connection\d+ ms$/)
  await expect(indicator).toHaveAttribute('title', /too slow to perform live/)
  delay = 0
  await expect(indicator).toHaveText('Server connected', { timeout: 20000 })
  await expect(indicator).not.toHaveClass(/lagging/)
})
