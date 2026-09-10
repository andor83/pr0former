# Score editor

Open **Score & parts** in preparation mode, with the audio engine on or off. The sidebar lists parts, player assignments, staff counts and instruments. Check the parts to display together; collapse the sidebar to widen the score. Part order buttons persist the ensemble order. The centered floating toolbar contains MusicXML import/export, **Part** settings, **Score** settings and **Bars & meter**. Follow, zoom and playback status sit in the footer. The score scrolls continuously, without pages. Zoom and Follow playback are local viewing settings.

The compact part rows include **M** (mute MIDI) and **S** (exclusive solo). Solo unmutes that part and silences the others. Switching solo off restores the other parts’ saved mute states. These settings affect scheduled notes, dynamics and raw MIDI; merely hiding a part does not mute it.

The separate Activate/Deactivate show button is removed. Enable the engine to hear the graph; Play starts the score (and enables the engine if needed). Preparation stays editable during playback. A conductor entering performance mode locks score saves on the server until returning to preparation. Player views remain read-only. Engine disablement stops score playback. Entered notes audition for about a third of a second through the assigned graph instrument and Part MIDI outputs; note-offs count rendered samples even while transport is stopped. Preview does not directly address legacy part MIDI/OSC device routes.

## Entering and editing

The white floating icon palette stays centered inside the viewer; hover for tool names and shortcuts. Voice, tuplet and phrasing controls are in **Entry settings**. The note, rest, accidental/dot, accent, clef, meter and bar menus open icon grids with large glyphs. Choose **Write**, a note value and optional dots/accidentals, then click the staff. Auto accidental uses the current key signature; explicit sharp/flat/natural overrides it. Choose a voice or tuplet ratio before entry. Choose **Select** to click or drag a selection. Ctrl/Cmd-click toggles individual notes; Ctrl/Cmd-drag adds to a selection. Drag selected notes to change their onset and staff position. Backspace/Delete removes the selection.

### Speedy Entry caret

In Write mode a click places the **caret** (a blinking cyan line with a notehead marker and the voice number) after the note it enters. The caret is the insertion point for the keyboard: a duration key inserts a note at the caret pitch and advances the caret by the written duration, so a phrase is typed as a sequence of durations and pitches, as in Finale’s Speedy Entry. Up/Down move the caret pitch by a staff step (Shift for an octave); Left/Right move the caret to the previous or next note boundary in the same staff and voice, or by one bar with Shift. Letters A–G insert a note of the current duration at the nearest matching pitch; Shift with a letter or digit stacks a chord tone on the last entry without advancing. `0` inserts a rest of the current duration; Backspace at the caret removes the entry that ends there and steps back; `T` marks the entry before the caret to be tied into the next entry of the same pitch (with one note selected, `T` ties it to the following same-pitch note; with two selected it behaves like the Tie button). Alt with 1–4 switches the entry voice. The footer status shows the caret bar, beat, voice and duration. Rapid typing queues behind the pending save and is never dropped; a validation failure reports the server error. Escape clears the selection first and then the caret.

### Tempo marks and staff text

The **Text and tempo** palette menu places a tempo mark (♩ = n) or a text mark by clicking a staff at the beat; the Measure dialog’s **Tempo** tab and the right-click menu do the same for the selected bars. Tempo marks live in the shared timeline and drive playback: the engine applies each mark as the written position reaches it, a manual tempo edit lasts until the next mark, and Play or Stop re-applies the mark in force. A mark at beat 0 also sets the project tempo and count-in. Text marks are attached to one staff and are shown on every player’s view: **Cue** (bold, with an arrow) for performance instructions such as “start granular”, **Rehearsal letter** (boxed), **Text**, **Expression** (italic), **Tempo text** such as “rit.” (display only; use a tempo mark for playback) and **Lyric** (below the staff, up to 256 characters). Click a mark to select it, drag to move it, Backspace to delete, or double-click to edit its kind, text and position. Bar insertion and deletion move marks and tempo changes with the music.

### MIDI entry

The footer **MIDI** selector uses Web MIDI (Chrome, Edge and Chromium-based browsers, secure context) with any connected input. **Play to enter** inserts a note at the caret for each key played, using the current duration; keys held together become a chord and the caret advances once. **Hold + number** stages held pitches and enters them as a chord when a duration key is pressed, as in Speedy Entry; without held pitches the number key uses the caret pitch. Sounding pitches are spelled for the staff’s key and transposition. The choice persists per browser. Safari and Firefox do not currently expose Web MIDI; the selector reports the failure and returns to Off.

Note onsets default to a 32nd-note grid independently of the selected duration, so quarters can start offbeat. The footer’s Snap control also offers 16ths, 64ths and triplet grids. Ghost rests continue filling empty time. An untied note extending past its starting bar is outlined in red; it remains editable and its exact duration is preserved. Hovered items turn orange; selected items stay green.

Clef and meter menu choices activate a placement tool: click the staff at the desired beat. The bar menu can repeat a clicked bar, add double/final/dashed barlines, or open the bar editor. **Insert after bar** and **Insert before bar** accept a bar count. Numeric editing of repeat ranges and navigation remains in Shared score; the Finale-style route is the bar selection and **Measure** dialog described under Shared timeline.

| Shortcut | Action |
| --- | --- |
| 1, 2, 3, 4 | 64th, 32nd, 16th, eighth (Finale keypad order; with a caret and no selection, inserts) |
| 5, 6, 7, 8 | Quarter, half, whole, double whole |
| 0 | Insert a rest of the current duration at the caret |
| A–G | Insert the nearest pitch with that letter at the caret; Shift adds it to the last chord |
| Shift-1…8 | Add the caret pitch to the last entry as a chord tone |
| Period | Cycle zero, one and two dots |
| Plus / minus | Raise/lower the accidental, through double sharps/flats |
| Up / down | Move the selection, or the caret pitch, by a diatonic staff step |
| Shift-up / down | Move an octave |
| Left / right | Select the adjacent note; with a caret, move to the previous/next entry boundary (Shift: by bar) |
| T | Tie: caret entry into the next entry, one selected note to its successor, or two selected notes |
| Alt-1…4 | Entry voice (layer) |
| R | Toggle a rest |
| Backspace / Delete | Delete the selection, the entry before the caret, or the contents of selected bars |
| Ctrl/Cmd-C, V | Copy selection; paste immediately after its original span |
| Ctrl/Cmd-Z, Shift-Z | Undo / redo score edits |
| Shift-F10 | Open the measure menu for the selected bars |
| Escape | Clear selection, then bar selection, then caret |

Edits apply to a local draft immediately and are saved about a quarter of a second after the last change; the footer shows the saving state and a validation failure reverts the unsaved edits with the server’s message. Undo entries reference the previous state rather than copying the project. Value/dot/accidental commands edit selected notes; without a selection they set entry values. Text fields retain ordinary typing. Double-click a selection, press Enter, or use the settings icon to edit duration, onset, velocity, voice, articulation and octave line in a dialog. Select two notes and choose Tie, Slur or Grace; select one to remove that relation. Ties require adjacent equal pitches. Grace moves the first selected note to the principal onset and steals a bounded portion of its playback time. Chords share an onset; overlapping rhythms at different onsets require different voices. Automatic rests fill unoccupied rhythm. They can be selected and deleted; bounded, persisted hidden-rest ranges retain that layout after reload. Editing or moving an automatic rest turns it into an explicit rest.

Accidentals, dots, articulations, signatures, repeats and phrasing marks have separate selection targets. Backspace/Delete removes removable marks without deleting their notes; drag an attached mark onto another note to reattach it. Drag timeline marks to move their beat. Double-click complex marks to edit their owning note, staff or shared-score settings. Initial clefs and the final remaining staff are required structure; generated beams, split-note ties and barlines follow the note rhythm/meter and are edited through those settings.

In **Piano roll**, the palette omits note values. Choose Write and drag horizontally to draw duration on an evenly spaced sixteenth-note grid. Drag a note body to move it in time and semitones; drag its right edge to resize. Select supports Ctrl/Cmd-click and a drag rectangle for multiple-note deletion or movement. Overlapping onsets use an available staff voice (up to four); server validation rejects rhythms that exceed this limit. Double-click opens note details. MIDI lanes and their collapsible numeric footer remain available below the part.

Saves use server revision checks. Keyboard edits queue behind a pending save. Conflicts retain a draft with Reapply and Discard actions. Reapply replaces score fields with the retained draft against the current revision; review collaborators' changes first. Score undo is local to the open editor and clears after a remote score change.

## Shared timeline

### Bar selection and the Measure dialog

In Select mode, clicking empty space in a bar selects that bar on that staff (tinted cyan); Shift-click extends the selection to another bar and a drag rectangle selects a beat range. Double-click the selected bars, right-click any bar, press Shift-F10, or use the **Measure** toolbar button to open the measure actions. The right-click menu offers Time signature, Key signature, Clef change, an immediate two-pass repeat of the selected bars, repeat passes and endings, D.C./D.S./Coda, barline style, insert/add/delete bars, selecting the notes in the bars, and clearing their contents (also Backspace). The **Measure** dialog shows the bar range (From/Through bar, or To end of score) and tabs:

- **Time signature** uses +/− steppers for beats and beat unit with a large preview and common signatures. Like Finale’s measure region, the new meter starts at the first selected bar and the previous meter resumes after the last selected bar unless *To end of score* is checked. Notes keep their timing.
- **Key signature** shows the fifteen keys in fifths order with their accidental counts, Major/Minor, and the Finale options **Hold to original pitches** (default) or **Transpose up/down** for the notes in the region. The previous key resumes after the region unless *To end of score* is checked.
- **Repeat** creates 𝄆 𝄇 around the selected bars with a passes stepper and an optional first-ending start bar; an overlapping repeat is replaced, and an existing one can be removed.
- **D.C. / D.S. / Coda** writes the single supported text repeat: D.C. or D.S. (segno at the first selected bar) at the end of the last selected bar, with Fine after a chosen bar or a To Coda / Coda pair.
- **Barline** sets the barline after the last selected bar to normal, double, final or dashed.
- **Clef** changes the chosen staff’s clef at the first selected bar.
- **Tempo** sets a ♩ per-minute tempo mark at the first selected bar, with common tempi as quick picks, and can remove an existing mark.
- **Add / delete bars** inserts before, adds after, adds at the end, deletes the selected bars, or clears their contents.
- **Mass edit** transposes the selected bars diatonically or chromatically (or by octave), doubles/halves note durations from the region start, and moves notes to another voice or staff. Selected bars also respond to the arrow keys (transpose by step, Shift for an octave) and to Ctrl/Cmd-C, X and V; paste replaces the destination range on the same or another part (respelled for its transposition) at the caret in Write mode or at the selected bars in Select mode.

Bar numbers in the dialog follow meter edits made while it is open. Every apply is one undoable, revision-checked save.

**Bars & meter** opens the numeric structural editor. Choose a one-based bar number or an exact zero-based quarter-beat position. **Set time signature** changes every part from that beat; at beat zero it also sets the initial/count-in meter. **Set clef at beat** changes the chosen staff, including positions between bar lines. Double-clicking a clef, time signature or bar line opens this editor at that location.

**Add bars at end** appends blank bars, completing a partial final bar first. **Insert before bar** or **Insert after bar** shifts later content in all parts. **Delete from bar** removes the chosen bars and their contents. Each action is one undoable, revision-checked save. Bar actions use the selected bar’s meter and update notes, automation, signatures, repeats, navigation and part lengths together. Notes/events crossing the cut are split or clipped; automation fragments retain their curve type with recalculated endpoints. Removed references are cleaned up. At least one bar must remain; the shared score limit is 4096 quarter beats. Applying a bar or meter edit to a legacy project creates its shared timeline.

Open **Shared score · meter, keys, repeats and navigation**. Existing projects retain their old loops until **Convert to shared score** is selected. Positions are zero-based quarter beats: a 4/4 bar lasts four, and a 6/8 bar lasts three. Meter changes never stretch notes.

Structured scores play all parts through the shared traversal and stop at its end. Enable Loop whole score to repeat that traversal. Independent conducted/freeform parts still need launch cues and loop within their part length. Repeats use ordered, nonoverlapping ranges and 2–32 passes. An optional first-ending start skips that ending on the final pass. One D.C./D.S. jump can use Fine or a coda pair; written repeats run before the jump. Nested repeats and arbitrary ending pass lists are not supported.

Each part supports up to eight staves with four voices each. Configure staff name, clef/clef changes, static key override, major/minor label, sounding transposition, instrument and MIDI channel. An inherited key follows the shared score. Transposition changes sounding pitch while retaining written spelling. Staff MIDI ports can inherit the part route or select a separate named port.

## Dynamics and MIDI

Under each part, **Dynamics** opens a dialog to add or edit ppp–fff marks and hairpins. Output can affect note attack velocity, MIDI CC, or both; the default controller is expression CC11. Velocity scales the note's stored velocity relative to 90. Continuous dynamics need an instrument that responds to the chosen CC. Click a written mark to select it, drag to change its beat, Backspace/Delete to remove it, or double-click for details.

Expand **MIDI lanes** to add visual point events, note blocks or ramps. The small **MIDI input · lane and event settings** footer expands to expose channel, message, controller/note number and numeric event values. Bend uses 0–16383; other values use 0–127. Curves include step, linear, ease-in, ease-out and S-curve. Click to draw, drag endpoints, or edit numeric event values. Program changes are points; note blocks carry attack velocity and duration. One raw lane owns each channel/message/controller target. A raw event overrides generated dynamics for the same CC during that event's interval.

Connect a **Part MIDI** node's **events** outlet to **MIDI output → events** for external delivery. Use **MIDI to control** for filtered graph control values. Its value is the last matching event per engine sample; existing Part MIDI pitch/velocity/gate/trigger/note-off outlets retain their original note-control behavior. A Part MIDI staff filter can isolate one staff; part-level automation reaches all matching part sources. MIDI graph boundaries allow these connections through subgraphs.

The server evaluates curves from engine time, sends changed values at up to 100 Hz plus event boundaries, and restores the appropriate value after repeat jumps. External delivery uses a bounded worker. These tests establish software behavior, not physical MIDI timing or audio latency.

## Performance and interchange

Performance mode shows every part assigned to the signed-in player. **Show all parts** defaults off on each entry; turning it on places assigned parts first, then the others in saved order. With no assignments, a message offers Show all. Visibility never changes scheduling or launch permissions.

MusicXML imports/exports multiple parts and staves, written pitches, voices/chords, supported rhythms and expressive marks, signatures, repeats, navigation, metronome tempo marks (quarter-note per-minute) and staff text (words and rehearsal marks; bold words import as cues, italic as expressions, placement-below as lyric text). Raw MIDI lanes, hidden-rest layout, routing/player assignments and graph state require native project JSON. Export reports omitted MIDI settings and any fraction rounding to 1/20160 quarter beats. Import accepts uncompressed `.xml`/`.musicxml` score-partwise files up to 4 MB, rejects entity declarations, and reports unsupported notation. This is a documented subset, not a guarantee of lossless exchange with every notation application.

The editor does not include page engraving, percussion/tab staves, nested repeats, arbitrary ending lists or comprehensive cross-staff collision handling. Physical iPad gestures, hardware MIDI output and performance-load acceptance remain manual checks.
