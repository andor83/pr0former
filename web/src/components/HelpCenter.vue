<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { ExternalLink, Search, X } from '@lucide/vue'
import type { Descriptor } from '../types'
import DocumentationGraph from './DocumentationGraph.vue'
import SetupGuide from './SetupGuide.vue'
import ScoreEntryGuide from './ScoreEntryGuide.vue'
import ScriptGuide from './ScriptGuide.vue'
import SampleCreditsGuide from './SampleCreditsGuide.vue'

const props = defineProps<{ descriptors: Descriptor[]; standalone?: boolean }>()
const emit = defineEmits<{ close: [] }>()
const dialog = ref<HTMLDialogElement>()
type Topic = 'quick-start' | 'architecture' | 'init' | 'build' | 'first-project' | 'performance' | 'interface' | 'score-entry' | 'nodes' | 'scripting' | 'credits'
const requestedTopic = new URLSearchParams(location.search).get('topic')
const topic = ref<Topic>(requestedTopic === 'scripting' || requestedTopic === 'credits' ? requestedTopic : 'quick-start')
const nodeCategory = ref('All nodes')
const nodeSearch = ref('')
const example = ref<Descriptor | null>(null)
const exampleDialog = ref<HTMLDialogElement>()
watch(example, async value => { if (value) { await nextTick(); exampleDialog.value?.showModal() } })
const previousTitle = document.title

const categories = computed(() => [...new Set(props.descriptors.map(node => node.category))].sort((a, b) => a.localeCompare(b)))
const visibleNodes = computed(() => {
  const query = nodeSearch.value.trim().toLocaleLowerCase()
  return [...props.descriptors]
    .filter(node => nodeCategory.value === 'All nodes' || node.category === nodeCategory.value)
    .filter(node => !query || [node.label, node.kind, node.category, node.description, node.documentation?.title, node.documentation?.explanation, ...node.aliases].join(' ').toLocaleLowerCase().includes(query))
    .sort((a, b) => a.label.localeCompare(b.label))
})

function selectTopic(next: typeof topic.value) {
  topic.value = next
  if (next !== 'nodes') example.value = null
  void nextTick(() => document.querySelector<HTMLElement>('.help-content')?.focus())
}
function selectCategory(category: string) {
  nodeCategory.value = category
  topic.value = 'nodes'
  example.value = null
}
function popout() {
  const url = new URL(location.href)
  url.search = ''
  url.searchParams.set('help', '1')
  window.open(url.href, '_blank', 'noopener')
}
onMounted(() => {
  if (props.standalone) document.title = 'Documentation — pr0former'
  else dialog.value?.showModal()
})
onBeforeUnmount(() => { if (props.standalone) document.title = previousTitle })
</script>

<template>
  <component
    :is="standalone ? 'section' : 'dialog'"
    ref="dialog"
    class="help-center"
    :class="{ 'help-standalone': standalone }"
    aria-labelledby="help-title"
    @cancel.prevent="emit('close')"
  >
    <header class="help-header">
      <div><div class="eyebrow">PR0FORMER GUIDE</div><h2 id="help-title">Documentation</h2></div>
      <button v-if="!standalone" class="button small" @click="popout"><ExternalLink :size="16" /> Open in new tab</button>
      <button v-if="!standalone" class="icon-button" aria-label="Close documentation" @click="emit('close')"><X :size="20" /></button>
    </header>
    <div class="help-layout">
      <nav class="help-nav" aria-label="Documentation sections">
        <p>Getting started</p>
        <button :class="{ active: topic === 'quick-start' }" @click="selectTopic('quick-start')">Quick start</button>
        <button :class="{ active: topic === 'first-project' }" @click="selectTopic('first-project')">My first project</button>
        <button :class="{ active: topic === 'architecture' }" @click="selectTopic('architecture')">Architecture</button>
        <button :class="{ active: topic === 'init' }" @click="selectTopic('init')">init.sh · Server setup</button>
        <button :class="{ active: topic === 'build' }" @click="selectTopic('build')">build.sh · Desktop build</button>
        <p>Using pr0former</p>
        <button :class="{ active: topic === 'performance' }" @click="selectTopic('performance')">Performance modes</button>
        <button :class="{ active: topic === 'interface' }" @click="selectTopic('interface')">Interface guide</button>
        <button :class="{ active: topic === 'score-entry' }" @click="selectTopic('score-entry')">Score entry</button>
        <button :class="{ active: topic === 'scripting' }" @click="selectTopic('scripting')">JavaScript scripting</button>
        <button :class="{ active: topic === 'credits' }" @click="selectTopic('credits')">Sample credits</button>
        <p>Reference</p>
        <button :class="{ active: topic === 'nodes' && nodeCategory === 'All nodes' }" @click="selectCategory('All nodes')">All nodes <span>{{ descriptors.length }}</span></button>
        <button v-for="category in categories" :key="category" :class="{ active: topic === 'nodes' && nodeCategory === category }" @click="selectCategory(category)">{{ category }} <span>{{ descriptors.filter(node => node.category === category).length }}</span></button>
      </nav>

      <main class="help-content" tabindex="-1">
        <SetupGuide v-if="topic === 'quick-start' || topic === 'architecture' || topic === 'init' || topic === 'build'" :topic="topic" />

        <ScriptGuide v-else-if="topic === 'scripting'" />
        <SampleCreditsGuide v-else-if="topic === 'credits'" />
        <ScoreEntryGuide v-else-if="topic === 'score-entry'" />

        <article v-else-if="topic === 'first-project'" class="help-article">
          <div class="eyebrow">GETTING STARTED</div><h1>My first project</h1>
          <p class="lead">A small oscillator patch is the fastest way to learn the graph without needing a microphone or MIDI device.</p>
          <section><h2>1. Start with a Freeform project</h2><p>Freeform is comfortable for experimenting because performers can launch their own parts. The graph itself works the same in every project mode.</p></section>
          <section><h2>2. Make a safe audio chain</h2><p>Add an Oscillator, Gain, Meter, and Monitor output. Connect them in that order. Set every node to two channels. Open Gain and begin around −18 dB before enabling the engine, then select and connect the dedicated feed in the browser monitor.</p>
            <DocumentationGraph v-if="descriptors.find(d => d.kind === 'oscillator')?.documentation" :documentation="descriptors.find(d => d.kind === 'oscillator')!.documentation!" :descriptors="descriptors" focus-kind="oscillator" />
          </section>
          <section><h2>3. Add musical control</h2><p>Connect a Piano or Part MIDI node to a Synth/FM Synth typed MIDI input. Typed MIDI preserves notes, velocity, channel messages, and pitch bend together.</p></section>
          <section><h2>4. Save a revision</h2><p>Edits update the working project. Use Save revision to create a named point in history; the unsaved dot tells you when the working copy is newer.</p></section>
        </article>

        <article v-else-if="topic === 'performance'" class="help-article">
          <div class="eyebrow">PROJECT BEHAVIOR</div><h1>Performance modes</h1>
          <p class="lead">The mode decides who launches musical material and how the shared score participates. It does not change the graph’s signal-processing rules.</p>
          <div class="mode-docs">
            <section><span>STRUCTURED</span><h2>One shared timeline</h2><p>Use this for a composed show in which parts follow the score and transport together. Play, pause, tempo, count-in, repeats, and navigation are conductor-controlled.</p><b>Good for fixed-form pieces and synchronized playback.</b></section>
            <section><span>CONDUCTED</span><h2>Cues from a conductor</h2><p>Arrange sets and performer tiles, then launch or arm individual parts and groups on musical pulse boundaries. Performer stage views stay focused on assigned material.</p><b>Good for flexible order with centralized cueing.</b></section>
            <section><span>FREEFORM</span><h2>Independent launches</h2><p>Performers can start and stop their own parts while the graph continues to run. Use the common tempo and graph clocks to keep independent material related.</p><b>Good for installations, improvisation, and rehearsal.</b></section>
          </div>
          <section><h2>Performance mode button</h2><p>The footer’s Performance mode button opens the reduced player-facing stage. It is a presentation view, distinct from the project’s Structured, Conducted, or Freeform scheduling mode.</p></section>
          <section><h2>Preparation and live edits</h2><p>Enable audio to prepare the project graph. Graph edits are validated by the server and installed between DSP blocks. A conductor can lock project edits during a performance; players remain locally read-only.</p></section>
          <section><h2>Rehearse conducted cues in the editor</h2><p>Start the project with the transport’s Play button, then open Conductor. Click a part tile to start or stop it on a pulse; double-click or use ARM to prepare it. PLAY ARMED, PLAY + REPEAT, live dynamics and Stop all next pulse work in the editor as well as the performance view. The conductor and editors can rehearse during preparation; live cue authority stays with the conductor during a locked performance. Green fill advances from left to right through each playing part.</p></section>
          <section><h2>Bind a MIDI controller</h2><p>The designated conductor clicks Connect local MIDI to share their browser’s inputs. With no designated conductor, the owner supplies them. Local MIDI requires a supporting browser on HTTPS or localhost. Server-connected MIDI inputs are also available. The conductor and editors can assign devices; local input always comes from the designated conductor.</p><ol><li>Click Bind MIDI. Optionally narrow Source and Device, or leave them on the first input received.</li><li>Click a highlighted part tile, ARM button, set, Next set, play/repeat/stop button, or group dynamic. For a set-selection slider or knob, click Bind set slider / knob.</li><li>Press or move the controller. The first supported MIDI control message saves its device, channel and note/controller number. Clock and note releases do not bind. The learning gesture does not trigger a cue.</li><li>Press Escape or click outside the binding controls to leave bind mode and cancel any pending capture. Cancel pending binding keeps bind mode open.</li></ol><p>A bound part tile toggles on/off on successive presses. Note releases and held controller values do not toggle again. A set slider or knob divides its full range across the ordered sets; Next set advances and wraps. Set selection changes the visible set without stopping existing parts. Dynamics and set selection need a continuous controller.</p><p>Open Bindings to assign a different local or server device or remove an assignment. Bindings are shared project settings, survive reload, and can be changed during performance without changing the score. Previously enabled local MIDI reconnects for that conductor when supported; only one conductor window supplies local MIDI at a time. Physical MIDI timing and browser/device compatibility require rehearsal with your hardware.</p></section>
        </article>

        <article v-else-if="topic === 'interface'" class="help-article">
          <div class="eyebrow">WORKSPACE TOUR</div><h1>How the interface is laid out</h1>
          <p class="lead">The top chooses context, the middle is your working surface, and the footer controls the running engine and show.</p>
          <div class="interface-map" aria-label="Interface layout diagram"><div class="map-top">TOP BAR · project, connection, save, help, settings, account</div><div class="map-left">LIBRARY<br><small>nodes, samples, subgraphs</small></div><div class="map-main">WORKSPACE TABS<br><small>graph · score · conductor · ensemble · monitor</small></div><div class="map-bottom">TRANSPORT · play, tempo, engine, performance, monitor</div></div>
          <section><h2>Graph</h2><p>Drag nodes from the left library, connect compatible ports, and use the canvas controls to move or fit the patch. Audio ports are round/cyan, control ports amber, spectral ports violet, and MIDI ports use their typed contract. Right-click a node or selection for layout, subgraph, duplicate, and delete actions.</p></section>
          <section><h2>Score</h2><p>Write and edit shared notation, parts, staves, dynamics, automation, repeats, tempo, and structural events. The score is horizontally continuous rather than paginated.</p></section>
          <section><h2>Conductor, Ensemble, and Monitor</h2><p>Conductor manages cues and performance sets. Ensemble manages members and assignments. Monitor selects the browser feed, local inputs, and diagnostics; it does not replace hardware output routing.</p></section>
          <section><h2>Top bar and transport</h2><p>The top bar holds project navigation, save/revision state, explanations, and the gear menu. The footer controls show transport, count-in, tempo, engine preparation, stage view, and browser monitoring. Browser timers display status only; musical scheduling follows engine sample time.</p></section>
          <section><h2>Keyboard and accessibility</h2><p>Space toggles play/pause, ` (backtick) toggles the audio engine and Shift+` opens or closes the Console when the workspace has focus. In the graph, N opens quick insert, A selects all, L auto-spaces a selection, and Delete removes selected items. Press I or use the header info button to expand or collapse descriptions in place. Info icons always remain available: hover, focus or tap one to read its explanation. Reduced-motion and coarse-pointer layouts are supported.</p></section>
        </article>

        <article v-else class="help-article node-reference">
          <div class="eyebrow">REFERENCE</div><h1>{{ nodeCategory }}</h1>
          <p class="lead">Descriptions, port contracts, and parameters come from the same server catalog used to build and validate the graph.</p>
          <label class="node-search"><Search :size="16" /><span class="sr-only">Search node documentation</span><input v-model="nodeSearch" placeholder="Search nodes, aliases, or descriptions…" aria-label="Search node documentation" /></label>
          <p v-if="!visibleNodes.length" class="empty-help">No nodes match that search.</p>
          <div class="node-doc-grid">
            <section v-for="node in visibleNodes" :key="node.kind" class="node-doc-card">
              <header><span :class="node.outputs[0]?.signal || node.inputs[0]?.signal || 'control'">{{ node.symbol }}</span><div><small>{{ node.category }}</small><h2>{{ node.label }}</h2></div></header>
              <p>{{ node.description }}</p><details v-if="node.documentation" class="node-usage"><summary>{{ node.documentation.title }}</summary><p>{{node.documentation.explanation}}</p></details>
              <dl><template v-if="node.inputs.length"><dt>Inputs</dt><dd>{{ node.inputs.map(port => `${port.label} · ${port.signal}`).join(', ') }}</dd></template><template v-if="node.outputs.length"><dt>Outputs</dt><dd>{{ node.outputs.map(port => `${port.label} · ${port.signal}`).join(', ') }}</dd></template><template v-if="node.parameters.length"><dt>Parameters</dt><dd>{{ node.parameters.map(parameter => `${parameter.label}${parameter.unit ? ` (${parameter.unit})` : ''}`).join(', ') }}</dd></template><template v-if="node.aliases.length"><dt>Also called</dt><dd>{{ node.aliases.join(', ') }}</dd></template></dl>
              <details class="node-contract"><summary>Port and parameter reference</summary><ul><li v-for="port in node.inputs" :key="'in-'+port.id"><b>In · {{port.label}}</b> · {{port.signal}}<template v-if="port.fixed_channels"> · {{port.fixed_channels}} ch</template></li><li v-for="port in node.outputs" :key="'out-'+port.id"><b>Out · {{port.label}}</b> · {{port.signal}}<template v-if="port.fixed_channels"> · {{port.fixed_channels}} ch</template></li><li v-for="p in node.parameters" :key="p.id"><b>{{p.label}}</b>: {{p.min}}–{{p.max}} {{p.unit}} · default {{p.default}} · {{p.structural ? 'modal setting' : 'can be connected'}}</li></ul></details><button v-if="node.documentation" class="button small" @click="example = node">Show example graph</button><p v-else>No example supplied by this node’s author.</p>
            </section>
          </div>
        </article>
      </main>
    </div>

    <dialog v-if="example?.documentation" ref="exampleDialog" class="example-card" :aria-label="`${example.label} example graph`" @cancel.prevent="example = null">
      <header><div><div class="eyebrow">{{example.label.toUpperCase()}} · WORKED EXAMPLE</div><h2>{{example.documentation.title}}</h2></div><button class="icon-button" aria-label="Close example graph" @click="example = null"><X :size="20" /></button></header>
      <p>{{example.documentation.explanation}}</p>
      <DocumentationGraph :key="example.kind" :documentation="example.documentation" :descriptors="descriptors" :focus-kind="example.kind" />
      <ol v-if="example.documentation.steps.length" class="example-steps"><li v-for="step in example.documentation.steps" :key="step">{{step}}</li></ol>
      <p class="example-note">The same nodes, ports and cables as the main graph, with no audio engine attached. Recreate the settings and complete the setup steps in your own project to try it.</p>
    </dialog>
  </component>
</template>

<style scoped>
.help-center{position:fixed;width:min(1180px,calc(100vw - 48px));height:min(820px,calc(100dvh - 48px));max-width:none;max-height:none;margin:auto;padding:0;border:1px solid #465557;border-radius:12px;background:#182022;color:var(--white);box-shadow:0 30px 100px #0009;overflow:hidden}.help-center::backdrop{background:#060b0db3;backdrop-filter:blur(4px)}.help-standalone{inset:0;width:100%;height:100dvh;border:0;border-radius:0}.help-header{height:78px;display:flex;align-items:center;gap:15px;padding:0 25px;border-bottom:1px solid var(--line);background:#1d2527}.help-header h2{margin-top:2px}.help-header .button{margin-left:auto}.help-layout{height:calc(100% - 78px);display:grid;grid-template-columns:236px minmax(0,1fr)}.help-nav{overflow:auto;padding:18px 12px 28px;border-right:1px solid var(--line);background:#151c1e;overscroll-behavior:contain}.help-nav p{padding:13px 11px 6px;color:#6f8385;font-size:8px;font-weight:700;letter-spacing:1.6px;text-transform:uppercase}.help-nav button{display:flex;align-items:center;width:100%;min-height:38px;padding:8px 11px;border-radius:5px;color:#9cafb0;text-align:left;font-size:11px}.help-nav button:hover{background:#ffffff08;color:var(--white)}.help-nav button.active{background:#263638;color:var(--cyan)}.help-nav button span{margin-left:auto;color:#607679;font-size:9px}.help-content{overflow:auto;padding:42px clamp(28px,5vw,68px) 70px;outline:0;overscroll-behavior:contain}.help-article{max-width:900px;margin:0 auto}.help-article>h1{font:500 clamp(30px,4vw,45px) 'Space Grotesk',sans-serif;letter-spacing:-1.5px;margin:6px 0 13px}.help-article .lead{max-width:760px;color:#9eb0b2;font-size:15px;line-height:1.65;margin-bottom:34px}.help-article>section{padding:25px 0;border-top:1px solid var(--line)}.help-article>section h2{font-size:18px;margin-bottom:8px}.help-article>section p{color:#98aaac;font-size:13px}.steps{list-style:none;counter-reset:step;padding:0;margin:0 0 32px;display:grid;gap:10px}.steps li{counter-increment:step;display:grid;grid-template-columns:38px 150px 1fr;align-items:start;gap:14px;padding:17px;border:1px solid var(--line);border-radius:8px;background:#1d2729}.steps li:before{content:counter(step);display:grid;place-items:center;width:28px;height:28px;border:1px solid #4d6b68;border-radius:50%;color:var(--cyan);font:12px 'Space Grotesk',sans-serif}.steps b{font-size:12px;padding-top:5px}.steps span{color:#91a5a7;font-size:12px;line-height:1.55}.help-article aside{display:flex;align-items:center;flex-wrap:wrap;gap:13px 22px;padding:16px 18px;background:#12191a;border-radius:7px;color:#91a4a6;font-size:11px}.help-article aside>b{color:var(--white);margin-right:auto}.help-article aside span{display:flex;align-items:center;gap:7px}.help-article aside i{width:8px;height:8px;background:var(--cyan);border-radius:50%}.help-article aside i.control{background:var(--amber);border-radius:1px;transform:rotate(45deg)}.help-article aside i.spectral{background:var(--violet);border-radius:0}.help-article aside i.midi{background:#a8c784;border-radius:2px}.mode-docs{display:grid;grid-template-columns:repeat(3,1fr);gap:14px;margin-bottom:31px}.mode-docs section{padding:23px;border:1px solid var(--line);border-radius:8px;background:#1d2729}.mode-docs section>span{color:var(--amber);font-size:8px;letter-spacing:1.4px}.mode-docs h2{font-size:17px;margin:8px 0 12px}.mode-docs p{color:#92a5a7;font-size:12px;min-height:116px}.mode-docs b{display:block;margin-top:15px;color:#bdc9c9;font-size:10px;line-height:1.5}.interface-map{display:grid;grid-template-columns:180px 1fr;grid-template-rows:52px 190px 56px;gap:7px;margin:5px 0 34px;padding:12px;border:1px solid #384649;border-radius:9px;background:#111718;font-size:9px;letter-spacing:1px;text-align:center}.interface-map>div{display:grid;place-items:center;border:1px solid #3b4a4c;border-radius:5px;background:#1e292b;color:#91a5a7}.interface-map small{font-size:8px;color:#607679}.map-top,.map-bottom{grid-column:1/-1}.map-main{background:#1b2829!important;color:var(--cyan)!important}.node-reference{max-width:1040px}.node-search{display:flex;align-items:center;gap:9px;width:min(520px,100%);padding:0 11px;margin-bottom:24px;border:1px solid var(--line);border-radius:6px;background:#111718;color:#718688}.node-search input{width:100%;border:0;background:transparent;padding:11px 0;font-size:12px}.node-search input:focus{outline:0}.node-doc-grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(285px,1fr));gap:13px}.node-doc-card{display:flex;flex-direction:column;padding:19px;border:1px solid var(--line);border-radius:8px;background:#1d2729}.node-doc-card header{display:flex;align-items:center;gap:12px}.node-doc-card header>span{display:grid;place-items:center;width:38px;height:38px;border:1px solid #4a5f61;border-radius:6px;color:var(--cyan);font:21px 'Space Grotesk',sans-serif}.node-doc-card header>span.control{color:var(--amber);border-color:#5a5039}.node-doc-card header>span.spectral{color:var(--violet);border-color:#514262}.node-doc-card header>span.midi{color:#a8c784;border-color:#526044}.node-doc-card header small{color:#708486;font-size:7px;letter-spacing:1.2px;text-transform:uppercase}.node-doc-card h2{font-size:15px;margin-top:2px}.node-doc-card>p{color:#96a8aa;font-size:11px;line-height:1.6;margin:14px 0}.node-doc-card dl{margin:auto 0 15px;padding-top:12px;border-top:1px solid #334144}.node-doc-card dt{color:#718688;font-size:8px;letter-spacing:1px;text-transform:uppercase;margin-top:8px}.node-doc-card dd{margin:3px 0;color:#aab9ba;font-size:10px;line-height:1.5}.node-doc-card .button{align-self:flex-start}.empty-help{padding:30px;border:1px dashed var(--line);color:var(--muted);text-align:center}.example-card{width:min(1120px,calc(100vw - 32px));max-height:calc(100dvh - 32px);color:var(--white);overflow:auto;padding:25px;border:1px solid #465557;border-radius:10px;background:#1b2426;box-shadow:0 24px 80px #000a}.example-card>header{display:flex;align-items:center;justify-content:space-between}.example-card>header h2{margin-top:3px}.example-card>p{color:#94a7a9;font-size:12px;margin-top:13px}.example-note{font-size:10px!important;color:#6f8587!important}.sr-only{position:absolute!important;width:1px;height:1px;margin:-1px;padding:0;overflow:hidden;clip:rect(0 0 0 0);white-space:nowrap;border:0}
@media(max-width:800px){.help-center{width:100%;height:100dvh;border:0;border-radius:0}.help-header{height:68px;padding:0 15px}.help-header .button{font-size:0;padding:8px}.help-layout{height:calc(100% - 68px);grid-template-columns:150px 1fr}.help-nav{padding:10px 6px 20px}.help-nav button{font-size:10px;padding:7px}.help-content{padding:28px 20px 55px}.steps li{grid-template-columns:32px 1fr}.steps li span{grid-column:2}.mode-docs{grid-template-columns:1fr}.mode-docs p{min-height:0}.interface-map{grid-template-columns:110px 1fr}.example-card{padding:18px}}
@media(max-width:520px){.help-layout{display:flex;flex-direction:column}.help-nav{display:flex;flex:0 0 auto;overflow-x:auto;border-right:0;border-bottom:1px solid var(--line);padding:7px}.help-nav p{display:none}.help-nav button{width:auto;white-space:nowrap}.help-nav button span{margin-left:7px}.help-content{flex:1}.help-header .eyebrow{display:none}.help-header h2{font-size:20px}.node-doc-grid{grid-template-columns:1fr}.interface-map{grid-template-columns:82px 1fr}}
.example-card::backdrop{background:#060b0dda;backdrop-filter:blur(5px)}
.example-steps{padding-left:22px;color:#afbec0;font-size:12px;line-height:1.8}.example-steps li{margin:8px 0}.node-usage,.node-contract{font-size:11px;line-height:1.65;color:#afbec0;margin-bottom:16px}.node-usage summary,.node-contract summary{cursor:pointer;color:var(--cyan);padding:6px 0}.node-contract ul{padding-left:16px}
</style>
