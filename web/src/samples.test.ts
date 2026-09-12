import { describe, expect, it } from 'vitest'
import { formatTags, hasTag, matchesSample, parseTags, tagCloud, toggleTag, type SampleEntry } from './samples'

const sample = (name: string, tags: string): SampleEntry => ({ id: name, asset: null, name, description: '', tags, category: '', musical_key: '', bpm: null, global: false, channels: 2, sample_rate: 48000, frames: 1, duration: 0, revision: 0, author: 'a', can_edit: true, can_publish: true })

describe('sample tags', () => {
  it('parses, trims, de-duplicates case-insensitively and formats', () => {
    expect(parseTags(' Kick, drums;kick ,  kit\nDrums , ,')).toEqual(['Kick', 'drums', 'kit'])
    expect(formatTags(['a', ' b ', 'A'])).toBe('a, b')
    expect(parseTags(undefined)).toEqual([])
    expect(parseTags('x'.repeat(40))).toEqual(['x'.repeat(32)])
    expect(parseTags(Array.from({ length: 40 }, (_, i) => `t${i}`).join(',')).length).toBe(32)
  })
  it('matches samples by search text and by every active tag', () => {
    const s = sample('Kick', 'drums, kit, cc0')
    expect(matchesSample(s, 'kick')).toBe(true)
    expect(matchesSample(s, '', ['Drums'])).toBe(true)
    expect(matchesSample(s, '', ['drums', 'loop'])).toBe(false)
    expect(hasTag(s, 'CC0')).toBe(true)
    expect(hasTag(s, 'c')).toBe(false)
  })
  it('builds a cloud ordered by use and toggles filters', () => {
    const cloud = tagCloud([sample('a', 'drums, kick'), sample('b', 'Drums, snare'), sample('c', 'drums')])
    expect(cloud).toEqual([{ tag: 'drums', count: 3 }, { tag: 'kick', count: 1 }, { tag: 'snare', count: 1 }])
    expect(toggleTag(['drums'], 'Kick')).toEqual(['drums', 'Kick'])
    expect(toggleTag(['drums', 'Kick'], 'kick')).toEqual(['drums'])
  })
})
