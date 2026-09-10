import { test, expect } from '@playwright/test'
import { spawn } from 'node:child_process'
import { mkdtempSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { resolve, join } from 'node:path'
import { createInterface } from 'node:readline'

test('desktop owner session opens the ready-made interface and keeps browser authentication separate', async ({ browser }) => {
  const root = resolve('..'), data = mkdtempSync(join(tmpdir(), 'pr0-desktop-browser-'))
  const child = spawn(join(root, 'target/release/pr0-server'), ['--desktop'], {
    cwd: data, env: { ...process.env, PR0_DATA: join(data, 'data'), PR0_RECORDINGS_ROOT: join(data, 'recordings'), PR0_WEB_ROOT: join(root, 'web/dist'), PR0_DISABLE_NATIVE_DEVICES: '1' }, stdio: ['pipe', 'pipe', 'pipe'],
  })
  const exited = new Promise<number | null>(resolve => child.once('exit', resolve))
  let stderr = ''
  child.stderr.on('data', chunk => { stderr += String(chunk) })
  const lines = createInterface({ input: child.stdout })
  const context = await browser.newContext()
  try {
    const ready = await new Promise<{ url: string; session: string }>((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error(`Desktop startup timed out: ${stderr}`)), 15000)
      child.once('error', error => { clearTimeout(timer); reject(error) })
      child.once('exit', () => { clearTimeout(timer); reject(new Error(stderr)) })
      lines.on('line', line => {
        if (line.startsWith('PR0_DESKTOP_READY ')) { clearTimeout(timer); resolve(JSON.parse(line.slice(18))) }
      })
    })
    const unauthenticated = await context.request.get(`${ready.url}/api/me`)
    expect(unauthenticated.status()).toBe(401)
    await context.addCookies([{ name: 'pr0_session', value: ready.session, url: ready.url, httpOnly: true, sameSite: 'Strict' }])
    const page = await context.newPage()
    await page.goto(ready.url)
    await expect(page.getByTitle('Sign out admin', { exact: true })).toBeVisible()
    await expect(page.getByRole('heading', { name: 'New performance', exact: true })).toBeVisible()
    await page.locator('.dialog-card input[maxlength="120"]').fill('Desktop workspace')
    await page.getByRole('button', { name: 'Create performance', exact: true }).click()
    await expect(page.getByRole('button', { name: 'Enable audio engine', exact: true })).toBeVisible()
    await page.reload()
    await expect(page.getByTitle('Sign out admin', { exact: true })).toBeVisible()
    expect(await page.evaluate(() => document.cookie)).not.toContain('pr0_session')
    await page.screenshot({ path: 'test-results/desktop.png' })
  } finally {
    await context.close().catch(() => {})
    child.stdin.end()
    const timer = setTimeout(() => child.kill('SIGKILL'), 15000)
    const code = await exited
    clearTimeout(timer); lines.close()
    expect(code, stderr).toBe(0)
  }
})
