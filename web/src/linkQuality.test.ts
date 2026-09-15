import { describe, expect, it } from 'vitest'
import { LinkMonitor } from './linkQuality'
function exchange(link: LinkMonitor, sentAt: number, rtt: number) { link.sent(sentAt); link.received(sentAt, sentAt + rtt) }
describe('link monitor', () => {
  it('reports nothing before the first round trip and stays clear on a fast link', () => {
    const link = new LinkMonitor()
    expect(link.read(0)).toEqual({ rtt: null, lagging: false })
    for (let i = 0; i < 5; i++) exchange(link, i * 1000, 20 + i)
    expect(link.read(5000)).toEqual({ rtt: 22, lagging: false })
  })
  it('ignores a single spike but flags sustained lag at the threshold', () => {
    const link = new LinkMonitor()
    for (let i = 0; i < 4; i++) exchange(link, i * 1000, 30)
    exchange(link, 4000, 900)
    expect(link.read(5000).lagging).toBe(false)
    exchange(link, 5000, 260); exchange(link, 6000, 250)
    expect(link.read(7000)).toEqual({ rtt: 900, lagging: true })
  })
  it('clears only once the link is comfortably below the threshold', () => {
    const link = new LinkMonitor()
    for (let i = 0; i < 5; i++) exchange(link, i * 1000, 400)
    expect(link.read(5000).lagging).toBe(true)
    for (let i = 5; i < 10; i++) exchange(link, i * 1000, 230)
    expect(link.read(10000)).toEqual({ rtt: 230, lagging: true })
    for (let i = 10; i < 15; i++) exchange(link, i * 1000, 150)
    expect(link.read(15000)).toEqual({ rtt: 150, lagging: false })
  })
  it('flags a stalled link while a ping is still unanswered', () => {
    const link = new LinkMonitor()
    for (let i = 0; i < 5; i++) exchange(link, i * 1000, 30)
    link.sent(5000); link.sent(6000)
    expect(link.read(5500).lagging).toBe(false)
    expect(link.read(5800)).toEqual({ rtt: 800, lagging: true })
    link.received(5000, 6100)
    expect(link.read(6150).lagging).toBe(true)
    link.received(6000, 6150)
    for (let i = 7; i < 12; i++) exchange(link, i * 1000, 30)
    expect(link.read(12000).lagging).toBe(false)
  })
  it('forgets everything on reset', () => {
    const link = new LinkMonitor()
    for (let i = 0; i < 5; i++) exchange(link, i * 1000, 500)
    link.sent(5000)
    link.reset()
    expect(link.read(9000)).toEqual({ rtt: null, lagging: false })
  })
})
