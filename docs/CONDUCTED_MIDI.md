# Conducted cues and MIDI bindings

The Conductor editor and performance view both provide part tiles, ARM,
PLAY ARMED, PLAY + REPEAT, group/part dynamics, and Stop all next pulse.
Start the project with transport Play to rehearse these controls in the editor.
Editors can rehearse during preparation; a locked performance retains conductor
cue authority. A tile's elapsed green fill and leading line move left to right.
Names, performer labels, arming and dynamics occupy separate rows.

## Learn a control

1. Designate the conductor in project settings. That user chooses **Connect local
   MIDI** in the Conductor workspace. Without a designation, the project owner
   supplies local MIDI. Browser permission, HTTPS/localhost and Web MIDI support
   are required. Server MIDI inputs need no browser MIDI permission.
2. The conductor or an editor chooses **Bind MIDI**. Optionally restrict Source
   to local or server MIDI and choose a device; the default accepts the first
   device from either source.
3. Select a highlighted control on the screen, then press or move the intended
   MIDI control. The first supported message saves its source, device, MIDI
   channel and note/controller number. MIDI clock/system messages and note
   releases are ignored. Learning and subsequent gestures while bind mode is
   open do not trigger existing MIDI cue bindings.
4. **Escape**, **End bind mode**, or clicking outside the binding controls ends
   bind mode and cancels pending capture. **Cancel pending binding** leaves bind
   mode open. A pending capture also times out after two minutes. Existing saved
   assignments are not erased by cancelling.

Only one editor can bind at a time. A new assignment replaces an existing
assignment for that on-screen target or physical MIDI control.

## Targets

| On-screen control | MIDI behavior |
| --- | --- |
| Part tile | Press to start; press again to stop, or cancel a queued start. |
| ARM | Toggle arming. Arming a part clears another armed part for the same performer. |
| Individual set | Display that set. Playing parts continue. |
| Next set | Advance through the ordered sets and wrap to the first. |
| Bind set slider / knob | Divide the continuous controller's full range across the ordered sets. |
| PLAY ARMED / PLAY + REPEAT | Launch armed parts once / repeatedly using the configured count-in. |
| Stop all next pulse | Stop all project parts at the next pulse. |
| Group dynamic | Apply the continuous controller's 0–127 dynamic value to the armed/playing group. |

Note-on buttons fire for positive velocity. CC/pressure/bend buttons fire when
crossing into the upper half of their range; return to the lower half before the
next press. Held values and note releases do not repeatedly toggle. Program
change bindings act on each received matching program change. Set selection
and dynamics require CC, pressure or pitch bend; bend uses its coarse 0–127
position. The channel is always part of the assignment.

## Shared devices and persistence

Open **Bindings** to select another conductor-local or server device, or remove
an assignment. The conductor and editors share the conductor's announced device
list. A local binding automatically belongs to the designated conductor; editors
cannot substitute their own browser's input stream or assign a different user.
After a conductor change, choose that conductor's devices for existing bindings.
Only one browser window supplies local MIDI per project. Disconnect local MIDI
before moving to another window. Lost connections release the source lease;
previously enabled inputs reconnect on reload when the browser permits it.

Bindings are validated and saved by the server as project configuration. They
can be assigned during a locked performance without unlocking score/graph edits.
The selected set, device presence, held-button state, learn state and playback
positions are transient and do not enter revision or undo history.

Server-connected bindings run on the server while the project is active, even
without an open conductor screen. Local bindings need the conductor's connected
browser. Both submit cues to the existing engine pulse scheduler. Native input
callbacks use bounded MIDI rings; device discovery and routing run outside audio
callbacks. External/controller arrival latency is best effort. Automated tests
use synthetic browser MIDI and software audio timing; physical controllers,
server MIDI drivers, reconnection behavior on real devices and performance
latency must be rehearsed manually.
