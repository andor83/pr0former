import { defineConfig } from '@playwright/test'
import { mkdtempSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
const data = process.env.PR0_TEST_DATA || mkdtempSync(join(tmpdir(), 'pr0former-e2e-'))
process.env.PR0_TEST_DATA = data
const load = process.env.PR0_LOAD === '1'
export default defineConfig({
  testMatch: load ? '**/load.spec.ts' : '**/*.spec.ts',
  testIgnore: load ? [] : ['**/load.spec.ts'],
  testDir: './e2e', workers: 1, reporter: 'list',
  use: { launchOptions: { args: ['--mute-audio'] }, baseURL: 'http://127.0.0.1:3101', viewport: { width: 1440, height: 960 } },
  webServer: { command: load ? 'cargo run --release -p pr0-server' : 'cargo run -p pr0-server', cwd: '..', env: { PR0_BIND: '127.0.0.1:3101', PR0_DATA: data }, url: 'http://127.0.0.1:3101/api/status', reuseExistingServer: false, timeout: 300000 },
})
