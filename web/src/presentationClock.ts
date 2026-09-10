// Presentation only: never feeds transport or DSP. Late telemetry may hold the
// display, but must not rewind a running timeline or invent progress while stale.
type Snapshot = { project_id: string; epoch: string; beat: number; bpm: number; server_time: number; running: boolean }
export class PresentationClock {
  private identity = ''
  private authoritative = 0
  private displayed = 0
  read(snapshot: Snapshot | null, active: boolean, stale: boolean, serverNow: number): number {
    if (!active || !snapshot) {
      this.identity = ''; this.authoritative = 0; this.displayed = 0
      return 0
    }
    const key = `${snapshot.project_id}/${snapshot.epoch}`
    const reset = key !== this.identity || snapshot.beat < this.authoritative
    this.identity = key
    this.authoritative = snapshot.beat
    if (reset || !snapshot.running) this.displayed = snapshot.beat
    if (!snapshot.running) return this.displayed
    if (stale) return this.displayed
    const elapsed = Math.max(0, Math.min(500, serverNow - snapshot.server_time))
    this.displayed = Math.max(this.displayed, snapshot.beat + elapsed / 60000 * snapshot.bpm)
    return this.displayed
  }
}
