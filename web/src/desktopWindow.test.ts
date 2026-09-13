import { afterEach, expect, it, vi } from 'vitest'
import { desktopProject, desktopView, updateDesktopWindow } from './desktopWindow'

afterEach(() => vi.unstubAllGlobals())
it('keeps desktop layout addressing out of ordinary browser sessions', () => {
  vi.stubGlobal('window', {})
  const replaceState = vi.fn()
  vi.stubGlobal('history', { replaceState })
  expect(desktopProject()).toBeNull()
  updateDesktopWindow('id', 'Name', 'score')
  expect(replaceState).not.toHaveBeenCalled()
})
it('reports the authorized project and bounded workspace view through the URL and title', () => {
  vi.stubGlobal('window', { __PR0_DESKTOP__: true })
  vi.stubGlobal('location', { href: 'http://studio.local/?project=one', search: '?project=one' })
  const replaceState = vi.fn()
  const document = { title: '' }
  vi.stubGlobal('history', { replaceState })
  vi.stubGlobal('document', document)
  expect(desktopProject()).toBe('one')
  updateDesktopWindow('two', 'Second performance', 'score')
  const url = replaceState.mock.calls[0][2] as URL
  expect(url.searchParams.get('project')).toBe('two')
  expect(url.searchParams.get('view')).toBe('score')
  expect(document.title).toContain('Second performance — score')
  expect(desktopView('unknown')).toBe('graph')
})
