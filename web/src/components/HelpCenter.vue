<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from 'vue'
import { ExternalLink, Search, X } from '@lucide/vue'
import type { Descriptor, Signal } from '../types'

const props = defineProps<{ descriptors: Descriptor[]; standalone?: boolean }>()
const emit = defineEmits<{ close: [] }>()
const dialog = ref<HTMLDialogElement>()
const topic = ref<'quick-start' | 'first-project' | 'performance' | 'interface' | 'nodes'>('quick-start')
const nodeCategory = ref('All nodes')
const nodeSearch = ref('')
const example = ref<Descriptor | null>(null)
const previousTitle = document.title

const categories = computed(() => [...new Set(props.descriptors.map(node => node.category))].sort((a, b) => a.localeCompare(b)))
const visibleNodes = computed(() => {
  const query = nodeSearch.value.trim().toLocaleLowerCase()
  return [...props.descriptors]
    .filter(node => nodeCategory.value === 'All nodes' || node.category === nodeCategory.value)
    .filter(node => !query || [node.label, node.kind, node.category, node.description, ...node.aliases].join(' ').toLocaleLowerCase().includes(query))
    .sort((a, b) => a.label.localeCompare(b.label))
})

type ExampleStep = { descriptor: Descriptor; port?: string; signal?: Signal; focus?: boolean }
const preferredSources: Record<Signal, string[]> = {
  audio: ['oscillator', 'sampler', 'audio_input', 'browser_input'],
  control: ['lfo', 'knobs', 'constant'],
  spectral: ['fft', 'stft'],
  midi: ['piano', 'part_midi', 'midi_input'],
}
const preferredTargets: Record<Signal, string[]> = {
  audio: ['gain', 'meter', 'output', 'monitor_output'],
  control: ['gain', 'math_multiply', 'control_output'],
  spectral: ['ifft', 'spectral_filter'],
  midi: ['synth', 'fm_synth', 'midi_output'],
}
function rank(kind: string, preferred: string[]) {
  const index = preferred.indexOf(kind)
  return index < 0 ? preferred.length : index
}
function sourceFor(node: Descriptor) {
  const input = node.inputs[0]
  if (!input) return
  const candidate = props.descriptors
    .filter(item => item.kind !== node.kind && item.outputs.some(port => port.signal === input.signal))
    .sort((a, b) => rank(a.kind, preferredSources[input.signal]) - rank(b.kind, preferredSources[input.signal]) || a.label.localeCompare(b.label))[0]
  const output = candidate?.outputs.find(port => port.signal === input.signal)
  return candidate && output ? { descriptor: candidate, port: `${output.label} → ${input.label}`, signal: input.signal } : undefined
}
function targetFor(node: Descriptor) {
  const output = node.outputs[0]
  if (!output) return
  const candidate = props.descriptors
    .filter(item => item.kind !== node.kind && item.inputs.some(port => port.signal === output.signal))
    .sort((a, b) => rank(a.kind, preferredTargets[output.signal]) - rank(b.kind, preferredTargets[output.signal]) || a.label.localeCompare(b.label))[0]
  const input = candidate?.inputs.find(port => port.signal === output.signal)
  return candidate && input ? { descriptor: candidate, port: `${output.label} → ${input.label}`, signal: output.signal } : undefined
}
const exampleSteps = computed<ExampleStep[]>(() => {
  if (!example.value) return []
  const before = sourceFor(example.value)
  const after = targetFor(example.value)
  return [...(before ? [before] : []), { descriptor: example.value, focus: true }, ...(after ? [after] : [])]
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
        <p>Using pr0former</p>
        <button :class="{ active: topic === 'performance' }" @click="selectTopic('performance')">Performance modes</button>
        <button :class="{ active: topic === 'interface' }" @click="selectTopic('interface')">Interface guide</button>
        <p>Reference</p>
        <button :class="{ active: topic === 'nodes' && nodeCategory === 'All nodes' }" @click="selectCategory('All nodes')">All nodes <span>{{ descriptors.length }}</span></button>
        <button v-for="category in categories" :key="category" :class="{ active: topic === 'nodes' && nodeCategory === category }" @click="selectCategory(category)">{{ category }} <span>{{ descriptors.filter(node => node.category === category).length }}</span></button>
      </nav>

      <main class="help-content" tabindex="-1">
        <article v-if="topic === 'quick-start'" class="help-article">
          <div class="eyebrow">GETTING STARTED</div><h1>Quick start</h1>
          <p class="lead">Build a signal graph, enable its engine, and listen from the browser—all without entering performance mode.</p>
          <ol class="steps">
            <li><b>Create or open a project.</b><span>Choose a mode and tempo. These can be changed later while the show is inactive.</span></li>
            <li><b>Build the graph.</b><span>Drag nodes from the library onto the canvas, then drag from an output port to a compatible input port.</span></li>
            <li><b>Set node parameters.</b><span>Click a node’s gear. Every editable parameter lives in that modal; connected parameters show their live source instead.</span></li>
            <li><b>Enable the audio engine.</b><span>The engine runs the graph during development. Use Browser monitor in the footer if you want to hear its monitor mix here.</span></li>
            <li><b>Press Play when you need the score.</b><span>Graph clocks and instruments can run while stopped; Play starts scheduled score and part events.</span></li>
          </ol>
          <aside><b>Signal colors</b><span><i class="audio"></i> Audio</span><span><i class="control"></i> Control</span><span><i class="spectral"></i> Spectral</span><span><i class="midi"></i> MIDI</span></aside>
        </article>

        <article v-else-if="topic === 'first-project'" class="help-article">
          <div class="eyebrow">GETTING STARTED</div><h1>My first project</h1>
          <p class="lead">A small oscillator patch is the fastest way to learn the graph without needing a microphone or MIDI device.</p>
          <section><h2>1. Start with a Freeform project</h2><p>Freeform is comfortable for experimenting because performers can launch their own parts. The graph itself works the same in every project mode.</p></section>
          <section><h2>2. Make a safe audio chain</h2><p>Add an Oscillator, Gain, Meter, and Output. Connect them in that order. Open Gain and begin around −18 dB before enabling the engine.</p>
            <div class="static-example" aria-label="Example first project graph"><div class="doc-node audio"><small>SOURCE</small><b>∿ Oscillator</b></div><i class="doc-wire audio"></i><div class="doc-node audio"><small>LEVEL</small><b>× Gain</b></div><i class="doc-wire audio"></i><div class="doc-node audio"><small>ANALYSIS</small><b>▥ Meter</b></div><i class="doc-wire audio"></i><div class="doc-node audio"><small>ROUTING</small><b>⇥ Output</b></div></div>
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
        </article>

        <article v-else-if="topic === 'interface'" class="help-article">
          <div class="eyebrow">WORKSPACE TOUR</div><h1>How the interface is laid out</h1>
          <p class="lead">The top chooses context, the middle is your working surface, and the footer controls the running engine and show.</p>
          <div class="interface-map" aria-label="Interface layout diagram"><div class="map-top">TOP BAR · project, connection, save, help, settings, account</div><div class="map-left">LIBRARY<br><small>nodes, samples, subgraphs</small></div><div class="map-main">WORKSPACE TABS<br><small>graph · score · conductor · ensemble · monitor</small></div><div class="map-bottom">TRANSPORT · play, tempo, engine, performance, monitor</div></div>
          <section><h2>Graph</h2><p>Drag nodes from the left library, connect compatible ports, and use the canvas controls to move or fit the patch. Audio ports are round/cyan, control ports amber, spectral ports violet, and MIDI ports use their typed contract. Right-click a node or selection for layout, subgraph, duplicate, and delete actions.</p></section>
          <section><h2>Score</h2><p>Write and edit shared notation, parts, staves, dynamics, automation, repeats, tempo, and structural events. The score is horizontally continuous rather than paginated.</p></section>
          <section><h2>Conductor, Ensemble, and Monitor</h2><p>Conductor manages cues and performance sets. Ensemble manages members and assignments. Monitor selects the browser feed, local inputs, and diagnostics; it does not replace hardware output routing.</p></section>
          <section><h2>Top bar and transport</h2><p>The top bar holds project navigation, save/revision state, explanations, and the gear menu. The footer controls show transport, count-in, tempo, engine preparation, stage view, and browser monitoring. Browser timers display status only; musical scheduling follows engine sample time.</p></section>
          <section><h2>Keyboard and accessibility</h2><p>Space toggles play/pause when the workspace has focus. In the graph, N opens quick insert, A selects all, L auto-spaces a selection, and Delete removes selected items. Press I to show full explanations or collapse them behind info buttons. Reduced-motion and coarse-pointer layouts are supported.</p></section>
        </article>

        <article v-else class="help-article node-reference">
          <div class="eyebrow">REFERENCE</div><h1>{{ nodeCategory }}</h1>
          <p class="lead">Descriptions, port contracts, and parameters come from the same server catalog used to build and validate the graph.</p>
          <label class="node-search"><Search :size="16" /><span class="sr-only">Search node documentation</span><input v-model="nodeSearch" placeholder="Search nodes, aliases, or descriptions…" aria-label="Search node documentation" /></label>
          <p v-if="!visibleNodes.length" class="empty-help">No nodes match that search.</p>
          <div class="node-doc-grid">
            <section v-for="node in visibleNodes" :key="node.kind" class="node-doc-card">
              <header><span :class="node.outputs[0]?.signal || node.inputs[0]?.signal || 'control'">{{ node.symbol }}</span><div><small>{{ node.category }}</small><h2>{{ node.label }}</h2></div></header>
              <p>{{ node.description }}</p>
              <dl><template v-if="node.inputs.length"><dt>Inputs</dt><dd>{{ node.inputs.map(port => `${port.label} · ${port.signal}`).join(', ') }}</dd></template><template v-if="node.outputs.length"><dt>Outputs</dt><dd>{{ node.outputs.map(port => `${port.label} · ${port.signal}`).join(', ') }}</dd></template><template v-if="node.parameters.length"><dt>Parameters</dt><dd>{{ node.parameters.map(parameter => `${parameter.label}${parameter.unit ? ` (${parameter.unit})` : ''}`).join(', ') }}</dd></template><template v-if="node.aliases.length"><dt>Also called</dt><dd>{{ node.aliases.join(', ') }}</dd></template></dl>
              <button class="button small" @click="example = node">Show example graph</button>
            </section>
          </div>
        </article>
      </main>
    </div>

    <div v-if="example" class="example-overlay" role="dialog" aria-modal="true" :aria-label="`${example.label} example graph`" @click.self="example = null">
      <section class="example-card">
        <header><div><div class="eyebrow">ILLUSTRATIVE ROUTE</div><h2>{{ example.label }} example</h2></div><button class="icon-button" aria-label="Close example graph" @click="example = null"><X :size="20" /></button></header>
        <p>This mock graph shows one compatible way to place {{ example.label }} in a patch. Port labels on the wires identify the connection.</p>
        <div class="dynamic-example">
          <template v-for="(step, index) in exampleSteps" :key="step.descriptor.kind">
            <div v-if="index" class="example-wire"><i :class="step.signal || exampleSteps[index - 1]?.descriptor.outputs[0]?.signal || 'control'"></i><span>{{ step.port }}</span></div>
            <div class="doc-node large" :class="[{ focus: step.focus }, step.descriptor.outputs[0]?.signal || step.descriptor.inputs[0]?.signal || 'control']"><small>{{ step.descriptor.category }}</small><b><em>{{ step.descriptor.symbol }}</em>{{ step.descriptor.label }}</b><span>{{ step.focus ? 'DOCUMENTED NODE' : 'COMPATIBLE EXAMPLE' }}</span></div>
          </template>
        </div>
        <p class="example-note">This is documentation artwork, not a running engine graph. Channel widths and required settings still need to match in your project.</p>
      </section>
    </div>
  </component>
</template>

<style scoped>
.help-center{position:fixed;width:min(1180px,calc(100vw - 48px));height:min(820px,calc(100dvh - 48px));max-width:none;max-height:none;margin:auto;padding:0;border:1px solid #465557;border-radius:12px;background:#182022;color:var(--white);box-shadow:0 30px 100px #0009;overflow:hidden}.help-center::backdrop{background:#060b0db3;backdrop-filter:blur(4px)}.help-standalone{inset:0;width:100%;height:100dvh;border:0;border-radius:0}.help-header{height:78px;display:flex;align-items:center;gap:15px;padding:0 25px;border-bottom:1px solid var(--line);background:#1d2527}.help-header h2{margin-top:2px}.help-header .button{margin-left:auto}.help-layout{height:calc(100% - 78px);display:grid;grid-template-columns:236px minmax(0,1fr)}.help-nav{overflow:auto;padding:18px 12px 28px;border-right:1px solid var(--line);background:#151c1e;overscroll-behavior:contain}.help-nav p{padding:13px 11px 6px;color:#6f8385;font-size:8px;font-weight:700;letter-spacing:1.6px;text-transform:uppercase}.help-nav button{display:flex;align-items:center;width:100%;min-height:38px;padding:8px 11px;border-radius:5px;color:#9cafb0;text-align:left;font-size:11px}.help-nav button:hover{background:#ffffff08;color:var(--white)}.help-nav button.active{background:#263638;color:var(--cyan)}.help-nav button span{margin-left:auto;color:#607679;font-size:9px}.help-content{overflow:auto;padding:42px clamp(28px,5vw,68px) 70px;outline:0;overscroll-behavior:contain}.help-article{max-width:900px;margin:0 auto}.help-article>h1{font:500 clamp(30px,4vw,45px) 'Space Grotesk',sans-serif;letter-spacing:-1.5px;margin:6px 0 13px}.help-article .lead{max-width:760px;color:#9eb0b2;font-size:15px;line-height:1.65;margin-bottom:34px}.help-article>section{padding:25px 0;border-top:1px solid var(--line)}.help-article>section h2{font-size:18px;margin-bottom:8px}.help-article>section p{color:#98aaac;font-size:13px}.steps{list-style:none;counter-reset:step;padding:0;margin:0 0 32px;display:grid;gap:10px}.steps li{counter-increment:step;display:grid;grid-template-columns:38px 150px 1fr;align-items:start;gap:14px;padding:17px;border:1px solid var(--line);border-radius:8px;background:#1d2729}.steps li:before{content:counter(step);display:grid;place-items:center;width:28px;height:28px;border:1px solid #4d6b68;border-radius:50%;color:var(--cyan);font:12px 'Space Grotesk',sans-serif}.steps b{font-size:12px;padding-top:5px}.steps span{color:#91a5a7;font-size:12px;line-height:1.55}.help-article aside{display:flex;align-items:center;flex-wrap:wrap;gap:13px 22px;padding:16px 18px;background:#12191a;border-radius:7px;color:#91a4a6;font-size:11px}.help-article aside>b{color:var(--white);margin-right:auto}.help-article aside span{display:flex;align-items:center;gap:7px}.help-article aside i{width:8px;height:8px;background:var(--cyan);border-radius:50%}.help-article aside i.control{background:var(--amber);border-radius:1px;transform:rotate(45deg)}.help-article aside i.spectral{background:var(--violet);border-radius:0}.help-article aside i.midi{background:#a8c784;border-radius:2px}.static-example,.dynamic-example{display:flex;align-items:center;justify-content:center;gap:0;overflow-x:auto;padding:28px 12px;margin-top:18px;border:1px solid #334144;border-radius:8px;background:radial-gradient(ellipse at center,#213032,#12191a)}.doc-node{flex:0 0 auto;width:132px;padding:11px 12px;border:1px solid #546466;border-top:2px solid var(--cyan);border-radius:6px;background:linear-gradient(145deg,#293234,#202729);box-shadow:0 8px 20px #0004}.doc-node small{display:block;color:#718588;font-size:7px;letter-spacing:1.2px;text-transform:uppercase;margin-bottom:9px}.doc-node b{display:flex;gap:7px;align-items:center;font:500 11px 'Space Grotesk',sans-serif}.doc-node em{color:var(--cyan);font-size:20px;font-style:normal}.doc-node>span{display:block;margin-top:10px;color:#607679;font-size:7px;letter-spacing:1px}.doc-node.control{border-top-color:var(--amber)}.doc-node.spectral{border-top-color:var(--violet)}.doc-node.midi{border-top-color:#a8c784}.doc-node.focus{border-color:var(--cyan);box-shadow:0 0 0 3px #79d5ce18,0 8px 20px #0004}.doc-node.large{width:158px}.doc-wire{width:42px;height:2px;flex:0 0 42px;background:var(--cyan)}.doc-wire.control{background:var(--amber)}.doc-wire.spectral{background:var(--violet)}.doc-wire.midi{background:#a8c784}.mode-docs{display:grid;grid-template-columns:repeat(3,1fr);gap:14px;margin-bottom:31px}.mode-docs section{padding:23px;border:1px solid var(--line);border-radius:8px;background:#1d2729}.mode-docs section>span{color:var(--amber);font-size:8px;letter-spacing:1.4px}.mode-docs h2{font-size:17px;margin:8px 0 12px}.mode-docs p{color:#92a5a7;font-size:12px;min-height:116px}.mode-docs b{display:block;margin-top:15px;color:#bdc9c9;font-size:10px;line-height:1.5}.interface-map{display:grid;grid-template-columns:180px 1fr;grid-template-rows:52px 190px 56px;gap:7px;margin:5px 0 34px;padding:12px;border:1px solid #384649;border-radius:9px;background:#111718;font-size:9px;letter-spacing:1px;text-align:center}.interface-map>div{display:grid;place-items:center;border:1px solid #3b4a4c;border-radius:5px;background:#1e292b;color:#91a5a7}.interface-map small{font-size:8px;color:#607679}.map-top,.map-bottom{grid-column:1/-1}.map-main{background:#1b2829!important;color:var(--cyan)!important}.node-reference{max-width:1040px}.node-search{display:flex;align-items:center;gap:9px;width:min(520px,100%);padding:0 11px;margin-bottom:24px;border:1px solid var(--line);border-radius:6px;background:#111718;color:#718688}.node-search input{width:100%;border:0;background:transparent;padding:11px 0;font-size:12px}.node-search input:focus{outline:0}.node-doc-grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(285px,1fr));gap:13px}.node-doc-card{display:flex;flex-direction:column;padding:19px;border:1px solid var(--line);border-radius:8px;background:#1d2729}.node-doc-card header{display:flex;align-items:center;gap:12px}.node-doc-card header>span{display:grid;place-items:center;width:38px;height:38px;border:1px solid #4a5f61;border-radius:6px;color:var(--cyan);font:21px 'Space Grotesk',sans-serif}.node-doc-card header>span.control{color:var(--amber);border-color:#5a5039}.node-doc-card header>span.spectral{color:var(--violet);border-color:#514262}.node-doc-card header>span.midi{color:#a8c784;border-color:#526044}.node-doc-card header small{color:#708486;font-size:7px;letter-spacing:1.2px;text-transform:uppercase}.node-doc-card h2{font-size:15px;margin-top:2px}.node-doc-card>p{color:#96a8aa;font-size:11px;line-height:1.6;margin:14px 0}.node-doc-card dl{margin:auto 0 15px;padding-top:12px;border-top:1px solid #334144}.node-doc-card dt{color:#718688;font-size:8px;letter-spacing:1px;text-transform:uppercase;margin-top:8px}.node-doc-card dd{margin:3px 0;color:#aab9ba;font-size:10px;line-height:1.5}.node-doc-card .button{align-self:flex-start}.empty-help{padding:30px;border:1px dashed var(--line);color:var(--muted);text-align:center}.example-overlay{position:absolute;inset:0;z-index:3;display:grid;place-items:center;padding:28px;background:#060b0dda;backdrop-filter:blur(5px)}.example-card{width:min(820px,100%);max-height:100%;overflow:auto;padding:25px;border:1px solid #465557;border-radius:10px;background:#1b2426;box-shadow:0 24px 80px #000a}.example-card>header{display:flex;align-items:center;justify-content:space-between}.example-card>header h2{margin-top:3px}.example-card>p{color:#94a7a9;font-size:12px;margin-top:13px}.dynamic-example{min-height:190px}.example-wire{position:relative;width:95px;min-width:72px;height:2px;background:#486c69}.example-wire i{display:block;width:100%;height:2px;background:var(--cyan)}.example-wire i.control{background:var(--amber)}.example-wire i.spectral{background:var(--violet)}.example-wire i.midi{background:#a8c784}.example-wire span{position:absolute;left:50%;bottom:8px;width:120px;transform:translateX(-50%);color:#789092;font-size:8px;text-align:center}.example-note{font-size:10px!important;color:#6f8587!important}.sr-only{position:absolute!important;width:1px;height:1px;margin:-1px;padding:0;overflow:hidden;clip:rect(0 0 0 0);white-space:nowrap;border:0}
@media(max-width:800px){.help-center{width:100%;height:100dvh;border:0;border-radius:0}.help-header{height:68px;padding:0 15px}.help-header .button{font-size:0;padding:8px}.help-layout{height:calc(100% - 68px);grid-template-columns:150px 1fr}.help-nav{padding:10px 6px 20px}.help-nav button{font-size:10px;padding:7px}.help-content{padding:28px 20px 55px}.steps li{grid-template-columns:32px 1fr}.steps li span{grid-column:2}.mode-docs{grid-template-columns:1fr}.mode-docs p{min-height:0}.interface-map{grid-template-columns:110px 1fr}.static-example{justify-content:flex-start}.doc-node{width:115px}.doc-wire{width:28px;flex-basis:28px}.dynamic-example{justify-content:flex-start}.example-card{padding:18px}}
@media(max-width:520px){.help-layout{display:flex;flex-direction:column}.help-nav{display:flex;flex:0 0 auto;overflow-x:auto;border-right:0;border-bottom:1px solid var(--line);padding:7px}.help-nav p{display:none}.help-nav button{width:auto;white-space:nowrap}.help-nav button span{margin-left:7px}.help-content{flex:1}.help-header .eyebrow{display:none}.help-header h2{font-size:20px}.node-doc-grid{grid-template-columns:1fr}.interface-map{grid-template-columns:82px 1fr}}
</style>
