# Conducted performance handoff

Last updated: 2026-09-11. The working branch is `codex/conducted-performance`.
The previous, unrelated working-tree changes were committed separately as
`81dadc0 Expand project management and universal MIDI routing` and are already
on `origin/main`. The conducted-performance work described below is intentionally
uncommitted so the next developer can review and continue it.

## Product contract agreed with the user

- An ensemble has one designated conductor. That member cannot be assigned a
  performer part.
- Conducted projects have an ordered collection of sets, and each set has an
  ordered collection of parts. Editing preserves that order and supports
  touch/pointer reordering.
- In the locked conductor performance screen, tapping an idle part starts a cue;
  tapping a playing part stops it. Both changes occur on the next independent
  global pulse.
- Arming is separate from tapping: an armed group starts together from the large
  Play button. Play + Repeat forces every launched part to repeat even if the
  notation itself does not repeat. Ordinary cues are one-shot.
- A cue from rest starts its count-in on the next pulse, then starts music after
  the configured count-in. A busy performer does not count in again: the next
  part is queued so only one part per performer sounds at once.
- Every performer should ultimately see one continuously scrolling staff. Rests
  occupy idle time and future parts should animate into that lane before their
  start, rather than replacing the display abruptly.
- Each part may have its own meter. The independent global pulse is the only
  shared rhythmic unit.
- A vertical performer dynamics meter follows written dynamics until overridden
  by the conductor. Live override glows and Return to score relinquishes control.
- Browser MIDI should ultimately support both conductor mappings and performer
  MIDI streaming to the server.

## Implemented in the current working tree

### Project model and validation

`crates/core/src/lib.rs` adds `Project.conductor`, `Project.conducted`, ordered
`ConductedSet` values, count-in/global-pulse settings, and per-part
`performance_meters`. Validation bounds the layout, validates IDs and meters,
rejects duplicate/missing set part references, and rejects malformed settings.

`crates/server/src/main.rs` verifies that the designated conductor is a project
member with no assigned parts, clears the designation when that member is
removed, and gives cue/transport authority to the designation (with owner
override for recovery). It adds `POST /api/projects/{id}/cue` for arm, unarm,
start, stop, repeat, and dynamic requests. Cue request IDs are bounded and part
targets are checked server-side.

### Scheduling and telemetry

`crates/server/src/performance.rs` makes conducted/freeform launches one-shot by
default and implements global-pulse quantization, count-in state, forced repeat,
arming, bounded idempotent request IDs, per-performer successor queues, and live
dynamic overrides. Only one lane for a performer can sound at a time; cueing
another lane makes an active forced-repeat lane exit at its cycle boundary.
Queued entries now retain an explicit engine-beat `scheduled_start`, including
when the active part is stopped early. Every prepared part-local meter segment
maps its denominator beat to one global pulse; the inverse map drives notation
position telemetry across mid-part meter changes.

Telemetry now reports `armed`, `repeating`, `count_in_remaining`,
`dynamic_override`, `queue_position`, and `scheduled_start` per part. Dynamic override sends CC11
through staff/external MIDI routing and overrides later attack velocities;
Return to score clears it and sends the neutral CC value. These mutations happen
on the orchestration/command path, not inside `Engine::render` or a device
callback.

`crates/server/src/audio.rs` carries the new commands and emits count-in clicks
from engine time. Monitor output nodes can be associated with any part for a
performer. Conducted count-in clicks are mixed only into those dedicated feeds
when that performer is counting; shared transport/metronome clicks retain their
existing all-monitor behavior. The node modal exposes the association.

### Frontend

`web/src/components/ConductorWorkspace.vue` is the new Conductor tab/editor and
locked touch-first performance surface. It has ordered sets, part membership and
pointer reorder, active/queued set indicators, large tiles, progress sweeps,
explicit arm controls, group Play/Play + Repeat/Stop, per-part and group dynamics,
and Web MIDI Learn for play/repeat/stop/dynamic. MIDI mappings live in project-
scoped browser local storage and emit semantic cue requests; MIDI CC dynamics are
coalesced to animation frames. Both conductor and performer clients announce
their explicitly permissioned browser MIDI devices to the server.

Performer stage Web MIDI now streams three-byte MIDI channel messages over the
authenticated project WebSocket to the currently active assigned part. The
server rejects structured/inactive/unassigned targets, rejects system/SysEx and
out-of-range data, caps each connection at 2,048 messages/second, and hands valid
messages to the orchestration worker's prepared part-MIDI path. Note messages can
also drive the part's direct instrument and external MIDI route. Held browser
notes receive note-offs/all-notes-off on explicit stage shutdown, WebSocket
disconnect, or show reset.

`web/src/components/EnsembleWorkspace.vue` lets an owner designate an eligible
member. `ScorePartDialog.vue` excludes that conductor from performer assignment
and exposes an ordered editor for the initial and mid-part local conducted meter
changes. `App.vue` routes the exact
designated conductor into the locked conductor stage and performers into
`StageView.vue`. The stage has the translucent animated count-in and a dynamic
meter that follows the first staff's written automation unless overridden.

`web/src/components/ScoreWorkspace.vue` engraves a conducted part with its local
meter and keeps the existing scrolling/rest-generating score surface. Deleting a
part also removes its set references.

`web/src/conductedLane.ts` now composes all observed and scheduled parts for one
performer into a single synthetic score timeline. It retains played segments,
maps the engine clock back through each part's local meter map, inserts generated
rest space between cues, expands live forced-repeat cycles, remaps linked note
IDs/staves, and exposes queued notation before its scheduled start. `StageView`
renders only that lane, shows a sliding NEXT badge, and fades newly arrived
notation (disabled under reduced motion).

## Remaining validation intentionally excluded from this implementation pass

No known software item from the conducted-performance implementation list remains
open. The full Playwright run and physical testing were explicitly excluded from
the completion request. `web/e2e/conductor.spec.ts` now contains the unrun browser
coverage for designation/cue authorization, request idempotency, set activity,
same-performer queueing, stopping, forced repeat, live dynamics/Return to score,
and the new touch-first controls. Physical MIDI, iPad/Safari, multi-client timing,
soak, and performance-scale acceptance remain manual.

## Validation state

- 2026-09-11 continuation: focused server tests for explicit successor timing
  (including retained boundaries) and mid-part conducted meter conversion pass.
- The production frontend build passes with the new synthetic performer lane,
  and 62 frontend unit tests pass, including local/global polymeter conversion,
  queued/repeated notation concatenation, and preserved idle rest space.
- Focused core/server tests pass for Monitor-output part association and
  same-performer cue-count-in targeting; the workspace compiles after the
  per-feed click split.
- Focused server tests pass for MIDI byte validation, part-scoped routing, held
  note state, and panic. The frontend production build passes with performer
  Web MIDI permission/device announcement/streaming controls.
- Set and part reorders now use measured before/after positions and Web Animations
  FLIP transforms for iPad-like sibling reshuffling; reduced-motion skips them.
  The frontend build also covers the full per-part meter-change editor.

- `cargo test --workspace --quiet`: passed with localhost UDP permission — core
  16, DSP 93, auxiliary 1 + 1, server 79 passed and 1 ignored (190 passed total).
- `npm --prefix web test`: 62 passed.
- `npm --prefix web run build`: passed; only the existing Vite large-chunk
  warning remains.
- The original `web/e2e/conductor.spec.ts` cue case passed focused and during the
  partial full run. Its subsequently expanded authorization, queue, repeat,
  dynamics, set-activity, and reduced-motion cases are authored but intentionally
  unrun in this pass.
- A full Playwright run was stopped at the user's request after 27 tests passed.
  Before stopping, two existing control-routing cases failed due missing/stale
  expected graph state and the existing granular case failed because its test
  still calls `selectOption` on the now-searchable sample `<input>`. One presence
  case was interrupted. These failures are outside the conducted files but need
  a clean isolated rerun before claiming a green full suite.
- `git diff --check` passes. Running workspace-wide `cargo fmt --check` reports
  formatting differences already present in the `81dadc0` base; the modified
  conducted Rust files were formatted, and unrelated rustfmt churn was removed.
- No physical MIDI device, iPad/Safari touch interface, multi-client latency,
  hardware audio, soak, or 32-player test has been performed.

## Safe next sequence

Start by reading `docs/ARCHITECTURE.md`, `docs/STATUS.md`, and this file. Inspect
the uncommitted diff rather than regenerating it. The next engineering action is
the explicitly deferred full Playwright run in clean test state, followed by any
repairs it reveals. Physical acceptance remains separate. After those checks,
update `docs/STATUS.md`, commit the feature on `codex/conducted-performance`, and
only push after user confirmation.
