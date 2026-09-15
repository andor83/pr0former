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
  await page.goto('/')
  const status = await (await page.request.get('/api/status')).json()
  await page.getByLabel('Username', { exact: true }).fill('link-quality')
  await page.getByLabel('Password', { exact: true }).fill('test1234')
  await page.getByRole('button', { name: status.bootstrap ? 'Create account' : 'Sign in', exact: true }).click()
  if (!status.bootstrap && await page.getByText(/invalid|unknown|incorrect/i).count()) {
    await page.getByRole('button', { name: 'New here? Create an account', exact: true }).click()
    await page.getByRole('button', { name: 'Create account', exact: true }).click()
  }
  const nameField = page.getByLabel('Project name', { exact: true })
  const indicator = page.locator('.server-indicator')
  await expect(indicator).toHaveText(/Server connected|Connecting/)
  if (await nameField.waitFor({ state: 'visible', timeout: 5000 }).then(() => true, () => false)) {
    await nameField.fill('Link quality')
    await page.getByRole('button', { name: 'Create performance', exact: true }).click()
  }
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
