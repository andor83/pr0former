<script setup lang="ts">
import { createScoreDraft, type ScoreDraft } from './scoreDraft'
import { newId } from './id'
import { PresentationClock } from './presentationClock'
import {matchingPorts,selectionOrder} from './connectMatching'
import { defineAsyncComponent, computed, nextTick, markRaw, onBeforeUnmount, onMounted, ref, shallowRef, provide, watch } from 'vue'
import { VueFlow, useVueFlow, SelectionMode } from '@vue-flow/core'
import { graphViewKey, graphViewport, readView, writeView, type GraphViewport } from './viewMemory'
import type { Connection, Node as FlowNode, Edge as FlowEdge } from '@vue-flow/core'
import { createLocalMidiInput } from './localMidiInput'
import { useCanvasTouch } from './canvasTouch'
import {useInputConnectionDrag} from './inputConnectionDrag'
import { useLibraryTouch, type LibraryItem } from './libraryTouch'
import { useTouchScrollContainment } from './touchScroll'
import { Background } from '@vue-flow/background'
import { Controls } from '@vue-flow/controls'
import { Activity, AudioLines, ChevronDown, ChevronRight, Disc3, FolderOpen, Headphones, LayoutGrid, LogOut, Maximize, Moon, Music2, Network, Pause, Play, Plus, Power, Presentation, Radio, Repeat2, Search, Settings, Settings2, Square, Sun, Users, X, Download, Upload, Undo2, Save, Info } from '@lucide/vue'
import SubgraphLibrary from './components/SubgraphLibrary.vue'
import SampleLibrary from './components/SampleLibrary.vue'
import type { SampleEntry } from './samples'
import SaveSubgraphDialog from './components/SaveSubgraphDialog.vue'
import PatchNode from './components/PatchNode.vue'
import SignalEdge from './components/SignalEdge.vue'
import { autoSpace } from './autoLayout'
import NodeModal from './components/NodeModal.vue'
const PartPlaybackDialog = defineAsyncComponent(() => import('./components/PartPlaybackDialog.vue'))
const ScoreWorkspace = defineAsyncComponent(() => import('./components/ScoreWorkspace.vue'))
import MonitorWorkspace from './components/MonitorWorkspace.vue'
import type { HardwareLevels } from './types'
import StageView from './components/StageView.vue'
import ProjectSettings from './components/ProjectSettings.vue'
import SystemSettings from './components/SystemSettings.vue'
import ProjectBrowser from './components/ProjectBrowser.vue'
import QuickNodeBrowser from './components/QuickNodeBrowser.vue'
import {recentProjects} from './projectSearch'
import EngineConsole from './components/EngineConsole.vue'
const HelpCenter = defineAsyncComponent(() => import('./components/HelpCenter.vue'))
import TaskProgress from './components/TaskProgress.vue'
import EnsembleWorkspace from './components/EnsembleWorkspace.vue'
import ConductorWorkspace from './components/ConductorWorkspace.vue'
import { createConductorMidi, conductorMidiKey } from './conductorMidi'
const settingsOpen = ref(false), systemOpen=ref(false), consoleOpen=ref(false), documentationOpen=ref(false), gearOpen=ref(false)
/** Light or dark canvas and library for the signal graph; nodes keep their charcoal styling. */
const graphLight = ref((() => { try { return localStorage.getItem('pr0former.graph.theme') === 'light' } catch { return false } })())
watch(graphLight, light => { try { localStorage.setItem('pr0former.graph.theme', light ? 'light' : 'dark') } catch { /* private mode */ } })
const progress=ref(''),engineEnabled=ref(false),audioSettings=ref<{sample_rate:number;block_size:number;interfaces:any[]}>({sample_rate:48000,block_size:128,interfaces:[]})
const routeParams = new URLSearchParams(location.search)
const consoleProject=routeParams.get('console')
const standaloneHelp=routeParams.has('help')
import UserProfileDialog from './components/UserProfileDialog.vue'
import { avatarUrl, type UserProfile } from './userProfile'
import { desktopProject, desktopView, isDesktopWindow, updateDesktopWindow } from './desktopWindow'
const requestedDesktopProject = desktopProject()
const requestedDesktopView = desktopView(new URLSearchParams(location.search).get('view'))
async function withProgress(title:string,fn:()=>Promise<void>){progress.value=title;try{await fn()}finally{progress.value=''}}
async function refreshAudio(){const [s,d]=await Promise.all([api<typeof audioSettings.value>('/audio/config'),api<any>('/devices')]);audioSettings.value=s;devices.value=d;engineEnabled.value=d.engine_enabled}
async function toggleEngine(){if(!project.value)return;await api(`/projects/${project.value.id}/engine`,'POST',{enabled:!graphActive.value});await refreshAudio()}

const stage = ref(false), stageMonitor = ref(false)
import { api } from './api'
import { nodeDescriptor, descendants, duplicateNodes, makeSubgraph } from './subgraphs'
import {scoreMeasures} from './score'
import { importScoreMusicXML as importMusicXML, exportScoreMusicXML, musicXMLExportWarnings } from './scoreMusicxml'
import type { Descriptor, GraphEdge, GraphNode, IoConfig, Member, Mode, Part, Project, Summary, Telemetry } from './types'
import { showHelp, toggleHelp } from './help'

const user = ref<UserProfile | null>(null)
const accountOpen=ref(false), profileOpen=ref(false), accountButton=ref<HTMLButtonElement>()
async function profileSaved(value:UserProfile){user.value=value;await refreshMembers()}
function closeProfile(){profileOpen.value=false;void nextTick(()=>accountButton.value?.focus())}
function accountKey(event:KeyboardEvent){
  const buttons=Array.from((event.currentTarget as HTMLElement).querySelectorAll<HTMLButtonElement>('[role=menuitem]:not(:disabled)'))
  if(event.key==='Escape'){accountOpen.value=false;accountButton.value?.focus();event.preventDefault()}
  if(event.key==='ArrowDown'||event.key==='ArrowUp'){event.preventDefault();accountOpen.value=true;void nextTick(()=>{const items=Array.from((accountButton.value?.parentElement)?.querySelectorAll<HTMLButtonElement>('[role=menuitem]:not(:disabled)')||[]);const current=buttons.indexOf(document.activeElement as HTMLButtonElement);items[(current+(event.key==='ArrowDown'?1:-1)+items.length)%items.length]?.focus()})}
}
const bootstrap = ref(false), registering = ref(false), username = ref(''), password = ref('')
const busy = ref(false), error = ref(''), notice = ref('')
const monitorWorkspace = ref<InstanceType<typeof MonitorWorkspace>>()
provide('disconnectBrowserInput',async()=>{await monitorWorkspace.value?.disconnectInput()})
provide('connectBrowserInput', async (node:string) => {
  await nextTick()
  await monitorWorkspace.value?.connectInput(node)
})
provide('setNodeParameter', (node:string, key:string, value:number) => queueNodeParameter(node,key,value))
const monitorState = computed(() => monitorWorkspace.value?.monitorState ?? 'disconnected')
const monitorOn = computed(() => !['disconnected', 'disconnecting'].includes(monitorState.value))
const monitorLevel = computed(() => monitorWorkspace.value?.monitorLevel ?? 0)
watch(() => monitorWorkspace.value?.monitorError, message => {
  if (!message) return
  error.value = message
  if (stage.value) stageMonitor.value = true
})
const browserOpen=ref(false),quickNodes=ref(false)
const recentSummaries=computed(()=>recentProjects(summaries.value))
const summaries = ref<Summary[]>([]), project = ref<Project | null>(null), role = ref('performer')
const automaticInput=computed(()=>project.value?.graph.nodes.find(n=>n.kind==='browser_input' && project.value?.local_audio_assignments?.[n.id]===user.value?.id)?.id)
provide('localAudioAccess',computed(()=>({userId:user.value?.id,assignments:project.value?.local_audio_assignments||{},members:members.value,canAssign:['owner','editor'].includes(role.value)&&!performanceLocked.value})))
provide('assignBrowserInput',async(node:string,member:string)=>{if(!project.value)return;const next=clone(project.value);next.local_audio_assignments={...next.local_audio_assignments};if(member)next.local_audio_assignments[node]=member;else delete next.local_audio_assignments[node];await task(()=>saveProject(next))})

const scoreDraft = shallowRef<ScoreDraft | null>(null)
watch(() => project.value?.id, id => {
  scoreDraft.value = id ? createScoreDraft(() => project.value!, saveScoreProject) : null
}, { flush: 'sync' })
const descriptors = ref<Descriptor[]>([]), members = ref<Member[]>([])
const defaultHeroTitles = [
  { first: 'Insert pithy title here', second: 'Put something funny here too' },
  ...['Stop, Collaborate and Listen', 'F*ck it, we\'ll do it live!', 'A very musical hampster wheel', 'Science b!tches', 'ERROR....nah JK', 'This is AI slop', 'Injecting the Raccoons Now', 'Now with 80% more cheese', 'Have you considered how Carl feels?', 'Illegal in many states', 'She turned me into a newt!', 'Welcome back Mr. Wick', 'Turning the frogs gay', 'Your bit drift is showing', 'you forgot to return your Amazon purchase', 'Saints be praised!', 'TETSUOOOOOOO', 'It\'s over 9000!'].map(first => ({ first, second: 'Live Electroacoustic Performance Platform' })),
]
const loginTitles = ref(defaultHeroTitles.map(title => ({ ...title })))
const heroSlogan = ref('')
const heroSubline = ref('')
const heroTitleVisible = ref(false)
let heroTitleTimer:ReturnType<typeof setTimeout>|undefined
const heroTitleFadeMs = window.matchMedia('(prefers-reduced-motion: reduce)').matches ? 0 : 220
type HeroPort = { label:string; signal:string }
type HeroNode = { label:string; symbol:string; signal:string; category:string; channels:number; inputs:HeroPort[]; outputs:HeroPort[]; offset:number }
const heroNodes = ref<HeroNode[]>([])
function heroPortPreview(ports:HeroPort[], connected:HeroPort) {
  return [connected, ...ports.filter(port => port !== connected)].slice(0, 2)
}
function chooseHeroTitle() {
  const alternatives = loginTitles.value.filter(title => title.first !== heroSlogan.value || title.second !== heroSubline.value)
  const choices = alternatives.length ? alternatives : loginTitles.value
  const title = choices[Math.floor(Math.random() * choices.length)] || defaultHeroTitles[0]!
  heroSlogan.value = title.first
  heroSubline.value = title.second
}
function revealHeroTitle() {
  clearTimeout(heroTitleTimer)
  heroTitleVisible.value = false
  chooseHeroTitle()
  void nextTick(() => requestAnimationFrame(() => { heroTitleVisible.value = true }))
}
function cycleHeroTitle() {
  if (!heroTitleVisible.value) return
  clearTimeout(heroTitleTimer)
  heroTitleVisible.value = false
  heroTitleTimer = setTimeout(revealHeroTitle, heroTitleFadeMs)
}
function randomizeHero(randomizeSlogan = true) {
  if (randomizeSlogan) chooseHeroTitle()
  const available = descriptors.value.filter(d => d.outputs.length && d.inputs.length && d.kind !== 'subgraph' && !d.kind.startsWith('subgraph_'))
  for (let attempt = 0; attempt < 80; attempt++) {
    const first = available[Math.floor(Math.random() * available.length)]
    if (!first) break
    const firstOutput = first.outputs[Math.floor(Math.random() * first.outputs.length)]!
    const secondCandidates = available.filter(d => d !== first && d.inputs.some(p => p.signal === firstOutput.signal))
    const second = secondCandidates[Math.floor(Math.random() * secondCandidates.length)]
    const secondInput = second?.inputs.find(port => port.signal === firstOutput.signal)
    const secondOutput = second?.outputs[Math.floor(Math.random() * second.outputs.length)]
    const thirdCandidates = secondOutput ? available.filter(d => d !== first && d !== second && d.inputs.some(p => p.signal === secondOutput.signal)) : []
    const third = thirdCandidates[Math.floor(Math.random() * thirdCandidates.length)]
    const thirdInput = third?.inputs.find(port => port.signal === secondOutput?.signal)
    if (second && secondInput && secondOutput && third && thirdInput) {
      const chain = [
        { descriptor:first, input:first.inputs[0]!, output:firstOutput },
        { descriptor:second, input:secondInput, output:secondOutput },
        { descriptor:third, input:thirdInput, output:third.outputs[0]! },
      ]
      heroNodes.value = chain.map(({descriptor,input,output}, index) => ({
        label:descriptor.label, symbol:descriptor.symbol, category:descriptor.category,
        signal:descriptor.outputs[0]?.signal || descriptor.inputs[0]?.signal || 'control', channels:descriptor.default_channels || 1, offset:[0, 34, 10][index]!,
        inputs:heroPortPreview(descriptor.inputs, input), outputs:heroPortPreview(descriptor.outputs, output),
      }))
      return
    }
  }
  heroNodes.value = []
}
watch(() => descriptors.value.length, count => { if (count && !user.value) randomizeHero(false) }, { immediate: true })
const nodeLibraryOpen = ref(true), sampleLibraryOpen=ref(false)
const samplePanel=ref<InstanceType<typeof SampleLibrary>>(),projectSamples=ref<SampleEntry[]>([])
const tab = ref('graph'), library = ref(true), search = ref(''), category = ref('All nodes')
watch(() => [project.value?.id, project.value?.name, tab.value, stage.value] as const,
  ([id, name, view, isStage]) => updateDesktopWindow(user.value ? id : undefined, name, isStage ? 'stage' : view))
function restoreDesktopView(event: Event) {
  if (!isDesktopWindow()) return
  const view = desktopView((event as CustomEvent<string>).detail)
  stage.value = view === 'stage'
  if (view !== 'stage') tab.value = view
}
window.addEventListener('pr0-desktop-view', restoreDesktopView)
onBeforeUnmount(() => window.removeEventListener('pr0-desktop-view', restoreDesktopView))
watch(()=>[tab.value,stage.value,project.value?.id],()=>{if(tab.value!=='graph'||stage.value||!project.value)quickNodes.value=false})
const partPlayerPreview=ref<string|null>(null)
const previewNode=computed(()=>project.value?.graph.nodes.find(n=>n.id===partPlayerPreview.value && n.kind==='part_player'))
watch(previewNode,node=>{if(!node)partPlayerPreview.value=null})
const selectedNode = ref<string | null>(null), selectedPart = ref(''), projectPicker = ref(false), creating = ref(false)
const newName = ref('Untitled performance'), newMode = ref<Mode>('conducted')
const telemetry = shallowRef<Telemetry | null>(null), receivedAt = ref(0), now = ref(performance.now()), connected = ref(false)
const hardwareLevels = shallowRef<HardwareLevels | null>(null), hardwareReceived = ref(0)
const tempoSource = computed(() => project.value?.graph.edges.find(e => e.target_port === 'tempo' && project.value?.graph.nodes.some(n => n.id === e.target && n.kind === 'clock')))
const graphId = ref<string | null>(null)
const performanceId = ref<string | null>(null)
const activeId = ref<string | null>(null), bpmDraft = ref(120), saving = ref(false), fullscreen = ref(false)
const tempoEditing = ref(false), pendingTempo = ref<number | null>(null)
const devices = ref<any>(null)
type SaveStatus = {revision:number;change_revision:number;dirty:boolean}
const savedRevision = ref<SaveStatus | null>(null), checkpointBusy = ref(false)
const revisionsOpen = ref(false), revisions = ref<{revision:number;current:boolean}[]>([]), loadedRevision = ref<number | null>(null), revisionsBusy = ref(false)
const saveRequested = ref<string | null>(null)
const unsaved = computed(() => !!project.value && (!savedRevision.value || project.value.revision > savedRevision.value.change_revision || parameterPending.value || saving.value || scoreDraft.value?.pending.value || scoreDraft.value?.conflict.value))
const revisionText = computed(() => scoreDraft.value?.conflict.value ? 'Score draft needs attention' : scoreDraft.value?.pending.value ? 'Saving score…' : checkpointBusy.value ? 'Saving revision…' : savedRevision.value ? `Revision ${savedRevision.value.revision}${unsaved.value ? ' · Unsaved changes' : ' · Saved'}` : 'Loading revision…')
function acceptSaveStatus(id: string, saved: SaveStatus) {
  if (project.value?.id === id && (!savedRevision.value || saved.change_revision >= savedRevision.value.change_revision)) savedRevision.value = saved
}
watch(() => project.value?.id, () => { savedRevision.value = null; saveRequested.value = null })
function requestSave() {
  if (!project.value || !editable.value || checkpointBusy.value) return
  saveRequested.value = project.value.id
  void flushManualSave()
}
async function flushManualSave() {
  const id = saveRequested.value
  if (!id || checkpointBusy.value || saving.value || parameterFlush || controlsBusy) return
  if (project.value?.id !== id || !editable.value) { saveRequested.value = null; return }
  if (queuedParameters.size) { clearTimeout(saveTimer); await flushParameters(); return }
  if (pendingControls.size) { await flushControls(); return }
  if (parameterPending.value) return
  saveRequested.value = null; checkpointBusy.value = true
  try {
    await scoreDraft.value?.flush()
    const saved = await api<SaveStatus>(`/projects/${id}/save`, 'POST', {})
    acceptSaveStatus(id, saved)
    if (project.value?.id === id) notice.value = `Revision ${saved.revision} saved`
  } catch (e) { report(e) }
  finally { checkpointBusy.value = false }
}
async function openRevisions() {
  if (!project.value) return
  revisionsBusy.value = true
  try { revisions.value = await api(`/projects/${project.value.id}/revisions`); revisionsOpen.value = true } catch (e) { report(e) }
  finally { revisionsBusy.value = false }
}
async function loadRevision(revision: number) {
  if (!project.value || revisionsBusy.value) return
  revisionsBusy.value = true
  try {
    const result = await api<{project: Project; source_revision:number}>(`/projects/${project.value.id}/revisions/${revision}`)
    project.value = result.project
    bpmDraft.value = result.project.bpm
    loadedRevision.value = result.source_revision === result.project.revision ? null : result.source_revision
    scoreDraft.value = createScoreDraft(() => project.value!, saveScoreProject)
    revisionsOpen.value = false
    notice.value = `Loaded revision ${result.source_revision}; next save will create a new revision.`
  } catch (e) { report(e) }
  finally { revisionsBusy.value = false }
}

const libraryPanel = ref<InstanceType<typeof SubgraphLibrary>>(), savingSubgraph = ref<string | null>(null)
const nodeMenu = ref<{ id: string; ids: string[]; x: number; y: number } | null>(null)
let menuOrigin: HTMLElement | null = null
const undo = ref<Project[]>([]), selectedEdges = ref<string[]>([])
const nodeTypes = { instrument: markRaw(PatchNode) }, edgeTypes = { signal: markRaw(SignalEdge) }
const phoneMedia=window.matchMedia('(max-width:600px), (max-height:500px) and (pointer:coarse)')
const phoneCompact=ref(phoneMedia.matches)
function phoneLayoutChanged(){const wasPhone=phoneCompact.value;phoneCompact.value=phoneMedia.matches;if(wasPhone||phoneCompact.value)void nextTick(()=>requestAnimationFrame(()=>fitView({padding:.18,maxZoom:phoneCompact.value ? 0.5 : 1})))}
const { fitView, setViewport, onViewportChangeEnd, screenToFlowCoordinate, getSelectedNodes, getSelectedEdges, getNodes, findNode, addSelectedNodes, removeSelectedNodes } = useVueFlow()
function selectAllNodes() { if (tab.value === 'graph' && !stage.value) addSelectedNodes(getNodes.value) }
// Lay out the selected nodes in signal-flow columns with crossing reduction;
// rows are pulled toward the ports they connect to (matching PatchNode's port rows).
async function autoSpaceSelection(ids: string[]) {
  if (!project.value || !graphEditable.value || ids.length < 2) return
  nodeMenu.value = null
  const next = clone(project.value), all = next.graph.nodes, list = descriptors.value
  const ports = (n: GraphNode, direction: 'input' | 'output') => {
    const d = nodeDescriptor(n, all, list)
    const raw = direction === 'input' ? [...d.inputs, ...d.parameters.filter(p => !p.structural).map(p => ({ id: p.id, signal: 'control' as const }))] : d.outputs
    return [...raw.filter(p => p.signal === 'midi'), ...raw.filter(p => p.signal !== 'midi')]
  }
  const index = (n: GraphNode, port: string, direction: 'input' | 'output') => Math.max(0, ports(n, direction).findIndex(p => p.id === port))
  const placed = autoSpace(all, next.graph.edges, ids, n => {
    const flow = findNode(n.id)
    return { width: flow?.dimensions?.width || 204, height: flow?.dimensions?.height || 156 }
  }, {
    portIndex: index,
    portCount: (n, direction) => Math.max(1, ports(n, direction).length),
    portOffset: (n, port, direction) => ['trigger', 'toggle'].includes(n.kind) ? 32 : n.kind === 'value' ? (direction === 'input' ? (port === 'trigger' ? 20 : 52) : 36) : 73 + index(n, port, direction) * 30,
  })
  let changed = false
  for (const n of all) { const p = placed.get(n.id); if (p && (p.x !== n.x || p.y !== n.y)) { n.x = p.x; n.y = p.y; changed = true } }
  if (changed) await saveProject(next)
}
const orderedSelection=ref<string[]>([]),selectionGesture=ref(false)
function finishSelection(){orderedSelection.value=selectionOrder([],getSelectedNodes.value.map(n=>n.id),project.value?.graph.nodes??[]);selectionGesture.value=false}
watch(()=>getSelectedNodes.value.map(n=>n.id),ids=>{orderedSelection.value=selectionOrder(selectionGesture.value?[]:orderedSelection.value,ids,project.value?.graph.nodes??[])})
watch(()=>project.value?.id,()=>orderedSelection.value=[])
const matchPlan=computed(()=>{
  const ids=nodeMenu.value?.ids??orderedSelection.value
  if(ids.length!==2||!project.value)return null
  const order=selectionOrder(orderedSelection.value.filter(id=>ids.includes(id)),ids,project.value.graph.nodes)
  const source=project.value.graph.nodes.find(n=>n.id===order[0]),target=project.value.graph.nodes.find(n=>n.id===order[1])
  if(!source||!target)return null
  const pairs=matchingPorts(source,target,project.value.graph.nodes,project.value.graph.edges,descriptors.value)
  return pairs.length?{source:source.id,target:target.id,pairs}:null
})
async function connectSelected(){
  const plan=matchPlan.value;if(!plan||!project.value||!graphEditable.value)return
  nodeMenu.value=null
  const next=clone(project.value)
  next.graph.edges.push(...plan.pairs.map(pair=>({id:newId(),source:plan.source,target:plan.target,...pair})))
  await saveProject(next)
}

const graphCanvas = ref<HTMLElement>()
useCanvasTouch(graphCanvas)
useTouchScrollContainment()
const { start: libraryTouchStart, preview: libraryTouchPreview } = useLibraryTouch({
  context: () => `${project.value?.id}:${graphParent.value}:${graphEditable.value}:${tab.value}:${stage.value}`,
  insert: (item: LibraryItem, point) => {
    if (!graphEditable.value) return
    const placement = point ? screenToFlowCoordinate(point) : undefined
    if ('sample' in item) {
      const sample=projectSamples.value.find(s=>s.id===item.sample)
      if(sample){if(point)dropSample(sample,point);else samplePanel.value?.edit(sample)}
    } else if ('kind' in item) {
      const descriptor = catalog.value.find(d => d.kind === item.kind)
      if (descriptor) addNode(descriptor, placement)
    } else void task(() => importSubgraph(item.library, item.version, placement))
  },
})
let socket: WebSocket | null = null, reconnect: ReturnType<typeof setTimeout> | undefined, ping: ReturnType<typeof setInterval> | undefined
const localMidi=createLocalMidiInput(message=>{
  if(socket?.readyState!==WebSocket.OPEN||socket.bufferedAmount>65536)return false
  socket.send(JSON.stringify(message));return true
})
provide('localMidi',localMidi)

watch(()=>project.value?.graph.nodes, nodes=>{
  for(const node of Object.keys(localMidi.states))if(!nodes?.some(n=>n.id===node&&n.kind==='local_midi_input'))localMidi.disconnect(node)
})
onBeforeUnmount(()=>localMidi.dispose())

let lastEngineStatus = -Infinity
let frame = 0, lastSequence = -1, lastEpoch = '', offset = 0, bestRtt = Infinity
let saveTimer: ReturnType<typeof setTimeout> | undefined
let queuedParameters = new Map<string, { node: string; parameter: string; value: number }>()
let parameterFlush = false
const invitation = new URLSearchParams(location.search).get('invite')
const performanceLocked = computed(() => !!project.value && performanceId.value === project.value.id)
const editable = computed(() => !performanceLocked.value && ['owner', 'editor', 'conductor'].includes(role.value))
const conductor = computed(() => project.value?.conductor
  ? project.value.conductor === user.value?.id || role.value === 'owner'
  : ['owner', 'conductor'].includes(role.value))
const conductorStage = computed(() => project.value?.mode === 'conducted' && (project.value.conductor
  ? project.value.conductor === user.value?.id
  : ['owner', 'conductor'].includes(role.value)))
const transportBusy = ref(false), parameterPending = ref(false)
const countIn = ref('bar')
const countInOptions = new Set(['0', 'bar', ...Array.from({ length: 32 }, (_, i) => String(i + 1))])
watch(() => project.value?.id, (id) => {
  countIn.value = 'bar'
  if (!id) return
  try {
    const saved = localStorage.getItem(`pr0former.count-in.${id}`)
    if (saved && countInOptions.has(saved)) countIn.value = saved
  } catch { /* storage may be unavailable in private/browser test contexts */ }
}, { immediate: true })
watch([() => project.value?.id, countIn], ([id, value]) => {
  if (!id || !countInOptions.has(value)) return
  try { localStorage.setItem(`pr0former.count-in.${id}`, value) } catch { /* ignore unavailable storage */ }
})
watch(()=>project.value?.id,async id=>{projectSamples.value=[];if(id){try{const samples=await api<SampleEntry[]>(`/projects/${id}/samples`);if(project.value?.id===id)projectSamples.value=samples}catch(e){report(e)}}})
const countingIn = computed(() => active.value && !stale.value && telemetry.value?.count_in_remaining != null)
const graphEditable = computed(() => editable.value && !saving.value && !parameterPending.value && !progress.value)
const graphActive = computed(() => !!project.value && graphId.value === project.value.id)
watch([()=>project.value?.id,graphActive,()=>role.value,connected],()=>localMidi.stop())
const active = computed(() => !!project.value && activeId.value === project.value.id)
const stale = computed(() => !connected.value || now.value - receivedAt.value > 500)
const running = computed(() => active.value && !!telemetry.value?.running && !stale.value)
const metronomeOn = computed(() => graphActive.value && !stale.value && !!telemetry.value?.metronome)
async function toggleMetronome() { if (!project.value || !conductor.value) return; await api(`/projects/${project.value.id}/transport`, 'POST', { action: 'metronome', enabled: !metronomeOn.value }) }
const presentationClock = new PresentationClock()
const beat = computed(() => presentationClock.read(telemetry.value, active.value, stale.value, now.value + offset))
const meterBeat = computed(() => beat.value * (project.value?.beat_unit || 4) / 4)
const graphParent = ref<string | null>(null)
/** Remembered pan/zoom for the current project and subgraph; VueFlow mounts straight into it instead of refitting. */
const graphView = ref<GraphViewport | null>(null)
watch([() => project.value?.id, graphParent], ([id, parent]) => {
  graphView.value = id ? graphViewport(readView(graphViewKey(id)), parent) : null
}, { immediate: true, flush: 'sync' })
onViewportChangeEnd((v) => {
  const id = project.value?.id
  if (!id || ![v.x, v.y, v.zoom].every(Number.isFinite)) return
  const next = { x: v.x, y: v.y, zoom: v.zoom }
  graphView.value = next
  writeView(graphViewKey(id), { ...(readView<Record<string, GraphViewport>>(graphViewKey(id)) || {}), [graphParent.value || 'root']: next })
})
const visibleNodes = computed(() => project.value?.graph.nodes.filter(n => (n.parent || null) === graphParent.value) || [])
const visibleEdges = computed(() => project.value?.graph.edges.filter(e => visibleNodes.value.some(n => n.id === e.source)) || [])
const breadcrumbs = computed(() => {
  const path: GraphNode[] = []; let id = graphParent.value
  while (id) { const node = project.value?.graph.nodes.find(n => n.id === id); if (!node || path.includes(node)) break; path.unshift(node); id = node.parent || null }
  return path
})
watch(() => project.value?.graph.nodes, nodes => { if (graphParent.value && !nodes?.some(n=>n.id===graphParent.value)) void navigateGraph(null) })
async function navigateGraph(id: string | null) {
  graphParent.value = id; selectedNode.value = null; selectedEdges.value = []; nodeMenu.value = null; pendingPort.value = null
  await nextTick(); if (!graphView.value) fitView({padding:0.2,maxZoom:phoneCompact.value ? 0.5 : 1})
}
function openNode(id: string) {
  const node = project.value?.graph.nodes.find(n=>n.id===id)
  if (node?.kind === 'subgraph') void navigateGraph(id); else selectedNode.value = id
}
const selected = computed(() => project.value?.graph.nodes.find(n => n.id === selectedNode.value))
const selectedDescriptor = computed(() => selected.value && project.value ? nodeDescriptor(selected.value, project.value.graph.nodes, descriptors.value) : undefined)
const part = computed(() => project.value?.parts.find(p => p.id === selectedPart.value) || project.value?.parts[0])
const partPlayback = computed(() => active.value ? telemetry.value?.parts?.find(p => p.id === part.value?.id) : undefined)
watch(() => telemetry.value?.parts, states => {
  if (!stage.value || conductorStage.value || !states || !project.value || !user.value) return
  const assigned = project.value.parts.filter(part => part.performer === user.value!.id)
  const next = assigned.find(part => states.find(state => state.id === part.id)?.playing)
    || assigned.find(part => states.find(state => state.id === part.id)?.count_in_remaining)
    || assigned.find(part => states.find(state => state.id === part.id)?.pending?.[1])
  if (next) selectedPart.value = next.id
})
const partBeat = computed(() => {
  const state = partPlayback.value
  if (!state || !part.value) return 0
  if (stale.value) return state.position
  const pending = state.pending
  const applied = pending && beat.value >= pending[0]
  const playing = applied ? pending[1] : state.playing
  const start = applied ? pending[0] : state.start
  return playing ? Math.max(0, beat.value - start) % part.value.loop_beats : 0
})
const scoreBeats = computed(() => Object.fromEntries((project.value?.parts||[]).map(p=>{
  const state=active.value?telemetry.value?.parts?.find(s=>s.id===p.id):undefined
  if(!state)return [p.id,0]
  if(stale.value)return [p.id,state.position]
  if(project.value?.score)return [p.id,state.playing&&running.value?Math.min(state.position_end??Infinity,state.position+Math.max(0,beat.value-(telemetry.value?.beat??beat.value))):state.position]
  const applied=state.pending&&beat.value>=state.pending[0]
  const playing=applied?state.pending![1]:state.playing,start=applied?state.pending![0]:state.start
  return [p.id,playing?Math.max(0,beat.value-start)%p.loop_beats:state.position]
})))
const initialMeter=computed(()=>project.value?.score?.meters.find(m=>m.beat===0)||{beats:project.value?.beats_per_bar||4,unit:project.value?.beat_unit||4})
const scorePosition=computed(()=>{
 const p=project.value
 if(!p?.score)return {bar:Math.floor(meterBeat.value/(p?.beats_per_bar||4))+1,beat:Math.floor(meterBeat.value%(p?.beats_per_bar||4))+1,beats:p?.beats_per_bar||4}
 const position=p.mode==='structured'?(scoreBeats.value[p.parts[0]?.id||'']||0):beat.value
 const measures=scoreMeasures(Math.max(p.score.length,position+1),p.beats_per_bar,p.beat_unit||4,p.score.meters),m=measures.find(m=>position<m.end)||measures.at(-1)
 return m?{bar:m.number,beat:Math.floor((position-m.start)*m.unit/4)+1,beats:m.beats}:{bar:1,beat:1,beats:p.beats_per_bar}
})
const canLaunchPart = computed(() => conductor.value || (project.value?.mode === 'freeform' && part.value?.performer === user.value?.id))
const partStatus = computed(() => {
  const state = partPlayback.value
  if (!active.value || !state) return 'Part idle'
  if (stale.value) return 'Part timing unavailable'
  if (state.pending) return `${state.pending[1] ? 'Launch' : 'Stop'} queued · beat ${state.pending[0] + 1}`
  return state.playing ? (running.value ? 'Part playing' : 'Part armed / paused') : 'Part idle'
})
const categories = computed(() => ['All nodes', ...new Set(descriptors.value.map(d => d.category))])
const catalog = computed(() => descriptors.value.filter(d => (!d.kind.startsWith('subgraph_') || !!graphParent.value) && (category.value === 'All nodes' || d.category === category.value) && `${d.label} ${d.kind} ${d.aliases.join(' ')}`.toLowerCase().includes(search.value.toLowerCase())))
const flowNodes = computed<FlowNode[]>(() => visibleNodes.value.map(n => ({ id: n.id, type: 'instrument', position: { x: n.x, y: n.y }, draggable: graphEditable.value, data: { previewPart:(id:string)=>partPlayerPreview.value=id,partName:project.value!.parts.find(p=>p.id===n.part_id)?.name,parameter:queueNodeParameter,projectId:project.value!.id,controller:controllerGesture.bind(null,project.value!.id),canMute:canMuteInput(n.id),connected:project.value!.graph.edges.filter(e=>e.target===n.id).map(e=>e.target_port),piano:pianoNote,active:graphActive.value,editable:editable.value&&!progress.value,driven:project.value!.graph.edges.some(e=>e.target===n.id&&e.target_port==='in'),setControl:graphControl,previewControl:previewGraphControl,bang:bangControl,node: n, descriptor: nodeDescriptor(n, project.value!.graph.nodes, descriptors.value), open: openNode, edit: (id: string) => selectedNode.value = id, connectPort, toggleSelection, contextMenu: openNodeMenu } })))
const flowEdges = computed<FlowEdge[]>(() => visibleEdges.value.filter(e=>!inputDrag.hidden.value.has(e.id)).map(e => {
  const source = project.value!.graph.nodes.find(n => n.id === e.source)
  const d = source ? nodeDescriptor(source, project.value!.graph.nodes, descriptors.value) : undefined
  // Edge props carry only structure; edges read live telemetry through the
  // injected shallow reference so the edge list is not rebuilt at 20 Hz.
  return { id: e.id, source: e.source, target: e.target, sourceHandle: e.source_port, targetHandle: e.target_port, type: 'signal', data: { projectId: project.value!.id, signal: d?.outputs.find(p => p.id === e.source_port)?.signal || 'control', channels: d?.outputs.find(p => p.id === e.source_port)?.fixed_channels || source?.channels || 1 } }
}))
provide('telemetry', telemetry)
provide('telemetryStale',stale)
provide('graphActive',graphActive)
const pendingPort = ref<{ node: string; port: string; direction: string } | null>(null)

function clone<T>(value: T): T { return JSON.parse(JSON.stringify(value)) }
function report(e: unknown) { error.value = e instanceof Error ? e.message : String(e) }
async function task(fn: () => Promise<void>) { error.value = ''; try { await fn() } catch (e) { report(e) } }
async function refresh() { summaries.value = await api<Summary[]>('/projects') }
async function refreshMembers() { if (project.value) members.value = await api<Member[]>(`/projects/${project.value.id}/members`) }
function acceptEnsembleProject(saved:Project) { if(project.value?.id===saved.id&&saved.revision>=project.value.revision)project.value=saved }
async function authenticate() {
  busy.value = true
  await task(async () => {
    user.value = await api('/' + (bootstrap.value || registering.value ? 'register' : 'login'), 'POST', { username: username.value, password: password.value, invite: invitation })
    randomizeHero()
    password.value = ''
    if (invitation) { const joined = await api<{ project_id: string }>('/join', 'POST', { token: invitation }); await refresh(); await openProject(joined.project_id); history.replaceState({}, '', '/') }
    else { await refresh(); if (requestedDesktopProject) await openProject(requestedDesktopProject); else if (summaries.value[0]) await openProject((recentSummaries.value[0]||summaries.value[0]).id); else creating.value = true }
  }); busy.value = false
}
async function openProject(id: string) {
  tempoEditing.value=false;pendingTempo.value=null
  await scoreDraft.value?.flush()
  await withProgress('Loading project and preparing clips', async()=>{
  const result = await api<{ project: Project; role: string }>(`/projects/${id}`)
  await api(`/projects/${id}/opened`,'POST');await refresh();browserOpen.value=false
  graphParent.value = null; project.value = result.project; role.value = result.role; bpmDraft.value = result.project.bpm
  settingsOpen.value = false; stage.value = false; selectedPart.value = result.project.parts.find(p => p.performer === user.value?.id)?.id || result.project.parts[0]?.id || ''; selectedNode.value = null; projectPicker.value = false; telemetry.value = null; receivedAt.value = 0; undo.value = []
  members.value = await api<Member[]>(`/projects/${id}/members`); connect(id)
  await refreshAudio(); systemOpen.value=false; consoleOpen.value=false
  if (id === requestedDesktopProject) {
    tab.value = requestedDesktopView === 'stage' ? 'graph' : requestedDesktopView
    stage.value = requestedDesktopView === 'stage'
  }
  setTimeout(() => { if (graphView.value) void setViewport(graphView.value); else fitView({ padding: 0.18, duration: 300, maxZoom:phoneCompact.value ? 0.5 : 1 }) }, 100)
  })
}
function visualizerSubscription(){if(socket?.readyState===WebSocket.OPEN)socket.send(JSON.stringify({type:'visualizers',enabled:graphActive.value && !stage.value && (tab.value==='graph'||!!selected.value?.kind.endsWith('_visualizer'))}))}
function performanceMidi(message: Record<string, unknown>) {
  if (socket?.readyState !== WebSocket.OPEN) return false
  socket.send(JSON.stringify(message)); return true
}
const conductorMidi=createConductorMidi(performanceMidi)
provide(conductorMidiKey,conductorMidi)
const suppliesConductorMidi=computed(()=>!!project.value && (project.value.conductor ? project.value.conductor===user.value?.id : role.value==='owner'))
const canBindConductorMidi=computed(()=>conductor.value||role.value==='editor')
watch([()=>project.value?.id,()=>project.value?.mode,suppliesConductorMidi,connected],()=>conductorMidi.context(project.value?.mode==='conducted'?project.value.id:'',suppliesConductorMidi.value,connected.value))
onBeforeUnmount(()=>conductorMidi.dispose())
watch([tab,stage,selectedNode,graphActive],visualizerSubscription)
function offline() { connected.value = false; clearInterval(ping); clearTimeout(reconnect); socket?.close() }
function online() { if (user.value && project.value) connect(project.value.id) }
function connect(id: string) {
  if (socket) { socket.onclose = null; socket.close() }
  clearTimeout(reconnect); clearInterval(ping); connected.value = false; lastEngineStatus = -Infinity; lastSequence = -1; bestRtt = Infinity
  const currentSocket = new WebSocket(`${location.protocol === 'https:' ? 'wss:' : 'ws:'}//${location.host}/api/projects/${id}/events`)
  socket = currentSocket
  currentSocket.onopen = () => { if (socket !== currentSocket) return; connected.value = true; visualizerSubscription(); ping = setInterval(() => socket?.readyState === WebSocket.OPEN && socket.send(JSON.stringify({ type: 'ping', client_time: performance.now() })), 1000) }
  currentSocket.onmessage = event => {
    if (socket !== currentSocket || project.value?.id !== id) return
    const message = JSON.parse(event.data)
    if(message.type==='session_revoked'){
      error.value='Your session was ended by an administrator. Sign in again to continue.'
      user.value=null;project.value=null;systemOpen.value=browserOpen.value=quickNodes.value=false
      clearTimeout(reconnect);clearInterval(ping);currentSocket.close();return
    }
    conductorMidi.receive(message)
    if(message.type==='local_midi_status')localMidi.receive(message)
    if(message.type==='piano_error'||message.type==='control_error')report(new Error(message.error))
    if(message.type==='samples')void samplePanel.value?.refresh()
    if(message.type==='hardware_levels'){hardwareLevels.value=message;hardwareReceived.value=performance.now()}
    if(message.type==='project_save') acceptSaveStatus(id, message.save)
    if(message.type==='members_changed') void refreshMembers().catch(report)
    if(message.type==='audio_engine_status'){engineEnabled.value=message.enabled;audioSettings.value.sample_rate=message.sample_rate;audioSettings.value.block_size=message.block_size}
    if(message.type==='system_audio'){audioSettings.value=message.settings}
    if (message.type === 'pong') { const rtt = performance.now() - message.client_time; if (rtt < bestRtt) { bestRtt = rtt; offset = message.server_time - (message.client_time + rtt / 2) } }
    if (message.type === 'engine_status' && message.server_time >= lastEngineStatus) {
      lastEngineStatus = message.server_time
      performanceId.value = message.performance_project ?? null
      activeId.value = message.active_project
      graphId.value = message.graph_project ?? null
      if (message.graph_project !== id) { telemetry.value = null; receivedAt.value = 0 }
    }
    if (message.type === 'telemetry') {
      if (graphId.value !== id) return
      if (message.epoch === lastEpoch && message.sequence <= lastSequence) return
      lastEpoch = message.epoch; lastSequence = message.sequence
      if (message.revision < (project.value?.revision || 0)) return
      engineEnabled.value=message.engine_enabled ?? true
      telemetry.value = message; receivedAt.value = performance.now()
      if (bestRtt === Infinity) offset = message.server_time - performance.now()
      if (pendingTempo.value !== null && Math.abs(message.bpm - pendingTempo.value) < 0.000001) pendingTempo.value = null
      if (!tempoEditing.value && pendingTempo.value === null) bpmDraft.value = message.bpm
    }
    if (message.type === 'project' && message.project.revision > (project.value?.revision || 0)) project.value = message.project
    if (message.type === 'resync_required') void task(async () => {
      const latest = await api<{ project: Project }>(`/projects/${id}`)
      if (project.value?.id === id && latest.project.revision > project.value.revision) project.value = latest.project
    })
  }
  currentSocket.onclose = () => { if (socket !== currentSocket) return; connected.value = false; clearInterval(ping); reconnect = setTimeout(() => { if (navigator.onLine && user.value && project.value?.id === id) connect(id) }, 1500) }
}
async function createProject() { busy.value = true; await task(async () => { const p = await api<Project>('/projects', 'POST', { name: newName.value, mode: newMode.value }); creating.value = false; await refresh(); await openProject(p.id) }); busy.value = false }
function remember() { if (project.value) undo.value = [...undo.value.slice(-49), clone(project.value)] }
async function saveProject(next: Project, record = true) {
  if(next.conducted?.midi_bindings)next.conducted.midi_bindings=next.conducted.midi_bindings.filter(b=>b.action==='part'||b.action==='arm'?next.parts.some(p=>p.id===b.target):b.action==='set'?next.conducted!.sets.some(s=>s.id===b.target):true)
  if(next.local_audio_assignments)next.local_audio_assignments=Object.fromEntries(Object.entries(next.local_audio_assignments).filter(([id])=>next.graph.nodes.some(n=>n.id===id&&n.kind==='browser_input')))
  if (!project.value || saving.value) return
  const previous = clone(project.value)
  saving.value = true
  try { const saved = await api<Project>(`/projects/${next.id}`, 'PUT', next); if (record) undo.value = [...undo.value.slice(-49), previous]; if (project.value?.id === saved.id && saved.revision >= project.value.revision) project.value = saved }
  catch (e) { saveRequested.value = null; report(e); const latest = await api<{ project: Project }>(`/projects/${next.id}`); if (project.value?.id === latest.project.id && latest.project.revision >= project.value.revision) project.value = latest.project }
  finally { saving.value = false }
}
async function applyScript(script:import('./types').ScriptConfig,id:string,projectId:string) {
  if(!project.value||project.value.id!==projectId||!graphEditable.value||saving.value)throw new Error('Script editing is currently unavailable.')
  const previous=clone(project.value),next=clone(project.value)
  const node=next.graph.nodes.find(n=>n.id===id)
  if(!node||node.kind!=='js_control')throw new Error('The script node no longer exists.')
  node.script={...script,revision:(node.script?.revision??0)+1}
  const inputs=new Set(['midi',...script.inputs.map(p=>p.name)]),outputs=new Set(['midi',...script.outputs.map(p=>p.name)])
  next.graph.edges=next.graph.edges.filter(e=>(e.target!==id||inputs.has(e.target_port))&&(e.source!==id||outputs.has(e.source_port)))
  saving.value=true
  try{
    const saved=await api<Project>(`/projects/${next.id}`,'PUT',next)
    undo.value=[...undo.value.slice(-49),previous]
    if(project.value?.id===saved.id&&saved.revision>=project.value.revision)project.value=saved
  }finally{saving.value=false}
}
async function saveScoreProject(next:Project) {
  if (saving.value) await new Promise<void>(resolve => { const stop = watch(saving, value => { if (!value) { stop(); resolve() } }) })
  if(!project.value||stage.value||!editable.value)throw new Error('Score editing is currently unavailable.')
  saving.value=true
  try {
    const saved=await api<Project>(`/projects/${next.id}`, 'PUT',next)
    if(project.value?.id===saved.id&&saved.revision>=project.value.revision)project.value=saved
  } catch(error) {
    const latest=await api<{project:Project}>(`/projects/${next.id}`)
    if(project.value?.id===latest.project.id&&latest.project.revision>=project.value.revision)project.value=latest.project
    throw error
  } finally {saving.value=false}
}
function addNode(d: Descriptor, placement?: {x:number;y:number}, sample?:SampleEntry) {
  if (!project.value || !graphEditable.value) return
  const rect = document.querySelector('.graph-canvas')?.getBoundingClientRect()
  const pos = placement || screenToFlowCoordinate({ x: (rect?.left || 0) + (rect?.width || 900) / 2, y: (rect?.top || 0) + (rect?.height || 600) / 2 })
  const n: GraphNode = { parent: graphParent.value, id: newId(), kind: d.kind, label: d.label, x: pos.x, y: pos.y, channels: d.default_channels ?? 2, parameters: Object.fromEntries(d.parameters.map(p => [p.id, p.default])) }
  if(sample){n.channels=sample.channels;n.parameters.asset=sample.asset!;n.label=sample.name}
  const next = clone(project.value); next.graph.nodes.push(n); void task(() => saveProject(next))
}
function addSampleNode(sample:SampleEntry){const descriptor=descriptors.value.find(d=>d.kind==='poly_sampler');if(descriptor)addNode(descriptor,undefined,sample)}
async function ensureProjectSample(id:string):Promise<SampleEntry>{
  const existing=projectSamples.value.find(s=>s.id===id)
  if(existing)return existing
  if(!project.value)throw new Error('Open a project first')
  const sample=await api<SampleEntry>(`/projects/${project.value.id}/samples/${id}/add`,'POST')
  await samplePanel.value?.refresh();return sample
}
function quickInsert(item:LibraryItem,point?:{x:number;y:number}){
  quickNodes.value=false
  void task(async()=>{
    if(!graphEditable.value)return
    const pos=point?screenToFlowCoordinate(point):undefined
    if('sample' in item){const sample=await ensureProjectSample(item.sample);if(point)dropSample(sample,point);else addSampleNode(sample)}
    else if('library' in item)await importSubgraph(item.library,item.version,pos)
    else {const d=descriptors.value.find(d=>d.kind===item.kind);if(d)addNode(d,pos)}
  })
}
function dropSample(sample:SampleEntry,point:{x:number;y:number}){
  if(!project.value||!graphEditable.value||sample.asset===null)return
  const hit=document.elementFromPoint(point.x,point.y)?.closest('.vue-flow__node'),id=hit?.getAttribute('data-id')
  if(id){
    const next=clone(project.value),node=next.graph.nodes.find(n=>n.id===id)
    if(!node||!nodeDescriptor(node,next.graph.nodes,descriptors.value)?.parameters.some(p=>p.id==='asset')){report(new Error('This node does not accept samples'));return}
    node.parameters.asset=sample.asset;node.channels=sample.channels
    void task(()=>saveProject(next))
  }else{
    const descriptor=descriptors.value.find(d=>d.kind==='poly_sampler')
    if(descriptor)addNode(descriptor,screenToFlowCoordinate(point),sample)
  }
}
function updateSampleChoices(choices: import('./types').SampleChoice[]) {
  if (!project.value || !selected.value || !graphEditable.value) return
  const next = clone(project.value)
  next.graph.nodes.find(n => n.id === selected.value!.id)!.sample_choices = choices
  void task(() => saveProject(next))
}
function assignSample(sample:SampleEntry){if(!project.value||!selected.value)return;const next=clone(project.value),node=next.graph.nodes.find(n=>n.id===selected.value!.id)!;node.parameters.asset=sample.asset!;if(node.kind!=='convolution_reverb')node.channels=sample.channels;void task(()=>saveProject(next))}
function assignNodeIo(io: IoConfig) {
  if (!project.value || !selected.value || !graphEditable.value) return
  const next = clone(project.value), node = next.graph.nodes.find(n => n.id === selected.value!.id)
  if (node) { node.io = io; void task(() => saveProject(next)) }
}
function assignNodePart(id: string) {
  if (!project.value || !selected.value || !graphEditable.value) return
  const next = clone(project.value), node = next.graph.nodes.find(n => n.id === selected.value!.id)
  if (node && ['part_midi','part_player','monitor_output'].includes(node.kind)) { node.part_id = id || null; void task(() => saveProject(next)) }
}
function libraryDrag(event: DragEvent, kind: string) {
  if (!graphEditable.value) return
  event.dataTransfer?.setData('application/pr0former',JSON.stringify({kind})); if(event.dataTransfer) event.dataTransfer.effectAllowed='copy'
}
async function importSubgraph(id: string, version: number, placement?: {x:number;y:number}) {
  if (!project.value || !graphEditable.value) return
  const rect=document.querySelector('.graph-canvas')!.getBoundingClientRect(), pos=placement||screenToFlowCoordinate({x:rect.left+rect.width/2,y:rect.top+rect.height/2})
  const previous=clone(project.value); saving.value=true
  try {
    const saved=await api<Project>(`/projects/${previous.id}/subgraphs/insert`,'POST',{library_id:id,version,revision:previous.revision,parent:graphParent.value,...pos})
    undo.value=[...undo.value.slice(-49),previous]
    if(project.value?.id===saved.id&&project.value.revision<=saved.revision)project.value=saved
  } finally {saving.value=false}
}
function libraryDrop(event: DragEvent) {
  if (!graphEditable.value) return
  const text=event.dataTransfer?.getData('application/pr0former'); if(!text)return
  let data: LibraryItem
  try {
    const parsed = JSON.parse(text)
    if (!parsed || typeof parsed !== 'object') throw new Error()
    if (typeof parsed.sample === 'string') data = { sample: parsed.sample }
    else if (typeof parsed.kind === 'string') data = { kind: parsed.kind }
    else if (typeof parsed.library === 'string' && Number.isInteger(parsed.version)) data = { library: parsed.library, version: parsed.version }
    else throw new Error()
  } catch { report(new Error('Invalid library drag data')); return }
  void task(async () => {
    const pos = screenToFlowCoordinate({ x: event.clientX, y: event.clientY })
    if ('sample' in data) {
      const sample=await ensureProjectSample(data.sample)
      dropSample(sample,{x:event.clientX,y:event.clientY})
    } else if ('kind' in data) {
      const descriptor = descriptors.value.find(d => d.kind === data.kind)
      if (!descriptor) throw new Error('This node is no longer available in the library')
      addNode(descriptor, pos)
    } else await importSubgraph(data.library, data.version, pos)
  })
}
async function librarySaved(reference: {id:string;version:number}) {
  const node=savingSubgraph.value;savingSubgraph.value=null
  nodeLibraryOpen.value=false
  sampleLibraryOpen.value=false
  await libraryPanel.value?.refresh()
  if(project.value&&node&&graphEditable.value){const next=clone(project.value),target=next.graph.nodes.find(n=>n.id===node);if(target){target.library=reference;await saveProject(next)}}
}
const inputDrag=useInputConnectionDrag(graphCanvas,{
  editable:()=>graphEditable.value&&!saving.value,
  context:()=>`${project.value?.id}:${graphParent.value}`,
  edges:()=>visibleEdges.value,
  tap:(node,port)=>connectPort(node,port,'target'),
  drop:(ids,target)=>{
    pendingPort.value=null
    if(!project.value)return
    const next=clone(project.value),moving=new Set(ids)
    if(target){
      for(const edge of next.graph.edges)if(moving.has(edge.id)){edge.target=target.node;edge.target_port=target.port}
      if(JSON.stringify(next.graph.edges)===JSON.stringify(project.value.graph.edges))return
    }else next.graph.edges=next.graph.edges.filter(e=>!moving.has(e.id))
    void task(()=>saveProject(next))
  },
})
function onConnect(c: Connection) {
  if (!project.value || !graphEditable.value) return
  const sourcePort = c.sourceHandle || 'out', targetPort = c.targetHandle || 'in'
  const next = clone(project.value)
  if (next.graph.edges.some(e => e.source === c.source && e.source_port === sourcePort && e.target === c.target && e.target_port === targetPort)) {
    pendingPort.value = null
    return
  }
  const source = next.graph.nodes.find(n => n.id === c.source), target = next.graph.nodes.find(n => n.id === c.target)
  const sourceDescriptor = source && nodeDescriptor(source, next.graph.nodes, descriptors.value)
  const targetDescriptor = target && nodeDescriptor(target, next.graph.nodes, descriptors.value)
  const port = targetDescriptor?.inputs.find(p => p.id === targetPort)
  const sourcePortDescriptor = sourceDescriptor?.outputs.find(p => p.id === sourcePort)
  const existing = next.graph.edges.find(e => e.target === c.target && e.target_port === targetPort)
  const widthMatches = sourcePortDescriptor?.signal === 'audio' && source && target && (sourcePortDescriptor.fixed_channels ?? source.channels) === (port?.fixed_channels ?? target.channels)
  if (source && target && port?.signal === 'audio' && widthMatches && existing) {
    const mixerId = newId()
    const mixer = { id: mixerId, kind: 'mixer', label: 'Convenience mixer', parent: target.parent || null, x: (source.x + target.x) / 2, y: (source.y + target.y) / 2 + 40, channels: target.channels, parameters: { gain: -6 } }
    next.graph.nodes.push(mixer)
    existing.target = mixerId; existing.target_port = 'a'
    next.graph.edges.push(
      { id: newId(), source: c.source, source_port: sourcePort, target: mixerId, target_port: 'b' },
      { id: newId(), source: mixerId, source_port: 'out', target: c.target, target_port: targetPort },
    )
  } else {
    next.graph.edges.push({ id: newId(), source: c.source, source_port: sourcePort, target: c.target, target_port: targetPort })
  }
  void task(() => saveProject(next)); pendingPort.value = null
}
function connectPort(node: string, port: string, direction: string) {
  if (!graphEditable.value) return
  const p = pendingPort.value
  if (!p || p.direction === direction) { pendingPort.value = { node, port, direction }; return }
  const source = direction === 'source' ? { node, port } : p, target = direction === 'target' ? { node, port } : p
  onConnect({ source: source.node, sourceHandle: source.port, target: target.node, targetHandle: target.port })
}
function nodeDrag({ node, nodes }: { node: FlowNode; nodes: FlowNode[] }) {
  if (!project.value || !graphEditable.value) return
  const moved=nodes?.length?nodes:[node], next=clone(project.value)
  for(const dragged of moved){const target=next.graph.nodes.find(n=>n.id===dragged.id);if(target){target.x=dragged.position.x;target.y=dragged.position.y}}
  if(moved.some(d=>{const old=project.value!.graph.nodes.find(n=>n.id===d.id);return old&&(old.x!==d.position.x||old.y!==d.position.y)}))void task(()=>saveProject(next))
}
function editParameter(key: string, value: number) {
  if (!selected.value || !project.value || (!editable.value && !(selected.value.kind==='browser_input'&&key==='mute'&&canMuteInput(selected.value.id)))) return
  if (selectedDescriptor.value?.parameters.find(p => p.id === key)?.structural) {
    if (!graphEditable.value) return
    const next = clone(project.value)
    const node=next.graph.nodes.find(n => n.id === selected.value!.id)!
    node.parameters[key] = value
    if(['knobs','sliders'].includes(node.kind)&&key==='count')next.graph.edges=next.graph.edges.filter(e=>!(e.source===node.id&&e.source_port!=='midi'&&Number(e.source_port.split('_').at(-1))>value)&&!(e.target===node.id&&e.target_port!=='midi'&&Number(e.target_port.split('_').at(-1))>value))
    if(node.kind==='pitch_tracker'&&key==='slots')next.graph.edges=next.graph.edges.filter(e=>e.source!==node.id||!/^pitch[1-4]$/.test(e.source_port)||Number(e.source_port.slice(5))<=value)
    if(node.kind==='control_input') {
      const mode=node.parameters.mode??2,min=node.parameters.min??-100000,max=node.parameters.max??100000
      node.control_value=mode===4?(typeof node.control_value==='string'?node.control_value:''):mode===0?0:Math.max(min,Math.min(max,mode===1?Math.round(Number(node.control_value)||0):Number(node.control_value)||0))
    }
    void task(() => saveProject(next))
    return
  }
  queueNodeParameter(selected.value.id,key,value)
}
function canMuteInput(node:string){return !!user.value && project.value?.local_audio_assignments?.[node]===user.value.id}
function queueNodeParameter(node:string, key:string, value:number) {
  const ownMute=key==='mute'&&project.value?.graph.nodes.some(n=>n.id===node&&n.kind==='browser_input')&&canMuteInput(node)
  if ((!editable.value&&!ownMute) || !project.value || project.value.graph.edges.some(e=>e.target===node && e.target_port===key)) return
  if (saving.value && !parameterFlush) return
  if (!queuedParameters.size && !parameterFlush) remember()
  parameterPending.value = true
  queuedParameters.set(`${node}/${key}`, { node, parameter: key, value })
  clearTimeout(saveTimer); saveTimer = setTimeout(flushParameters, 60)
}
async function flushParameters() {
  if (parameterFlush || !project.value) return
  parameterFlush = true; saving.value = true
  try { while (queuedParameters.size && project.value) { const [id, value] = queuedParameters.entries().next().value!; queuedParameters.delete(id); const saved: Project = await api<Project>(`/projects/${project.value.id}/parameter`, 'PUT', { ...value, revision: project.value.revision }); if (project.value?.id === saved.id && saved.revision >= project.value.revision) project.value = saved } }
  catch (e) { saveRequested.value = null; queuedParameters.clear(); report(e); if (project.value) { const p = await api<{ project: Project }>(`/projects/${project.value.id}`); if (project.value?.id === p.project.id && p.project.revision >= project.value.revision) project.value = p.project } }
  finally { parameterFlush = false; saving.value = false; parameterPending.value = false }
}
async function undoEdit() {
  if (!graphEditable.value || !project.value) return
  const previous = undo.value.pop(); if (!previous) return
  previous.revision = project.value.revision
  await saveProject(previous, false)
}
function removeEdges(ids: string[], nodeIds: string[] = []) {
  if (!project.value || !graphEditable.value) return
  const next = clone(project.value)
  nodeIds = [...descendants(next.graph.nodes, nodeIds)]
  for (const edge of next.graph.edges.filter(e => ids.includes(e.id) || nodeIds.includes(e.source))) {
    const node = next.graph.nodes.find(n => n.id === edge.target)
    const descriptor = descriptors.value.find(d => d.kind === node?.kind)
    const parameter = descriptor?.parameters.find(p => p.id === edge.target_port)
    const value = telemetry.value?.values[edge.target]?.[edge.target_port]
    if (node && parameter && value !== undefined && Number.isFinite(value)) node.parameters[parameter.id] = Math.max(parameter.min, Math.min(parameter.max, value))
  }
  next.graph.nodes = next.graph.nodes.filter(n => !nodeIds.includes(n.id))
  next.graph.edges = next.graph.edges.filter(e => !ids.includes(e.id) && !nodeIds.includes(e.source) && !nodeIds.includes(e.target) && !(nodeIds.includes(e.source_port) && project.value!.graph.nodes.find(n=>n.id===e.source)?.kind==='subgraph') && !(nodeIds.includes(e.target_port) && project.value!.graph.nodes.find(n=>n.id===e.target)?.kind==='subgraph'))
  for (const part of next.parts) {if (part.instrument_node && nodeIds.includes(part.instrument_node)) part.instrument_node = null;for(const staff of part.staves||[])if(staff.instrument_node&&nodeIds.includes(staff.instrument_node))staff.instrument_node=null}
  if (selectedNode.value && nodeIds.includes(selectedNode.value)) selectedNode.value = null
  selectedEdges.value = []
  void task(() => saveProject(next))
}
function disconnect(edge: GraphEdge) { removeEdges([edge.id]) }
async function uploadSample(file: File) { if (!project.value || !selected.value) return; const form = new FormData(); form.append('sample', file); const response = await fetch(`/api/projects/${project.value.id}/samples`, { method: 'POST', headers: { 'X-Pr0former': '1' }, body: form }); const result = await response.json(); if (!response.ok) throw new Error(result.error); const next = clone(project.value); const node = next.graph.nodes.find(n => n.id === selected.value!.id)!; node.parameters.asset = result.asset; node.channels = result.channels; await saveProject(next);await samplePanel.value?.refresh() }
const pendingControls=new Map<string,number|string>()
let controlsBusy=false
function graphControl(id:string,value:number|string){if(!editable.value)return;pendingControls.set(id,value);void flushControls()}
function previewGraphControl(node:string,value:number){
  if(!project.value||!editable.value||!graphActive.value||socket?.readyState!==WebSocket.OPEN||socket.bufferedAmount>65536)return
  socket.send(JSON.stringify({type:'graph_control',node,value,revision:project.value.revision}))
}
async function flushControls(){
  if(controlsBusy||saving.value||!project.value||!pendingControls.size)return
  controlsBusy=true;parameterPending.value=true;remember()
  try {
    while(pendingControls.size&&project.value){
      const [node,value]=pendingControls.entries().next().value!;pendingControls.delete(node)
      saving.value=true
      const saved:Project=await api<Project>(`/projects/${project.value.id}/control`,'PUT',{node,value,revision:project.value.revision})
      if(project.value?.id===saved.id&&saved.revision>=project.value.revision)project.value=saved
    }
  }catch(e){saveRequested.value=null;pendingControls.clear();report(e)}finally{saving.value=false;parameterPending.value=false;controlsBusy=false}
}
watch(saving,value=>{if(!value)void flushControls()})
watch([saving, parameterPending], () => { if (saveRequested.value) void flushManualSave() })
watch(()=>project.value?.id,()=>pendingControls.clear())
const controllerRequests = new Map<string,Promise<void>>()
const pendingControllerValues = new Map<string,Map<number,number>>()
const controllerValueFlushes = new Set<string>()
const learningControllers = new Map<string,number>()
function enqueueControllerRequest(key:string,action:()=>Promise<void>){
  const request=(controllerRequests.get(key)||Promise.resolve()).then(action).catch(report)
  controllerRequests.set(key,request)
  void request.finally(()=>{if(controllerRequests.get(key)===request)controllerRequests.delete(key)})
  return request
}
function queueControllerValue(id:string,node:string,index:number,value:number){
  const key=`${id}:${node}`
  const pending=pendingControllerValues.get(key)||new Map<number,number>()
  pending.set(index,value);pendingControllerValues.set(key,pending)
  if(controllerValueFlushes.has(key))return
  controllerValueFlushes.add(key)
  void enqueueControllerRequest(key,async()=>{
    try{
      while(pending.size){
        const values=[...pending];pending.clear()
        for(const [nextIndex,nextValue] of values)await api(`/projects/${id}/controller`,'PUT',{node,index:nextIndex,value:nextValue})
      }
    }finally{
      controllerValueFlushes.delete(key)
      pendingControllerValues.delete(key)
    }
  })
}
function controllerGesture(id:string,node:string,index:number,value:number|null|'cancel'|'clear') {
  if(value==='clear') {
    if(project.value?.id!==id||!graphEditable.value)return
    const key=`${id}:${node}`
    learningControllers.delete(key)
    void task(async()=>{
      await controllerRequests.get(key)
      if(project.value?.id!==id)return
      const next=clone(project.value),target=next.graph.nodes.find(n=>n.id===node)
      if(!target||target.kind!=='knobs'||index<0||index>=(target.parameters.count??4))return
      target.parameters[`channel_${index+1}`]=0
      await saveProject(next)
    })
    return
  }
  if(value!=='cancel'&&(!project.value||project.value.id!==id||!editable.value||!graphActive.value))return
  const key=`${id}:${node}`
  if(value===null)learningControllers.set(key,telemetry.value?.values[node]?._learn_serial??0);else learningControllers.delete(key)
  if(typeof value==='number'){queueControllerValue(id,node,index,value);return}
  void enqueueControllerRequest(key,async()=>{await api(`/projects/${id}/controller`,'PUT',{node,index,value:value==='cancel'?null:value,cancel:value==='cancel'})})
}
watch([telemetry,saving],()=>{
  if(!project.value||saving.value||!graphEditable.value)return
  for(const node of project.value.graph.nodes){
    const key=`${project.value.id}:${node.id}`,v=telemetry.value?.values[node.id]
    if(!learningControllers.has(key)||!v||v._learning||!v._learn_serial||v._learn_serial===learningControllers.get(key))continue
    learningControllers.delete(key)
    const next=clone(project.value),target=next.graph.nodes.find(n=>n.id===node.id)!
    target.parameters[`channel_${v._learn_index}`]=v._learn_channel!
    target.parameters[`controller_${v._learn_index}`]=v._learn_controller!
    void task(()=>saveProject(next));break
  }
})
function pianoNote(projectId:string,node:string,pitch:number,velocity:number,bend?:number) {
  if(project.value?.id!==projectId)return
  if(socket?.readyState!==WebSocket.OPEN || socket.bufferedAmount>65536) {
    // Do not queue a delayed performance for reconnection. Closing releases the
    // server's held notes, including a release that could not be delivered.
    socket?.close(); report(new Error('Piano connection unavailable or overloaded; reconnect before playing.')); return
  }
  socket.send(JSON.stringify({type:'piano',node,...(bend===undefined?{pitch,velocity}:{bend})}))
}
async function changeTempo() {
  const value = Number(bpmDraft.value), id = project.value?.id
  if(!id || !Number.isFinite(value) || value<1 || value>400) { bpmDraft.value=telemetry.value?.bpm??project.value?.bpm??120;throw new Error('Tempo must be 1–400 BPM') }
  pendingTempo.value=value
  try { await api(`/projects/${id}/transport`,'POST',{action:'tempo',bpm:value}) }
  catch(e) { if(project.value?.id===id){pendingTempo.value=null;bpmDraft.value=telemetry.value?.bpm??project.value.bpm}throw e }
}
async function bangControl(node:string){
  if(!project.value||!editable.value||!graphActive.value)return
  const id=project.value.id
  await task(async()=>{
    // A bang right after a manual control edit must not be dropped: wait for that save so the request carries the current revision.
    for(let i=0;i<150&&saving.value;i++)await new Promise(resolve=>setTimeout(resolve,20))
    if(project.value?.id!==id)return
    await api(`/projects/${id}/control`,'PUT',{node,revision:project.value.revision})
  })
}
function controlValue(value:number|string){if(selected.value && ['toggle','control_input'].includes(selected.value.kind)){graphControl(selected.value.id,value);return}if(!project.value||!selected.value||!graphEditable.value)return;const next=clone(project.value);next.graph.nodes.find(n=>n.id===selected.value!.id)!.control_value=value;void task(()=>saveProject(next))}
function editCurve(parameters: Record<string, number>) {
  if (!project.value || !selected.value || !graphEditable.value) return
  const next = clone(project.value)
  Object.assign(next.graph.nodes.find(n => n.id === selected.value!.id)!.parameters, parameters)
  void task(() => saveProject(next))
}
function nodeChannels(width: number) {
  if (!project.value || !selected.value || !graphEditable.value) return
  const next = clone(project.value), node = next.graph.nodes.find(n => n.id === selected.value!.id)!
  node.channels = width
  // A narrower meter loses its upper level outputs, so drop their cables too.
  if (node.kind === 'meter') next.graph.edges = next.graph.edges.filter(e => e.source !== node.id || !/^level_[1-8]$/.test(e.source_port) || Number(e.source_port.slice(6)) <= width)
  void task(() => saveProject(next))
}
function renameNode(label: string) {
  if (!project.value || !selected.value || !graphEditable.value || !label.trim()) return
  const next = clone(project.value); next.graph.nodes.find(n=>n.id===selected.value!.id)!.label = label.trim()
  void task(()=>saveProject(next))
}
async function duplicateNode(id: string) {
  if (!project.value || !graphEditable.value) return
  const selectedIds=nodeMenu.value?.ids || getSelectedNodes.value.map(n=>n.id)
  const ids=selectedIds.includes(id)?selectedIds:[id]
  nodeMenu.value = null
  const next = clone(project.value), copy = duplicateNodes(next.graph.nodes, next.graph.edges, ids)
  next.graph.nodes.push(...copy.nodes); next.graph.edges.push(...copy.edges)
  await saveProject(next); await nextTick()
  document.querySelector<HTMLElement>(`.vue-flow__node[data-id="${copy.id}"] .patch-node`)?.focus()
}
function deleteNode(id: string) { nodeMenu.value = null; removeEdges([], [id]) }
function removeNode() { if (selected.value) deleteNode(selected.value.id) }
function toggleSelection(id: string) {
  const node=findNode(id);if(!node)return
  if(node.selected)removeSelectedNodes([node]);else addSelectedNodes([...getSelectedNodes.value,node])
}
async function groupSelection(ids: string[]) {
  if(!project.value||!graphEditable.value)return
  nodeMenu.value=null
  const next=clone(project.value);makeSubgraph(next.graph.nodes,next.graph.edges,ids,descriptors.value)
  await saveProject(next)
}
function deleteSelection(ids:string[]){nodeMenu.value=null;removeEdges([],ids)}
function groupMenu({event,nodes}:{event:MouseEvent;nodes:FlowNode[]}) {
  event.preventDefault();event.stopPropagation()
  if(nodes[0])void openNodeMenu(nodes[0].id,event)
}
async function openNodeMenu(id: string, event: MouseEvent) {
  menuOrigin = event.currentTarget as HTMLElement
  const rect = menuOrigin.getBoundingClientRect()
  const selection=getSelectedNodes.value.map(n=>n.id)
  nodeMenu.value = { id, ids:selection.includes(id)?selection:[id], x: Math.max(8, Math.min(event.clientX || rect.left + 20, innerWidth - 188)), y: Math.max(8, Math.min(event.clientY || rect.top + 20, innerHeight - 200)) }
  await nextTick()
  document.querySelector<HTMLElement>('.node-context-menu button')?.focus()
}
function closeNodeMenu() { nodeMenu.value = null; menuOrigin?.focus() }
function outsideNodeMenu(event: PointerEvent) {
  if (!(event.target instanceof Element && event.target.closest('.account-menu'))) accountOpen.value=false
  if (nodeMenu.value && !(event.target instanceof Element && event.target.closest('.node-context-menu'))) nodeMenu.value = null
}
function nodeMenuKey(event: KeyboardEvent) {
  if(event.key.toLowerCase()==='c'&&!event.ctrlKey&&!event.metaKey&&!event.altKey&&matchPlan.value&&graphEditable.value){event.preventDefault();event.stopPropagation();if(!event.repeat)void task(connectSelected);return}
  if (event.key.toLowerCase() === 'd' && !event.ctrlKey && !event.metaKey && !event.altKey && nodeMenu.value && graphEditable.value) { event.preventDefault(); event.stopPropagation(); if (!event.repeat) void task(()=>duplicateNode(nodeMenu.value!.id)); return }
  if (event.key.toLowerCase() === 'l' && !event.ctrlKey && !event.metaKey && !event.altKey && nodeMenu.value && nodeMenu.value.ids.length > 1 && graphEditable.value) { event.preventDefault(); event.stopPropagation(); if (!event.repeat) void task(()=>autoSpaceSelection(nodeMenu.value!.ids)); return }
  if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); closeNodeMenu() }
  if (event.key === 'Tab') nodeMenu.value = null
  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    event.preventDefault()
    const buttons = [...document.querySelectorAll<HTMLButtonElement>('.node-context-menu button:not(:disabled)')]
    const current = buttons.indexOf(document.activeElement as HTMLButtonElement)
    buttons[(current + (event.key === 'ArrowDown' ? 1 : buttons.length - 1)) % buttons.length]?.focus()
  }
}
async function transport(action: string, beat?: number) { if (!project.value) return; const run=async()=>{await api(`/projects/${project.value!.id}/transport`, 'POST', { action, beat, bpm: bpmDraft.value, count_in_beats: action === 'play' || action === 'repeat' ? countIn.value === 'bar' ? initialMeter.value.beats : Number(countIn.value) : 0 });await refreshAudio()};if(action==='activate')await withProgress('Starting audio engine and activating show',run);else await run() }
async function seekScore(beat: number) { if (!project.value || !graphActive.value || !conductor.value) return; await task(() => transport('seek', beat)) }
async function launchPart(playing: boolean, repeat = false) { if (project.value && part.value) await api(`/projects/${project.value.id}/clip`, 'POST', { part: part.value.id, playing, repeat }) }
async function cueParts(request:{action:string;parts?:string[];repeat?:boolean;count_in_pulses?:number;value?:number|null}) { if (project.value) await api(`/projects/${project.value.id}/cue`, 'POST', {...request,request_id:newId()}) }
function updatePart(part: Part) { if (!project.value) return; const next=clone(project.value); next.parts=next.parts.map(p=>p.id===part.id?part:p); void task(()=>saveProject(next)) }
const scoreWorkspace = ref<{ flush: () => Promise<void> } | null>(null)
async function downloadXML() { await scoreDraft.value?.flush(); if (!part.value || !project.value) return; const warnings=musicXMLExportWarnings(project.value); notice.value=warnings.join(' '); const url = URL.createObjectURL(new Blob([exportScoreMusicXML(project.value)], { type: 'application/xml' })); const link = document.createElement('a'); link.href = url; link.download = `${project.value.name}.musicxml`; link.click(); URL.revokeObjectURL(url) }
async function uploadXML(event: Event) { const input = event.target as HTMLInputElement; const file = input.files?.[0]; if (!file || !project.value) return; await scoreDraft.value?.flush(); await task(async () => { const result = importMusicXML(await file.text()); if (result.warnings.length && !window.confirm(`Import with these limitations?\n${result.warnings.join('\n')}`)) return; const next = clone(project.value!); next.parts = result.parts; next.score = result.score; for (const node of next.graph.nodes) if (node.part_id && !next.parts.some(part => part.id === node.part_id)) node.part_id = null; next.beats_per_bar = result.beatsPerBar; next.beat_unit = result.beatUnit; await saveProject(next) }); input.value = '' }
function settingsSaved(saved: Project) {
  if (project.value?.id === saved.id && saved.revision >= project.value.revision) { project.value = saved; bpmDraft.value = saved.bpm }
  settingsOpen.value = false
  void task(refresh)
}
async function enterStage() { await scoreDraft.value?.flush(); if (conductor.value) await transport('performance'); stage.value = true; selectedNode.value = null; projectPicker.value = false; pendingPort.value = null; selectedEdges.value = [] }
async function exitStage() { if(conductor.value) await transport('prepare'); stage.value = false; fullscreen.value = false }
let pendingScoreAudition: { project: string; part: string; note: string; pitch?: number } | null = null
let auditioningScore = false
async function auditionScore(part: string, note: string, pitch?: number) {
  if (!project.value || !graphActive.value) return
  pendingScoreAudition = { project: project.value.id, part, note, pitch }
  if (auditioningScore) return
  auditioningScore = true
  try {
    while (pendingScoreAudition) {
      const preview = pendingScoreAudition
      pendingScoreAudition = null
      if (project.value?.id === preview.project && graphActive.value)
        await api(`/projects/${preview.project}/audition`, 'POST', { part: preview.part, note: preview.note, pitch: preview.pitch })
    }
  } finally {
    pendingScoreAudition = null
    auditioningScore = false
  }
}
async function toggleFullscreen() { if (document.fullscreenElement) await document.exitFullscreen(); else if (document.documentElement.requestFullscreen) await document.documentElement.requestFullscreen(); else fullscreen.value = !fullscreen.value }
async function signOut() { accountOpen.value=false; if (user.value?.is_desktop_session) return; await scoreDraft.value?.flush(); await api('/logout', 'POST'); stage.value = false; browserOpen.value=false; systemOpen.value=false; user.value = null; project.value = null; randomizeHero(); socket?.close(); clearTimeout(reconnect) }
/** Score editor of a conducted project: Play also launches the part being edited, which otherwise waits for a cue. */
const previewsPart = computed(() => project.value?.mode === 'conducted' && tab.value === 'score' && !stage.value && !!part.value)
const repeatPreviewsPart = computed(() => project.value?.mode !== 'structured' && tab.value === 'score' && !stage.value && !!part.value)
async function togglePlayback() {
  if (!project.value || !conductor.value || transportBusy.value || progress.value) return
  transportBusy.value = true
  const id = project.value.id
  try {
    const preview = previewsPart.value && !(partPlayback.value?.playing || partPlayback.value?.pending?.[1])
    if (!active.value) {
      if(!graphActive.value) await toggleEngine()
      if (project.value?.id !== id) return
      await transport('play')
      if (preview) await launchPart(true)
    } else if (running.value || countingIn.value) await transport('pause')
    else {
      // Launched while the clock is stopped, the part sounds from the first beat instead of the next pulse.
      if (preview) await launchPart(true)
      await transport('play')
    }
  } finally { transportBusy.value = false }
}
async function playRepeated() {
  if (!project.value || !conductor.value || transportBusy.value || progress.value) return
  transportBusy.value = true
  const id = project.value.id
  try {
    if (!graphActive.value) await toggleEngine()
    if (project.value?.id !== id) return
    await transport('repeat')
    if (repeatPreviewsPart.value) await launchPart(true, true)
  } finally { transportBusy.value = false }
}
function keydown(event: KeyboardEvent) {
  // i toggles explanations everywhere, including inside open modals, unless the user is typing.
  if (event.key.toLowerCase() === 'i' && !event.ctrlKey && !event.metaKey && !event.altKey && !event.shiftKey && !event.isComposing && !(event.target instanceof Element && event.target.closest('input,textarea,select,[contenteditable]:not([contenteditable="false"])'))) {
    event.preventDefault(); event.stopPropagation(); if (!event.repeat) toggleHelp(); return
  }
  // Saving remains available while a node modal or text field has focus.
  if (!event.isComposing && (event.ctrlKey || event.metaKey) && !event.altKey && !event.shiftKey && event.key.toLowerCase() === 's' && project.value) {
    event.preventDefault(); event.stopPropagation()
    if (!event.repeat) {
      if (event.target instanceof HTMLInputElement || event.target instanceof HTMLTextAreaElement) event.target.blur()
      requestSave()
    }
    return
  }
  if (event.key === 'Escape') pendingPort.value = null
  if (event.defaultPrevented || event.isComposing || partPlayerPreview.value || nodeMenu.value || savingSubgraph.value || selectedNode.value || creating.value || settingsOpen.value || systemOpen.value || consoleOpen.value || documentationOpen.value || gearOpen.value || accountOpen.value || profileOpen.value || progress.value || browserOpen.value || quickNodes.value || consoleProject || standaloneHelp) return
  if (event.target instanceof Element && event.target.closest('dialog,input,textarea,select,[contenteditable]:not([contenteditable="false"])')) return
  if (event.key.toLowerCase()==='n' && !event.ctrlKey && !event.metaKey && !event.altKey && !event.shiftKey && tab.value==='graph' && !stage.value && graphEditable.value) {
    event.preventDefault();event.stopPropagation();if(!event.repeat)quickNodes.value=true
    return
  }
  if (event.target instanceof Element && event.target.closest('button,a,[role="button"]:not(.vue-flow__node)')) return
  if (event.code === 'Space' && !event.ctrlKey && !event.metaKey && !event.altKey && !event.shiftKey && conductor.value && project.value) {
    event.preventDefault(); event.stopPropagation()
    if (!event.repeat) void task(togglePlayback)
  } else if ((event.ctrlKey || event.metaKey) && !event.altKey && !event.shiftKey && event.key.toLowerCase() === 'z' && tab.value === 'graph' && !stage.value && editable.value) {
    event.preventDefault(); event.stopPropagation()
    if (!event.repeat) void task(undoEdit)
  } else if (event.key.toLowerCase() === 'a' && !event.altKey && !event.shiftKey && tab.value === 'graph' && !stage.value && project.value) {
    event.preventDefault(); event.stopPropagation(); if (!event.repeat) selectAllNodes()
  } else if (event.key.toLowerCase() === 'l' && !event.ctrlKey && !event.metaKey && !event.altKey && tab.value === 'graph' && !stage.value && graphEditable.value && getSelectedNodes.value.length > 1) {
    event.preventDefault(); event.stopPropagation(); if (!event.repeat) void task(() => autoSpaceSelection(getSelectedNodes.value.map(n => n.id)))
  } else if (event.key.toLowerCase() === 'c' && !event.ctrlKey && !event.metaKey && !event.altKey && tab.value==='graph' && !stage.value && graphEditable.value && matchPlan.value) {
    event.preventDefault();event.stopPropagation();if(!event.repeat)void task(connectSelected)
  } else if (event.key.toLowerCase() === 'd' && !event.ctrlKey && !event.metaKey && !event.altKey && tab.value === 'graph' && !stage.value && graphEditable.value) {
    const id = (event.target instanceof Element ? event.target.closest('.vue-flow__node')?.getAttribute('data-id') : null) || getSelectedNodes.value[0]?.id
    if (id) { event.preventDefault(); event.stopPropagation(); if (!event.repeat) void task(()=>duplicateNode(id)) }
  } else if ((event.key === 'Delete' || event.key === 'Backspace') && tab.value === 'graph' && !stage.value && (getSelectedNodes.value.length || getSelectedEdges.value.length || selectedEdges.value.length) && graphEditable.value) {
    event.preventDefault(); event.stopPropagation()
    if (!event.repeat) removeEdges(getSelectedEdges.value.length ? getSelectedEdges.value.map(e => e.id) : selectedEdges.value, getSelectedNodes.value.map(n => n.id))
  }
}

onMounted(async () => {
  window.addEventListener('resize',phoneLayoutChanged)
  const tick = () => { now.value = performance.now(); frame = requestAnimationFrame(tick) }; frame = requestAnimationFrame(tick); window.addEventListener('keydown', keydown, true); window.addEventListener('pointerdown', outsideNodeMenu); window.addEventListener('offline', offline); window.addEventListener('online', online)
  if (standaloneHelp) { await task(async () => { descriptors.value = await api<Descriptor[]>('/catalog') }); return }
  try { loginTitles.value = await api<typeof loginTitles.value>('/login-titles') } catch { /* use built-in title pairs */ } finally { revealHeroTitle() }
  await task(async () => { descriptors.value = await api<Descriptor[]>('/catalog'); const status = await api<{ bootstrap: boolean; active_project: string | null; graph_project:string|null }>('/status'); bootstrap.value = status.bootstrap; activeId.value = status.active_project; graphId.value = status.graph_project; try { user.value = await api('/me') } catch { return }; await refresh(); if (invitation) { const result = await api<{ project_id: string }>('/join', 'POST', { token: invitation }); await refresh(); await openProject(result.project_id); history.replaceState({}, '', '/') } else if (consoleProject) await openProject(consoleProject); else if (requestedDesktopProject) await openProject(requestedDesktopProject); else if (summaries.value[0]) await openProject((recentSummaries.value[0]||summaries.value[0]).id); else creating.value = true })
})
onBeforeUnmount(() => { window.removeEventListener('resize',phoneLayoutChanged); cancelAnimationFrame(frame); clearInterval(ping); clearTimeout(reconnect); clearTimeout(saveTimer); clearTimeout(heroTitleTimer); socket?.close(); window.removeEventListener('keydown', keydown, true); window.removeEventListener('pointerdown', outsideNodeMenu); window.removeEventListener('offline', offline); window.removeEventListener('online', online) })
</script>

<template>
  <HelpCenter v-if="standaloneHelp" :descriptors="descriptors" standalone />
  <EngineConsole v-else-if="consoleProject && user" :project-id="consoleProject" standalone />
  <div v-else class="app-shell" :class="{ 'fullscreen-fallback': fullscreen, 'performance-mode': stage }">
    <UserProfileDialog v-if="profileOpen && user" @close="closeProfile" @saved="value=>task(()=>profileSaved(value))"/>
    <header v-show="!stage" class="topbar"><a class="brand" href="/" aria-label="pr0former home"><img class="brand-mark" src="/app-icon.png" alt=""><strong>pr<span>0</span>former</strong><sup>ALPHA</sup></a><div class="topbar-center"><template v-if="user"><button class="icon-button project-icon" aria-label="Choose project" :title="project ? `${project.name} · ${performanceLocked ? 'performance' : project.mode} · choose project` : 'Choose project'" @click="projectPicker = !projectPicker"><FolderOpen :size="16" /></button><button class="project-title" :title="project ? `${project.name} · ${performanceLocked ? 'PERFORMANCE' : project.mode.toUpperCase()}` : 'Choose project'" @click="projectPicker = !projectPicker">{{ project?.name || 'Your workspace' }}<ChevronDown :size="14" /></button><span v-if="project" class="mode-pill">{{ performanceLocked ? 'PERFORMANCE' : project.mode }}</span></template><template v-else><span class="tiny-dot"></span> ELECTROACOUSTIC PERFORMANCE WORKSPACE</template></div><div class="topbar-right"><span class="server-indicator"><span class="status-dot" :class="{ live: connected }"></span>{{ user ? connected ? 'Server connected' : 'Connecting' : 'Local performance server' }}</span><button class="icon-button" aria-label="Fullscreen" @click="task(toggleFullscreen)"><Maximize :size="17" /></button><button class="icon-button" :class="{ pressed: showHelp }" :aria-pressed="showHelp" aria-label="Explanations" :title="showHelp ? 'Collapse descriptions (i)' : 'Expand descriptions (i)'" @click="toggleHelp()"><Info :size="17" /></button><button v-if="user" class="icon-button" aria-label="Project browser" title="Browse projects and samples" @click="browserOpen=true;projectPicker=false"><FolderOpen :size="19"/></button><button v-if="user && project && editable" class="icon-button save-button" :class="{ unsaved }" aria-label="Save revision" :title="`${revisionText} · Save revision (Cmd/Ctrl+S) · autosave every minute`" :disabled="checkpointBusy" @click="requestSave"><Save :size="18" /><i v-if="unsaved && !checkpointBusy" class="unsaved-dot" aria-hidden="true"></i></button><button v-if="project" class="revision save-status" role="status" :aria-label="`${revisionText}. Browse revisions`" :disabled="revisionsBusy" @click="openRevisions">{{ revisionText }}</button><div v-if="user" class="gear-menu" @keydown.esc="gearOpen=false"><button class="icon-button" aria-label="Settings menu" aria-haspopup="menu" :aria-expanded="gearOpen" @click="gearOpen=!gearOpen"><Settings :size="19"/></button><div v-if="gearOpen" class="settings-menu" role="menu"><button role="menuitem" :disabled="!project||saving" @click="settingsOpen=true;gearOpen=false">Project settings</button><button v-if="user.is_admin" role="menuitem" @click="systemOpen=true;gearOpen=false">System settings</button><button role="menuitem" :disabled="!project" @click="consoleOpen=true;gearOpen=false">Console</button><button role="menuitem" @click="documentationOpen=true;gearOpen=false">Help &amp; documentation</button></div></div><div v-if="user" class="gear-menu account-menu" @keydown="accountKey" @focusout="event=>{if(!(event.currentTarget as HTMLElement).contains(event.relatedTarget as Node))accountOpen=false}"><button ref="accountButton" class="avatar" aria-label="User menu" :title="`User menu — ${user.username}`" aria-haspopup="menu" :aria-expanded="accountOpen" @click="accountOpen=!accountOpen"><img v-if="avatarUrl(user)" :src="avatarUrl(user)" alt="Your avatar"><span v-else>{{user.username.slice(0,2).toUpperCase()}}</span></button><div v-if="accountOpen" class="settings-menu" role="menu" aria-label="User account"><button role="menuitem" @click="profileOpen=true;accountOpen=false">Edit user profile</button><button role="menuitem" :disabled="user.is_desktop_session" :title="user.is_desktop_session?'The bundled admin stays signed in until you quit the app':undefined" @click="task(signOut)">Sign out</button></div></div></div></header>
    <div v-if="error" class="error-banner" role="alert">{{ error }}<button class="icon-button" aria-label="Dismiss error" @click="error = ''"><X :size="16" /></button></div>
    <main v-if="!user" class="welcome"><div class="welcome-copy"><div class="eyebrow"><span class="tiny-dot"></span> A SHARED SPACE FOR SOUND</div><h1 class="hero-title" :class="{ visible: heroTitleVisible }" role="button" tabindex="0" title="Show another title" @click="cycleHeroTitle" @keydown.enter.prevent="cycleHeroTitle" @keydown.space.prevent="cycleHeroTitle">{{ heroSlogan }}<br><em>{{ heroSubline }}</em></h1><div v-if="heroNodes.length" class="welcome-patch" aria-label="Example compatible signal graph"><template v-for="(node,index) in heroNodes" :key="`${node.label}-${index}`"><article class="mini-node" :class="node.signal" :style="{ transform: `translateY(${node.offset}px)` }"><div class="mini-node-cap"><span>{{ node.category }}</span><Settings2 :size="11" /></div><div class="mini-node-title"><span>{{ node.symbol }}</span><strong>{{ node.label }}</strong></div><div class="mini-node-ports"><div><span v-for="port in node.inputs" :key="`in-${port.label}`" class="mini-port input" :class="port.signal"><i></i>{{ port.label }}</span></div><div><span v-for="port in node.outputs" :key="`out-${port.label}`" class="mini-port output" :class="port.signal">{{ port.label }}<i></i></span></div></div><div class="mini-node-foot"><span>{{ node.signal === 'control' ? 'CONTROL' : node.signal === 'midi' ? 'MIDI' : `${node.channels} CH` }}</span><span>—</span></div></article><svg v-if="index < heroNodes.length - 1" class="mini-wire" viewBox="0 0 58 190" preserveAspectRatio="none" aria-hidden="true"><path :class="node.outputs[0]?.signal" :d="`M 0 ${node.offset + 69} C 18 ${node.offset + 69}, 38 ${heroNodes[index + 1]!.offset + 69}, 58 ${heroNodes[index + 1]!.offset + 69}`" /></svg></template></div></div><form class="auth-card" @submit.prevent="authenticate"><div class="eyebrow">{{ bootstrap ? 'FIRST-TIME SETUP' : invitation ? 'YOU’RE INVITED' : 'WELCOME BACK' }}</div><h2>{{ bootstrap ? 'Create your first account' : registering ? 'Join the ensemble' : 'Enter your workspace' }}</h2><p>{{ bootstrap ? 'Your projects and audio stay on this server.' : 'Sign in to compose, connect, and perform.' }}</p><label>Username<input v-model="username" autocomplete="username" minlength="3" maxlength="64" required placeholder="Your username"></label><label>Password<input v-model="password" type="password" :autocomplete="registering || bootstrap ? 'new-password' : 'current-password'" required :minlength="registering || bootstrap ? 8 : 1" placeholder="Your password"></label><button class="button primary wide" :disabled="busy">{{ busy ? 'Connecting…' : bootstrap || registering ? 'Create account' : 'Sign in' }}<ChevronRight :size="17" /></button><button v-if="invitation && !bootstrap" type="button" class="text-button" @click="registering = !registering">{{ registering ? 'Already have an account? Sign in' : 'New here? Create an account' }}</button><div class="auth-footer"><Radio :size="14" /> Connect on the same physical network.</div></form></main>
    <template v-else>
      <div v-if="projectPicker" class="project-popover"><button @click="browserOpen=true;projectPicker=false"><FolderOpen :size="16"/> Browse all projects</button><div class="eyebrow">RECENTLY OPENED</div><button v-for="p in recentSummaries" :key="p.id" @click="task(() => openProject(p.id))"><span>{{ p.name }}</span><small>{{ p.mode }}</small></button><button @click="creating = true; projectPicker = false"><Plus :size="16" /> New project</button></div>
      <div v-if="project" v-show="!stage" class="workspace-tabs"><nav><button :class="{ active: tab === 'graph' }" @click="tab = 'graph'"><Network :size="13" /> Signal Graph</button><button :class="{ active: tab === 'score' }" @click="tab = 'score'"><Music2 :size="13" /> Score & Parts</button><button v-if="project.mode==='conducted'" :class="{ active: tab === 'conductor' }" @click="tab = 'conductor'"><Radio :size="13" /> Conductor</button><button :class="{ active: tab === 'ensemble' }" @click="tab = 'ensemble'"><Users :size="13" /> Ensemble</button><button :class="{ active: tab === 'monitor' }" @click="tab = 'monitor'"><Headphones :size="13" /> Monitor</button></nav><div class="graph-legend"><span><i class="legend-audio"></i>Audio</span><span><i class="legend-control"></i>Control</span><span><i class="legend-spectral"></i>Spectral</span><span><i class="legend-midi"></i>MIDI</span></div></div>
      <ConductorWorkspace v-if="project && stage && conductorStage" :project="project" :members="members" :playback="telemetry?.parts||[]" :can-bind="canBindConductorMidi" :local-owner="suppliesConductorMidi" :can-cue="conductor || (!performanceLocked && role==='editor')" :editable="false" performance :active="active" :stale="stale" @cue="request=>task(()=>cueParts(request))" @midi="performanceMidi" />
      <StageView v-else-if="project && stage" :project="project" :user-id="user?.id||''" :beats="scoreBeats" :part="part" :playback="partPlayback" :playback-all="telemetry?.parts||[]" :global-beat="beat" :beat="partBeat" :meter-beat="meterBeat" :position="scorePosition" :bpm="active && telemetry ? telemetry.bpm : project.bpm" :active="active" :stale="stale" :running="running" :status="partStatus" :can-launch="canLaunchPart" :monitor-open="stageMonitor" @select="selectedPart = $event" @launch="playing => task(() => launchPart(playing))" @midi="performanceMidi" @fullscreen="task(toggleFullscreen)" @monitor="stageMonitor = !stageMonitor" />
      <div v-else-if="project && tab === 'graph'" class="graph-workspace" :class="{ 'graph-light': graphLight }">
        <aside v-if="library" class="node-library"><div class="library-heading"><button class="library-accordion-title" :aria-expanded="nodeLibraryOpen" aria-controls="node-library-content" @click="sampleLibraryOpen=false;nodeLibraryOpen=!nodeLibraryOpen"><ChevronDown :size="14" :class="{collapsed:!nodeLibraryOpen}" /> Node library</button><button class="icon-button" aria-label="Hide node library" @click="library = false"><LayoutGrid :size="15" /></button></div><div :class="{expanded:nodeLibraryOpen}" :inert="!nodeLibraryOpen" :aria-hidden="!nodeLibraryOpen" id="node-library-content" class="node-library-content"><label class="search-box"><Search :size="15" /><input v-model="search" placeholder="Find a node…" aria-label="Search nodes"><kbd>/</kbd></label><select v-model="category" aria-label="Node category"><option v-for="c in categories" :key="c">{{ c }}</option></select><div class="library-list"><button v-for="d in catalog" :key="d.kind" class="library-node" :disabled="!graphEditable" :title="d.description" :draggable="graphEditable" @dragstart="libraryDrag($event,d.kind)" @touchstart.prevent @pointerdown="libraryTouchStart($event,{kind:d.kind},d.label)" @click="addNode(d)"><span class="library-glyph" :class="d.outputs[0]?.signal || 'audio'">{{ d.symbol }}</span><span>{{ d.label }}<small>{{ d.category }}</small></span><Plus :size="13" class="add-sign" /></button></div></div><SubgraphLibrary :open="!nodeLibraryOpen&&!sampleLibraryOpen" @toggle="nodeLibraryOpen=sampleLibraryOpen?false:!nodeLibraryOpen;sampleLibraryOpen=false" ref="libraryPanel" :editable="graphEditable" @touch-insert="libraryTouchStart" @insert="(id,version)=>task(()=>importSubgraph(id,version))" /><SampleLibrary @touch="(event,sample)=>libraryTouchStart(event,{sample:sample.id},sample.name)" ref="samplePanel" :key="project.id" :project-id="project.id" :open="sampleLibraryOpen" :editable="graphEditable" @toggle="sampleLibraryOpen=!sampleLibraryOpen;nodeLibraryOpen=false" @entries="projectSamples=$event" @use="addSampleNode" /></aside>
        <div v-if="libraryTouchPreview" class="library-touch-preview" :style="{left:`${libraryTouchPreview.x + 16}px`,top:`${libraryTouchPreview.y - 24}px`}" aria-hidden="true">＋ {{ libraryTouchPreview.label }}</div><div ref="graphCanvas" class="graph-canvas" @dragover.prevent @drop.prevent="libraryDrop"><VueFlow :key="graphParent || 'root'" :nodes="flowNodes" :edges="flowEdges" :node-types="nodeTypes" :edge-types="edgeTypes" :min-zoom="0.2" :max-zoom="2" :nodes-connectable="graphEditable" :connect-on-click="false" :delete-key-code="null" :pan-activation-key-code="null" :selection-key-code="true" :multi-selection-key-code="['Control','Meta']" :selection-mode="SelectionMode.Partial" :pan-on-drag="[1,2]" :fit-view-options="{maxZoom:phoneCompact ? 0.5 : 1}" :fit-view-on-init="!graphView" :default-viewport="graphView || { x: 0, y: 0, zoom: 1 }" @selection-start="selectionGesture=true" @selection-end="finishSelection" @connect="onConnect" @node-drag-stop="nodeDrag" @selection-drag-stop="nodeDrag" @selection-context-menu="groupMenu" @edge-click="({ edge }) => selectedEdges = [edge.id]" @node-click="selectedEdges = []" @pane-click="selectedEdges = []"><Background :gap="24" :size="1" pattern-color="#394043" /><Controls position="bottom-left" :show-interactive="false" /></VueFlow><div class="canvas-top"><button v-if="!library" class="button small" @click="library = true"><LayoutGrid :size="14" /> Library</button><div class="graph-location"><nav class="graph-breadcrumbs" aria-label="Graph path"><button class="text-button" @click="navigateGraph(null)">Root graph</button><template v-for="n in breadcrumbs" :key="n.id"><span> / </span><button class="text-button" @click="navigateGraph(n.id)">{{n.label}}</button></template></nav><span class="canvas-caption">{{ visibleNodes.length }} NODES <span>/</span> {{ visibleEdges.length }} CONNECTIONS</span></div><div class="canvas-tools"><button class="icon-button" :disabled="!undo.length || !graphEditable" aria-label="Undo" title="Undo (Ctrl/Cmd+Z)" @click="task(undoEdit)"><Undo2 :size="16" /></button><button class="icon-button" :aria-label="graphLight ? 'Dark graph theme' : 'Light graph theme'" :aria-pressed="graphLight" :title="graphLight ? 'Switch the graph and library to the dark theme' : 'Switch the graph and library to the light theme'" @click="graphLight = !graphLight"><Sun v-if="!graphLight" :size="16" /><Moon v-else :size="16" /></button></div></div><svg v-if="inputDrag.preview.value" style="position:fixed;inset:0;width:100vw;height:100vh;pointer-events:none;z-index:50" aria-hidden="true"><path v-for="(origin,index) in inputDrag.preview.value.origins" :key="index" :d="`M ${origin.x} ${origin.y} C ${origin.x+70} ${origin.y}, ${inputDrag.preview.value.x-70} ${inputDrag.preview.value.y}, ${inputDrag.preview.value.x} ${inputDrag.preview.value.y}`" fill="none" :stroke="origin.color" stroke-width="1.5" stroke-dasharray="6 5" /></svg><div v-if="inputDrag.preview.value" class="connection-hint">Move {{inputDrag.preview.value.ids.length}} connections to an input · Drop elsewhere to disconnect · Esc to cancel</div><div v-else-if="pendingPort" class="connection-hint">Choose a {{ pendingPort.direction === 'source' ? 'target input' : 'source output' }} · Esc to cancel</div><div class="canvas-bottom-note"><span class="tiny-dot"></span>{{ active ? 'SHOW ACTIVE · EDIT NODES AND CONNECTIONS LIVE' : 'PATCH YOUR PERFORMANCE' }}</div></div>
      </div>
      <div v-else-if="project && tab === 'score'" class="content-pane score-content"><ScoreWorkspace ref="scoreWorkspace" :key="project.id" :project="project" :user-id="user?.id||''" :members="members" :beats="scoreBeats" :editable="editable&&!stage" :playing="active && (!!telemetry?.running || countingIn)" :saving="saving" :save="saveScoreProject" :draft-session="scoreDraft || undefined" @focus="selectedPart=$event" @seek="seekScore" @audition="(part,note,pitch)=>task(()=>auditionScore(part,note,pitch))" ><template #tools><button aria-label="MusicXML" title="Export MusicXML" @click="task(downloadXML)"><Download :size="15" /></button><label class="score-import" role="button" tabindex="0" title="Import MusicXML" aria-label="Import MusicXML" @keydown.enter.prevent="($event.currentTarget as HTMLElement).querySelector('input')?.click()" @keydown.space.prevent="($event.currentTarget as HTMLElement).querySelector('input')?.click()"><Upload :size="15" /><input type="file" accept=".xml,.musicxml" style="display:none" :disabled="!editable || running || countingIn" @change="task(() => uploadXML($event))"></label></template><template #footer><button v-if="project.mode !== 'structured'" class="button small" :disabled="!active || stale || !canLaunchPart" @click="task(() => launchPart(true))">Launch part</button><button v-if="project.mode !== 'structured'" class="button small" :disabled="!active || stale || !canLaunchPart" @click="task(() => launchPart(false))">Stop part</button><output class="mode-pill" aria-label="Part playback status">{{ partStatus }}</output><span v-if="notice" role="status">{{notice}}</span></template></ScoreWorkspace></div>
      <ConductorWorkspace v-else-if="project && tab === 'conductor'" :project="project" :members="members" :playback="telemetry?.parts||[]" :can-bind="canBindConductorMidi" :local-owner="suppliesConductorMidi" :can-cue="conductor || (!performanceLocked && role==='editor')" :editable="editable" :active="active" :stale="stale" @save="next=>task(()=>saveProject(next))" @cue="request=>task(()=>cueParts(request))" @midi="performanceMidi" @enter="task(enterStage)" />
      <div v-else-if="project && tab === 'ensemble'" class="content-pane"><EnsembleWorkspace :project="project" :members="members" :user-id="user?.id||''" :owner="role==='owner'" @refresh="task(refreshMembers)" @project="acceptEnsembleProject" @error="report" /></div>
      <div v-else-if="!project" class="empty-workspace"><Music2 :size="40" /><h2>A new space for your ensemble.</h2><button class="button primary" @click="creating = true"><Plus :size="16" /> Create a project</button></div>
      <MonitorWorkspace ref="monitorWorkspace" v-if="project" v-show="stage ? stageMonitor : tab === 'monitor'" :key="project.id" :project-id="project.id" :active="graphActive" :nodes="project.graph.nodes" :auto-input="automaticInput" :telemetry="telemetry" :hardware="hardwareLevels" :hardware-stale="!connected || now-hardwareReceived>500" :stale="stale" :age="now-receivedAt" :sample-rate="audioSettings.sample_rate" :block-size="audioSettings.block_size" :visible="stage ? stageMonitor : tab === 'monitor'" :stage="stage" />
      <footer v-if="project" class="transport-bar"><div class="transport-controls"><button class="icon-button stop-button" :disabled="!active || !conductor" aria-label="Stop" @click="task(() => transport('stop'))"><Square :size="16" fill="currentColor" /></button><button class="play-button" :disabled="!conductor || transportBusy || !!progress" :aria-label="running || countingIn ? 'Pause' : 'Play'" title="Space: play / pause" @click="task(togglePlayback)"><Pause v-if="running || countingIn" :size="19" fill="currentColor" /><Play v-else :size="19" fill="currentColor" /></button><button class="play-button repeat-play-button" :class="{ active: !!partPlayback?.repeating }" :disabled="!conductor || transportBusy || !!progress" aria-label="Play and repeat" title="Play and repeat the score or focused part" @click="task(playRepeated)"><Repeat2 :size="19" /></button><div v-if="countingIn" class="position-display count-in-position" role="status"><strong>{{ telemetry?.count_in_remaining || '→' }}</strong><small>COUNT IN</small></div><div v-else class="position-display"><strong>{{ String(scorePosition.bar).padStart(3, '0') }}<span>:</span>{{ String(scorePosition.beat).padStart(2, '0') }}</strong><small>BAR · BEAT</small></div></div><label class="count-in-control">Count in<select v-model="countIn" aria-label="Count in" :disabled="!conductor || running || countingIn || transportBusy" title="Clicks play through connected browser monitors before starting from the beginning"><option value="0">Off</option><option value="bar">1 bar ({{ initialMeter.beats }} beats)</option><option v-for="beats in 32" :key="beats" :value="String(beats)">{{ beats }} {{ beats === 1 ? 'beat' : 'beats' }}</option></select></label><div class="tempo-control"><label for="tempo">TEMPO</label><input id="tempo" v-model.number="bpmDraft" type="number" min="1" max="400" :disabled="!graphActive || !conductor || !!tempoSource" :title="tempoSource ? 'Driven by ' + (project.graph.nodes.find(n=>n.id===tempoSource?.source)?.label || tempoSource.source) : 'Project tempo'" @focus="tempoEditing=true" @blur="tempoEditing=false" @keydown.enter.prevent.stop="($event.target as HTMLInputElement).blur()" @change="task(changeTempo)"><span title="Quarter notes per minute">♩ BPM</span><div class="beat-lights"><i v-for="b in scorePosition.beats" :key="b" :class="{ lit: running && scorePosition.beat === b }"></i></div><button class="icon-button metronome-toggle" aria-label="Metronome" :aria-pressed="metronomeOn" :title="metronomeOn ? 'Metronome on · clicks in browser monitors while playing' : 'Metronome · click track in browser monitors while playing'" :disabled="!graphActive || !conductor" @click="task(toggleMetronome)"><svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true"><path d="M5.2 2h5.6l2.4 12H2.8z" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round"/><path d="M8 10.5 12.4 3.2" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/><circle cx="8" cy="10.8" r="1.2" fill="currentColor"/></svg></button></div><div class="transport-right"><span class="engine-label"><span class="status-dot" :class="{ live: graphActive && !stale }"></span>{{ graphActive ? stale ? 'ENGINE STALE' : 'ENGINE RUNNING' : 'ENGINE IDLE' }}<small>{{ audioSettings.sample_rate / 1000 }} kHz · {{ audioSettings.block_size }}-frame DSP blocks</small></span><button class="button collapsible" :class="{ live: graphActive }" :disabled="!conductor||!!progress" :aria-label="graphActive ? 'Disable audio engine' : 'Enable audio engine'" :title="graphActive ? 'Disable audio engine' : 'Enable audio engine'" @click="task(toggleEngine)"><Power :size="15" /><span class="button-label">{{ graphActive ? 'Disable audio engine' : 'Enable audio engine' }}</span></button><button v-if="!stage" class="button collapsible" aria-label="Performance mode" title="Performance mode · stage view for players" @click="task(enterStage)"><Presentation :size="15" /><span class="button-label">Performance mode</span></button><button v-else class="button collapsible" aria-label="End performance" title="End performance mode" @click="task(exitStage)"><Presentation :size="15" /><span class="button-label">End performance</span></button><button class="icon-button browser-monitor-toggle" :class="{ 'monitor-on': monitorOn }" :style="{ '--vu': monitorOn ? monitorLevel : 0 }" aria-label="Browser monitor" :aria-pressed="monitorOn" :aria-busy="['connecting','disconnecting','new'].includes(monitorState)" :title="monitorOn ? 'Turn off browser monitor' : graphActive ? 'Turn on browser monitor' : 'Enable the audio engine to monitor in this browser'" :disabled="(!graphActive && !monitorOn) || monitorState === 'disconnecting'" @click="monitorWorkspace?.toggleMonitor()"><Headphones :size="18" /></button></div></footer>
    </template>
    <div v-if="nodeMenu" class="node-context-menu" role="menu" aria-label="Node actions" :style="{ left: `${nodeMenu.x}px`, top: `${nodeMenu.y}px` }" @keydown="nodeMenuKey"><button v-if="matchPlan" role="menuitem" :disabled="!graphEditable" @click="task(connectSelected)">Connect matching ports <kbd>C</kbd></button>
      <template v-if="nodeMenu.ids.length>1">
        <button role="menuitem" :disabled="!graphEditable" @click="task(()=>autoSpaceSelection(nodeMenu!.ids))">Auto-space <kbd>L</kbd></button>
        <button role="menuitem" :disabled="!graphEditable" @click="task(()=>groupSelection(nodeMenu!.ids))">Make subgraph</button>
        <button role="menuitem" :disabled="!graphEditable" @click="task(()=>duplicateNode(nodeMenu!.id))">Duplicate</button>
        <button role="menuitem" class="danger" :disabled="!graphEditable" @click="deleteSelection(nodeMenu.ids)">Delete</button>
      </template>
      <template v-else>
      <button role="menuitem" @click="selectedNode = nodeMenu.id; nodeMenu = null">Edit node</button>
      <button v-if="project?.graph.nodes.find(n=>n.id===nodeMenu?.id)?.kind==='subgraph'" role="menuitem" @click="navigateGraph(nodeMenu.id)">Open subgraph</button>
      <button v-if="project?.graph.nodes.find(n=>n.id===nodeMenu?.id)?.kind==='subgraph'" role="menuitem" :disabled="!graphEditable" @click="savingSubgraph=nodeMenu.id;nodeMenu=null">Save to subgraph library</button>
      <button role="menuitem" :disabled="!graphEditable" @click="task(()=>duplicateNode(nodeMenu!.id))">Duplicate node <kbd>D</kbd></button>
      <button role="menuitem" class="danger" :disabled="!graphEditable" @click="deleteNode(nodeMenu.id)">Delete node</button>
      </template>
    </div>
    <SaveSubgraphDialog v-if="savingSubgraph && project" :project-id="project.id" :revision="project.revision" :node="project.graph.nodes.find(n=>n.id===savingSubgraph)!" @close="savingSubgraph=null" @saved="reference=>task(()=>librarySaved(reference))" />
    <QuickNodeBrowser v-if="quickNodes && project && tab==='graph' && !stage" :project-id="project.id" :descriptors="descriptors" :samples="projectSamples" @close="quickNodes=false" @insert="quickInsert"/><ProjectBrowser v-if="browserOpen && user" @close="browserOpen=false" @open="id=>task(()=>openProject(id))" @create="browserOpen=false;creating=true" @samples="samplePanel?.refresh()"/><SystemSettings v-if="systemOpen && user?.is_admin" :current-user-id="user.id" :project-id="project?.id" :project="project||undefined" :saving="saving" :editable="user.is_admin" :active="!!activeId" @part="updatePart" @close="systemOpen=false" @saved="task(refreshAudio)" />
    <EngineConsole v-if="consoleOpen && project" :key="project.id" :project-id="project.id" @close="consoleOpen=false" />
    <HelpCenter v-if="documentationOpen" :descriptors="descriptors" @close="documentationOpen=false" />
    <TaskProgress v-if="progress" :title="progress" />
    <ProjectSettings v-if="settingsOpen && project" :key="project.id" :project="project" :active="active" :editable="editable" @close="settingsOpen = false" @saved="settingsSaved" />
    <PartPlaybackDialog v-if="project && previewNode" :key="`${project.id}/${previewNode.id}/${previewNode.part_id}`" :project="project" :part="project.parts.find(p=>p.id===previewNode?.part_id)" :values="telemetry?.values[previewNode.id]" :stale="stale || !graphActive" @close="partPlayerPreview=null" />
    <NodeModal :apply-script="applyScript" :script-status="telemetry?.scripts?.[selectedNode!]" :osc-message="telemetry?.osc_messages?.[selectedNode!]" :route-targets="telemetry?.route_targets" :route-target="telemetry?.route_targets?.[selected?.id??'']" :samples="projectSamples" @sample="assignSample" @sample-choices="updateSampleChoices" :input-error="telemetry?.midi_input_error" :io-status="telemetry?.node_io" @io="assignNodeIo" :parts="project?.parts" @part="assignNodePart" :project-id="project?.id" v-if="selected && selectedDescriptor && project" :visualization="telemetry?.visualizations?.[selected.id]" :sample-rate="audioSettings.sample_rate" :block-size="audioSettings.block_size" :interfaces="audioSettings.interfaces.filter(i=>i.enabled)" :node="selected" :descriptor="selectedDescriptor" :nodes="project.graph.nodes" :edges="project.graph.edges" :values="telemetry?.values[selected.id]" :stale="stale" :editable="editable" :active="graphActive" :saving="saving" @rename="renameNode" @expand="navigateGraph(selected!.id)" @close="selectedNode = null" @change="editParameter" @disconnect="disconnect" @source="id => selectedNode = id" @undo="task(undoEdit)" @remove="removeNode" @upload="file => task(() => withProgress('Importing and converting clip', () => uploadSample(file)))" @channels="nodeChannels" @control="controlValue" @curve="editCurve" />
    <div v-if="creating" class="overlay"><form class="dialog-card" @submit.prevent="createProject"><header><div><div class="eyebrow">START SOMETHING</div><h2>New performance</h2></div><button type="button" class="icon-button" aria-label="Close" @click="creating = false"><X :size="20" /></button></header><label>Project name<input v-model="newName" maxlength="120" required autofocus></label><label>Performance mode</label><label v-for="m in [{ id: 'structured', title: 'Structured', text: 'A repeatable score and a shared timeline.' }, { id: 'conducted', title: 'Conducted', text: 'One conductor, an evolving performance.' }, { id: 'freeform', title: 'Freeform', text: 'Independent players, a common pulse.' }]" :key="m.id" class="mode-choice" :class="{ chosen: newMode === m.id }"><input v-model="newMode" type="radio" :value="m.id"><div><strong>{{ m.title }}</strong><p>{{ m.text }}</p></div></label><HelpNote>Structured mode follows the shared score. Conducted and freeform modes also support individual part launching.</HelpNote><button class="button primary wide" :disabled="busy">{{ busy ? 'Creating…' : 'Create performance' }}<Plus :size="16" /></button></form></div>

    <div v-if="revisionsOpen" class="overlay" @click.self="revisionsOpen=false"><section class="dialog-card revision-dialog" role="dialog" aria-modal="true" aria-labelledby="revisions-title"><header><div><div class="eyebrow">PROJECT HISTORY</div><h2 id="revisions-title">Revisions</h2></div><button type="button" class="icon-button" aria-label="Close" @click="revisionsOpen=false"><X :size="20" /></button></header><HelpNote>Select a revision to load it. Editing an older revision creates a new revision at the end of the history.</HelpNote><ol class="revision-list"><li v-for="entry in revisions" :key="entry.revision" :class="{ current: entry.current, loaded: loadedRevision === entry.revision }"><span class="revision-line" aria-hidden="true"></span><button class="revision-entry" @click="loadRevision(entry.revision)"><strong>Revision {{ entry.revision }}</strong><small v-if="entry.current">Current working copy</small><small v-else-if="loadedRevision === entry.revision">Loaded source · next save branches here</small><small v-else>Saved snapshot</small></button></li></ol></section></div>
  </div>
</template>
