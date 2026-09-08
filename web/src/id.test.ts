import { afterEach, expect, test, vi } from 'vitest'
import { newId } from './id'

afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals() })

test('uses the browser UUID API when available', () => {
  const uuid = '12345678-1234-4234-8234-123456789abc'
  vi.spyOn(globalThis.crypto, 'randomUUID').mockReturnValue(uuid)
  expect(newId()).toBe(uuid)
})

test('creates distinct version 4 UUIDs without the secure-context UUID API', () => {
  vi.stubGlobal('crypto', { getRandomValues: globalThis.crypto.getRandomValues.bind(globalThis.crypto) })
  const ids = Array.from({ length: 100 }, newId)
  expect(new Set(ids).size).toBe(100)
  for (const id of ids) expect(id).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/)
})
