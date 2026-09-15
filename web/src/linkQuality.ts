// Presentation only: rates the browser↔server link from the once-a-second
// ping/pong round trip so the header can warn that the link is too slow for
// live performance. Never feeds transport, scheduling or DSP.
export type LinkQuality = { rtt: number | null; lagging: boolean }
export class LinkMonitor {
  private samples: number[] = []
  private outstanding: number[] = []
  private lagging = false
  constructor(
    // Round trips at or above this many milliseconds are unusable for playing.
    readonly threshold = 250,
    // The badge clears only once every recent round trip is below this.
    readonly recovery = 200,
    // Waiting this long for any pong counts as lag even before it arrives.
    readonly stall = 750,
    readonly window = 5,
  ) {}
  reset() { this.samples = []; this.outstanding = []; this.lagging = false }
  sent(clientTime: number) { this.outstanding.push(clientTime) }
  received(clientTime: number, now: number) {
    this.samples = [...this.samples.slice(-(this.window - 1)), Math.max(0, now - clientTime)]
    this.outstanding = this.outstanding.filter(sentAt => sentAt > clientTime)
  }
  read(now: number): LinkQuality {
    if (!this.samples.length && !this.outstanding.length) return { rtt: null, lagging: false }
    const sorted = [...this.samples].sort((a, b) => a - b)
    const median = sorted.length ? sorted[Math.floor(sorted.length / 2)]! : 0
    const worst = sorted.length ? sorted[sorted.length - 1]! : 0
    const waiting = this.outstanding.length ? now - this.outstanding[0]! : 0
    if (waiting >= this.stall || median >= this.threshold) this.lagging = true
    else if (worst < this.recovery) this.lagging = false
    const rtt = this.lagging ? Math.max(worst, waiting) : median
    return { rtt: Math.round(rtt), lagging: this.lagging }
  }
}
