<script setup lang="ts">
const shortcuts = [
  ['1 · 2 · 3 · 4', '64th · 32nd · 16th · eighth'],
  ['5 · 6 · 7 · 8', 'Quarter · half · whole · double whole'],
  ['↑ / ↓', 'Move the caret pitch by a staff step; hold Shift for an octave.'],
  ['← / →', 'Move to the previous or next entry boundary; Shift moves by bar. Moving past the final bar offers to add one.'],
  ['A–G', 'Insert the nearest pitch with that letter, using the current duration.'],
  ['Shift + letter or number', 'Add a pitch to the last chord without advancing the caret.'],
  ['0', 'Insert one rest using the current duration, dots and tuplet setting.'],
  ['R', 'Toggle note/rest entry. In Select mode, convert the selected note or rest.'],
  ['. (period)', 'Cycle no dots → one dot → two dots → no dots.'],
  ['T', 'Tie the previous entry into the next same-pitch note you enter; press again to cancel the pending tie.'],
  ['+ / −', 'Raise or lower the accidental. Auto accidental follows the key signature; choose Auto or Natural in the palette.'],
  ['Alt + 1–4', 'Choose the entry voice.'],
  ['Backspace / Delete', 'Delete selected notes/rests, or the entry ending at the caret, and close the gap within the bar.'],
  ['Ctrl/Cmd + Z', 'Undo; Ctrl/Cmd + Shift + Z redoes the edit.'],
  ['Escape', 'Clear the selection, then the bar selection or caret.'],
]
</script>
<template>
  <article class="help-article score-entry-guide">
    <div class="eyebrow">SCORE &amp; PARTS</div><h1>Score entry</h1>
    <p class="lead">Write with the mouse, enter a phrase with arrows and numbers, or use a MIDI keyboard. Stop or pause playback to edit.</p>
    <section><h2>Start writing</h2>
      <p>Open <strong>Score &amp; Parts</strong>, show the part in the left sidebar, and choose <strong>Notation</strong>. Choose <strong>Write</strong> and a note value from the floating palette, then click the staff. This inserts the first note and places the cyan caret immediately after it. Up/Down chooses the next pitch; a number inserts it and advances the caret. This is <strong>quick entry</strong>, also called Speedy Entry.</p>
      <p>A staff click inserts at the bar’s beginning or after the note to its left. Inserting between notes pushes later entries right; a full bar reports an error instead of spilling music into the next bar. Choose a voice or tuplet ratio in <strong>Entry settings</strong> before entering that rhythm.</p>
    </section>
    <section><h2>Add the next bar with the arrow keys</h2>
      <p>In quick entry, press <kbd>→</kbd> or <kbd>Shift + →</kbd> to move into the bar after the final bar. The editor asks <strong>Add a new bar?</strong> Press Enter or choose <strong>Add bar</strong> to append one bar to every part and move the caret to its beginning. Cancel or Escape keeps the caret where it was; left-arrow movement stops at the score’s beginning.</p>
      <p>Check <strong>Don’t ask again this session</strong> when confirming to add subsequent bars automatically with the right arrow. The choice survives changing views and reloading this browser tab, and resets when the tab is closed. Holding an arrow down does not repeatedly create bars.</p>
      <p>The new bar uses the final bar’s time signature. A partial final bar is completed first. Bar creation is one undoable edit and stays subject to the score-length limit; it is unavailable while playback or count-in is running.</p>
    </section>
    <section><h2>Quick entry: rests and their lengths</h2>
      <p><kbd>0</kbd> inserts a rest with the current duration. For example, after entering an eighth note with <kbd>4</kbd>, <kbd>0</kbd> enters an eighth rest. You can also choose a note value in the palette, then press <kbd>0</kbd>.</p>
      <p>To choose a different rest length directly from the keyboard, start in note entry and press <kbd>R</kbd> to switch to rests, press the duration number, then press <kbd>R</kbd> again to return to notes. <strong>R → 5 → R</strong> inserts a quarter rest; <strong>R → 6 → R</strong> inserts a half rest. While rest entry is active, consecutive duration numbers enter consecutive rests. The footer shows whether you are entering notes or rests.</p>
    </section>
    <section><h2>Dotted notes and rests</h2>
      <p>With the caret active and no selection, press <kbd>.</kbd> <strong>before</strong> the duration number. <strong>. → 5</strong> enters a dotted quarter; <strong>. → . → 5</strong>, starting without dots, enters a double-dotted quarter. Dots also apply to rests: <strong>. → R → 4 → R</strong> enters a dotted eighth rest.</p>
      <p>The dot setting stays active for subsequent entries. Period cycles <strong>no dots → one dot → two dots → no dots</strong>; from one dot, press it twice to return to undotted entry. Check the footer’s duration. With a note selected, period changes that selected note instead of the next entry.</p>
    </section>
    <section><h2>Tie into the next note</h2>
      <p>Enter the first note, press <kbd>T</kbd>, then enter the next note at the <strong>same pitch</strong>. For two tied quarters, use <strong>5 → T → 5</strong>. The footer shows <strong>tie pending</strong> until the next entry. The notes must be adjacent in the same staff and voice; a tie can cross a barline. Press T again before entering to cancel.</p>
      <p>To tie existing notes, select the first note and press T to join it to its adjacent same-pitch successor. With two notes selected, T works like the Tie button. Pressing T again on the first selected note removes that tie. A slur is a separate phrasing mark: use the Slur tool to drag between notes.</p>
    </section>
    <section><h2>Keyboard reference</h2>
      <p>Duration numbers insert when the Write caret is active and nothing is selected. With a selection, they change its duration; otherwise they choose the next entry value. Shortcuts do not intercept typing in fields or dialogs.</p>
      <dl class="score-shortcuts"><div v-for="[keys, action] in shortcuts" :key="keys"><dt>{{keys}}</dt><dd>{{action}}</dd></div></dl>
    </section>
    <section><h2>Jazz chord symbols</h2>
      <p>Choose the <strong>Chord tool (C⁷)</strong> in the floating palette and click the desired beat in a bar. Type a symbol such as <strong>Dm7, G7♭9, Cmaj7/E</strong> or <strong>N.C.</strong>, or choose a root, a jazz chord quality and an optional slash bass. The preview shows the result; press Enter or <strong>Add chord</strong> to place it above that staff.</p>
      <p>The tool stays active so you can click the next bar or beat to continue the progression. Press Escape or choose Select to finish. Chords are attached to a staff and beat, so a bar can contain several changes. Clicking an occupied chord beat with the tool edits that symbol. In Select mode, double-click a chord to edit it, drag it to move it, or select it and press Backspace/Delete to remove it. Undo works as for other score marks.</p>
      <p>These are chord symbols for performers; they do not insert or play notes. They appear in read-only notation views and move with bar edits. Native saves preserve all labels. MusicXML carries pitch-root symbols as harmony (including slash bass); custom labels without a pitch root export as text.</p>
    </section>
    <section><h2>Select, move and delete</h2>
      <p>Choose Select and click a note or rest. Ctrl/Cmd-click adds or removes individual items; drag empty space for a selection box. Drag an existing note to move it, with a pitch audition when the engine is enabled. Double-click or press Enter to edit its settings.</p>
      <p>Backspace/Delete closes the removed duration by moving later notes and rests earlier in the <strong>same bar, staff and voice</strong>. Later bars keep their positions. Deleting one pitch from a chord keeps the remaining chord’s occupied time; deleting the whole chord closes its duration once. The caret’s Backspace/Delete does the same and steps back. Automatic rests between notes can also be deleted to close that silence; deleting a trailing automatic rest hides its symbol.</p>
      <p>Backspace/Delete on a selected bar region clears its contents and keeps the bars. To remove bars and close the gap across all parts, right-click a bar or selected bars and choose Delete bar / Delete bars. Keep at least one bar. Measure → Add / delete bars also offers this operation. Deleting an accidental, dot, articulation or phrasing mark edits that mark without deleting its note. Ctrl/Cmd-Z undoes an edit.</p>
      <p>Add part opens instrument presets with editable staff layout, clef and sounding transposition. For percussion, choose noteheads and drum stem marks in Entry settings before writing, or Edit selected notes afterward. Roll slashes and buzz marks are visual only; flags follow duration and MIDI pitches are not automatically mapped to a drum kit.</p>
    </section>
    <section><h2>Piano roll and MIDI entry</h2>
      <p>In Piano roll, choose Write and drag horizontally to draw a note’s duration. Drag its body to move it or its right edge to resize it. Arrow-and-number quick entry is available in Notation.</p>
      <p>The footer’s MIDI selector offers <strong>Play to enter</strong> (play notes at the current duration; held notes form a chord) and <strong>Hold + number</strong> (hold pitches on a MIDI keyboard, then press a duration number). It requires a browser with Web MIDI support and permission to access the input. Choose Off to use only the computer keyboard.</p>
    </section>
    <section><h2>Saving and playback</h2>
      <p>Edits save automatically; the footer reports saving and validation errors. Undo/redo belongs to the open score editor. Play and count-in disable editing; pause or stop to continue. Follow playback, Spacing and Zoom change your view without changing the notes.</p>
    </section>
  </article>
</template>
<style scoped>
.score-entry-guide{max-width:900px;margin:0 auto}.score-entry-guide h1{font:500 clamp(30px,4vw,45px) 'Space Grotesk',sans-serif;letter-spacing:-1.5px;margin:6px 0 13px}.lead{color:#9eb0b2;font-size:15px;line-height:1.65;margin-bottom:30px}.score-entry-guide section{padding:24px 0;border-top:1px solid var(--line)}h2{font-size:18px;margin-bottom:10px}p{color:#a8b9bb;font-size:13px;line-height:1.75}p+p{margin-top:12px}strong{color:var(--white);font-weight:600}kbd{display:inline-block;padding:0 6px;border:1px solid #526365;border-radius:4px;background:#11191b;color:var(--amber);font:inherit}.score-shortcuts{margin-top:16px}.score-shortcuts>div{display:grid;grid-template-columns:minmax(125px,1fr) 3fr;gap:16px;padding:12px 0;border-bottom:1px solid var(--line);font-size:12px;line-height:1.65}dt{color:var(--amber)}dd{margin:0;color:#a8b9bb}@media(max-width:600px){.score-shortcuts>div{grid-template-columns:1fr;gap:3px}}
</style>
