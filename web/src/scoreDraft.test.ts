import { describe, it, expect } from 'vitest'
import { createScoreDraft } from './scoreDraft'
import type { Project } from './types'
const project = (name: string, revision = 1) => ({ id: 'p', name, revision } as Project)
function deferred() { let resolve!: () => void; let reject!: (e: Error) => void; const promise = new Promise<void>((a,b) => { resolve=a; reject=b }); return { promise, resolve, reject } }
describe('score save barrier', () => {
  it('waits for the running request and drains edits typed during it', async () => {
    let current = project('old')
    const gate = deferred(), saved: string[] = []
    const session = createScoreDraft(() => current, async p => { if (!saved.length) await gate.promise; saved.push(p.name); current = { ...p, revision: p.revision + 1 } })
    session.draft.value = project('first')
    const first = session.flush()
    session.draft.value = project('newest')
    let done = false
    const barrier = session.flush().then(() => { done = true })
    await Promise.resolve()
    expect(done).toBe(false)
    gate.resolve()
    await Promise.all([first, barrier])
    expect(saved).toEqual(['first', 'newest'])
    expect(current.revision).toBe(3)
    expect(session.pending.value).toBe(false)
  })
  it('retains the newest draft on failure and blocks export until resolved', async () => {
    const gate = deferred()
    const session = createScoreDraft(() => project('old'), () => gate.promise)
    session.draft.value = project('submitted')
    const request = session.flush()
    session.draft.value = project('newest')
    gate.reject(new Error('Validation failed'))
    await expect(request).rejects.toThrow('Validation failed')
    expect(session.conflict.value?.name).toBe('newest')
    await expect(session.flush()).rejects.toThrow('Validation failed')
  })
})
