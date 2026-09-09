# Score editor

Open **Score & parts** while the show is inactive. The sidebar lists parts, player assignments, staff counts and instruments. Check the parts to display together; collapse the sidebar to widen the score. Part order buttons persist the ensemble order. Part settings configure the focused part and its staves. The score scrolls continuously, without pages. Zoom and Follow playback are local viewing settings.

## Entering and editing

Choose **Write**, a note value and optional dots/accidentals, then click the staff. Auto accidental uses the current key signature; explicit sharp/flat/natural overrides it. Choose a voice or tuplet ratio before entry. Choose **Select** to click or drag a selection. Ctrl/Cmd-click toggles individual notes; Ctrl/Cmd-drag adds to a selection. Drag selected notes to change their onset and staff position. Backspace/Delete removes the selection.

| Shortcut | Action |
| --- | --- |
| 1, 2, 3, 4 | 32nd, 16th, eighth, quarter |
| 5, 6, 7, 8 | Half, whole, double whole, 64th |
| Period | Cycle zero, one and two dots |
| Plus / minus | Raise/lower the accidental, through double sharps/flats |
| Up / down | Move by a diatonic staff step |
| Shift-up / down | Move an octave |
| Left / right | Select the adjacent note by onset |
| R | Toggle a rest |
| Ctrl/Cmd-C, V | Copy selection; paste immediately after its original span |
| Ctrl/Cmd-Z, Shift-Z | Undo / redo score edits |
| Escape | Clear selection |

Value/dot/accidental commands edit selected notes; without a selection they set entry values. Text fields retain ordinary typing. The inspector edits onset, velocity, voice, articulation and octave line. Select two notes and choose Tie, Slur or Grace; select one to remove that relation. Ties require adjacent equal pitches. Grace moves the first selected note to the principal onset and steals a bounded portion of its playback time. Chords share an onset; overlapping rhythms at different onsets require different voices. Automatic rests are display-only.

Saves use server revision checks. Keyboard edits queue behind a pending save. Conflicts retain a draft with Reapply and Discard actions. Reapply replaces score fields with the retained draft against the current revision; review collaborators' changes first. Score undo is local to the open editor and clears after a remote score change.

## Shared timeline

Open **Shared score · meter, keys, repeats and navigation**. Existing projects retain their old loops until **Convert to shared score** is selected. Positions are zero-based quarter beats: a 4/4 bar lasts four, and a 6/8 bar lasts three. Meter changes never stretch notes.

Structured scores play all parts through the shared traversal and stop at its end. Enable Loop whole score to repeat that traversal. Independent conducted/freeform parts still need launch cues and loop within their part length. Repeats use ordered, nonoverlapping ranges and 2–32 passes. An optional first-ending start skips that ending on the final pass. One D.C./D.S. jump can use Fine or a coda pair; written repeats run before the jump. Nested repeats and arbitrary ending pass lists are not supported.

Each part supports up to eight staves with four voices each. Configure staff name, clef/clef changes, static key override, major/minor label, sounding transposition, instrument and MIDI channel. An inherited key follows the shared score. Transposition changes sounding pitch while retaining written spelling. Staff MIDI ports can inherit the part route or select a separate named port.

## Dynamics and MIDI

Under each part, **Dynamics** adds ppp–fff marks and hairpins. Output can affect note attack velocity, MIDI CC, or both; the default controller is expression CC11. Velocity scales the note's stored velocity relative to 90. Continuous dynamics need an instrument that responds to the chosen CC.

**MIDI lanes** adds visual point events, note blocks or ramps. Select the channel, message and controller/note number. Bend uses 0–16383; other values use 0–127. Curves include step, linear, ease-in, ease-out and S-curve. Click to draw, drag endpoints, or edit numeric event values. Program changes are points; note blocks carry attack velocity and duration. One raw lane owns each channel/message/controller target. A raw event overrides generated dynamics for the same CC during that event's interval.

Connect a **Part MIDI** node's **events** outlet to **MIDI output → events** for external delivery. Use **MIDI to control** for filtered graph control values. Its value is the last matching event per engine sample; existing Part MIDI pitch/velocity/gate/trigger/note-off outlets retain their original note-control behavior. A Part MIDI staff filter can isolate one staff; part-level automation reaches all matching part sources. MIDI graph boundaries allow these connections through subgraphs.

The server evaluates curves from engine time, sends changed values at up to 100 Hz plus event boundaries, and restores the appropriate value after repeat jumps. External delivery uses a bounded worker. These tests establish software behavior, not physical MIDI timing or audio latency.

## Performance and interchange

Performance mode shows every part assigned to the signed-in player. **Show all parts** defaults off on each entry; turning it on places assigned parts first, then the others in saved order. With no assignments, a message offers Show all. Visibility never changes scheduling or launch permissions.

MusicXML imports/exports multiple parts and staves, written pitches, voices/chords, supported rhythms and expressive marks, signatures, repeats and navigation. Raw MIDI lanes, routing/player assignments and graph state require native project JSON. Export reports omitted MIDI settings and any fraction rounding to 1/20160 quarter beats. Import accepts uncompressed `.xml`/`.musicxml` score-partwise files up to 4 MB, rejects entity declarations, and reports unsupported notation. This is a documented subset, not a guarantee of lossless exchange with every notation application.

The editor does not include page engraving, percussion/tab staves, nested repeats, arbitrary ending lists or comprehensive cross-staff collision handling. Physical iPad gestures, hardware MIDI output and performance-load acceptance remain manual checks.
