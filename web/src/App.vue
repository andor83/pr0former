<script setup lang="ts">
import { newId } from './id'
import {matchingPorts,selectionOrder} from './connectMatching'
import { defineAsyncComponent, computed, nextTick, markRaw, onBeforeUnmount, onMounted, ref, shallowRef, provide, watch } from 'vue'
import { VueFlow, useVueFlow, SelectionMode } from '@vue-flow/core'
import type { Connection, Node as FlowNode, Edge as FlowEdge } from '@vue-flow/core'
import { useCanvasTouch } from './canvasTouch'
import {useInputConnectionDrag} from './inputConnectionDrag'
import { useLibraryTouch, type LibraryItem } from './libraryTouch'
import { useTouchScrollContainment } from './touchScroll'
import { Background } from '@vue-flow/background'
import { Controls } from '@vue-flow/controls'
import { Activity, AudioLines, ChevronDown, ChevronRight, Disc3, FolderOpen, Headphones, LayoutGrid, LogOut, Maximize, Music2, Network, Pause, Play, Plus, Radio, Search, Settings, Settings2, Square, Users, X, Download, Upload, Undo2, Save } from '@lucide/vue'
import SubgraphLibrary from './components/SubgraphLibrary.vue'
import SampleLibrary from './components/SampleLibrary.vue'
import type { SampleEntry } from './samples'
import SaveSubgraphDialog from './components/SaveSubgraphDialog.vue'
import PatchNode from './components/PatchNode.vue'
import SignalEdge from './components/SignalEdge.vue'
import NodeModal from './components/NodeModal.vue'
const ScoreEditor = defineAsyncComponent(() => import('./components/ScoreEditor.vue'))
import MonitorWorkspace from './components/MonitorWorkspace.vue'
import type { HardwareLevels } from './types'
import StageView from './components/StageView.vue'
import ProjectSettings from './components/ProjectSettings.vue'
import SystemSettings from './components/SystemSettings.vue'
import EngineConsole from './components/EngineConsole.vue'
import TaskProgress from './components/TaskProgress.vue'
const settingsOpen = ref(false), systemOpen=ref(false), consoleOpen=ref(false), gearOpen=ref(false)
const progress=ref(''),engineEnabled=ref(false),audioSettings=ref<{sample_rate:number;block_size:number;interfaces:any[]}>({sample_rate:48000,block_size:128,interfaces:[]})
const consoleProject=new URLSearchParams(location.search).get('console')
async function withProgress(title:string,fn:()=>Promise<void>){progress.value=title;try{await fn()}finally{progress.value=''}}
async function refreshAudio(){const [s,d]=await Promise.all([api<typeof audioSettings.value>('/system/audio'),api<any>('/devices')]);audioSettings.value=s;devices.value=d;engineEnabled.value=d.engine_enabled}
async function toggleEngine(){if(!project.value)return;await api(`/projects/${project.value.id}/engine`,'POST',{enabled:!graphActive.value});await refreshAudio()}

const stage = ref(false), stageMonitor = ref(false)
import { api } from './api'
import { nodeDescriptor, descendants, duplicateNodes, makeSubgraph } from './subgraphs'
import { importMusicXML, exportMusicXML } from './musicxml'
import type { Descriptor, GraphEdge, GraphNode, IoConfig, Member, Mode, Part, Project, Summary, Telemetry } from './types'

const user = ref<{ id: string; username: string } | null>(null)
const bootstrap = ref(false), registering = ref(false), username = ref(''), password = ref('')
const busy = ref(false), error = ref(''), notice = ref('')
const summaries = ref<Summary[]>([]), project = ref<Project | null>(null), role = ref('performer')
const descriptors = ref<Descriptor[]>([]), members = ref<Member[]>([])
const nodeLibraryOpen = ref(true), sampleLibraryOpen=ref(false)
const samplePanel=ref<InstanceType<typeof SampleLibrary>>(),projectSamples=ref<SampleEntry[]>([])
const tab = ref('graph'), library = ref(true), search = ref(''), category = ref('All nodes')
const selectedNode = ref<string | null>(null), selectedPart = ref(''), projectPicker = ref(false), creating = ref(false)
const newName = ref('Untitled performance'), newMode = ref<Mode>('conducted')
const telemetry = shallowRef<Telemetry | null>(null), receivedAt = ref(0), now = ref(performance.now()), connected = ref(false)
const hardwareLevels = shallowRef<HardwareLevels | null>(null), hardwareReceived = ref(0)
const tempoSource = computed(() => project.value?.graph.edges.find(e => e.target_port === 'tempo' && project.value?.graph.nodes.some(n => n.id === e.target && n.kind === 'clock')))
const graphId = ref<string | null>(null)
const activeId = ref<string | null>(null), bpmDraft = ref(120), saving = ref(false), fullscreen = ref(false)
const devices = ref<any>(null), inviteLink = ref(''), inviteRole = ref('performer')
type SaveStatus = {revision:number;change_revision:number;dirty:boolean}
const savedRevision = ref<SaveStatus | null>(null), checkpointBusy = ref(false)
const saveRequested = ref<string | null>(null)
const unsaved = computed(() => !!project.value && (!savedRevision.value || project.value.revision > savedRevision.value.change_revision || parameterPending.value || saving.value))
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
    const saved = await api<SaveStatus>(`/projects/${id}/save`, 'POST', {})
    acceptSaveStatus(id, saved)
    if (project.value?.id === id) notice.value = `Revision ${saved.revision} saved`
  } catch (e) { report(e) }
  finally { checkpointBusy.value = false }
}

const libraryPanel = ref<InstanceType<typeof SubgraphLibrary>>(), savingSubgraph = ref<string | null>(null)
const nodeMenu = ref<{ id: string; ids: string[]; x: number; y: number } | null>(null)
let menuOrigin: HTMLElement | null = null
const undo = ref<Project[]>([]), selectedEdges = ref<string[]>([])
const nodeTypes = { instrument: markRaw(PatchNode) }, edgeTypes = { signal: markRaw(SignalEdge) }
const phoneMedia=window.matchMedia('(max-width:600px), (max-height:500px) and (pointer:coarse)')
const phoneCompact=ref(phoneMedia.matches)
function phoneLayoutChanged(){const wasPhone=phoneCompact.value;phoneCompact.value=phoneMedia.matches;if(wasPhone||phoneCompact.value)void nextTick(()=>requestAnimationFrame(()=>fitView({padding:.18,maxZoom:phoneCompact.value ? 0.5 : 1})))}
const { fitView, screenToFlowCoordinate, getSelectedNodes, getSelectedEdges, findNode, addSelectedNodes, removeSelectedNodes } = useVueFlow()
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
let lastEngineStatus = -Infinity
let frame = 0, lastSequence = -1, lastEpoch = '', offset = 0, bestRtt = Infinity
let saveTimer: ReturnType<typeof setTimeout> | undefined
let queuedParameters = new Map<string, { node: string; parameter: string; value: number }>()
let parameterFlush = false
const invitation = new URLSearchParams(location.search).get('invite')
const editable = computed(() => ['owner', 'editor', 'conductor'].includes(role.value))
const conductor = computed(() => ['owner', 'conductor'].includes(role.value))
const transportBusy = ref(false), parameterPending = ref(false)
const countIn = ref('bar')
watch(() => project.value?.id, () => { countIn.value = 'bar' })
watch(()=>project.value?.id,async id=>{projectSamples.value=[];if(id){try{const samples=await api<SampleEntry[]>(`/projects/${id}/samples`);if(project.value?.id===id)projectSamples.value=samples}catch(e){report(e)}}})
const countingIn = computed(() => active.value && !stale.value && telemetry.value?.count_in_remaining != null)
const graphEditable = computed(() => editable.value && !saving.value && !parameterPending.value && !progress.value)
const graphActive = computed(() => !!project.value && graphId.value === project.value.id)
const active = computed(() => !!project.value && activeId.value === project.value.id)
const stale = computed(() => !connected.value || now.value - receivedAt.value > 500)
const running = computed(() => active.value && !!telemetry.value?.running && !stale.value)
const beat = computed(() => {
  const t = telemetry.value
  if (!active.value || !t) return 0
  if (stale.value || !t.running) return t.beat
  const elapsed = Math.max(0, Math.min(500, now.value + offset - t.server_time))
  return t.beat + elapsed / 60000 * t.bpm
})
const meterBeat = computed(() => beat.value * (project.value?.beat_unit || 4) / 4)
const graphParent = ref<string | null>(null)
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
  await nextTick(); fitView({padding:0.2,maxZoom:phoneCompact.value ? 0.5 : 1})
}
function openNode(id: string) {
  const node = project.value?.graph.nodes.find(n=>n.id===id)
  if (node?.kind === 'subgraph') void navigateGraph(id); else selectedNode.value = id
}
const selected = computed(() => project.value?.graph.nodes.find(n => n.id === selectedNode.value))
const selectedDescriptor = computed(() => selected.value && project.value ? nodeDescriptor(selected.value, project.value.graph.nodes, descriptors.value) : undefined)
const part = computed(() => project.value?.parts.find(p => p.id === selectedPart.value) || project.value?.parts[0])
const partPlayback = computed(() => active.value ? telemetry.value?.parts?.find(p => p.id === part.value?.id) : undefined)
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
const flowNodes = computed<FlowNode[]>(() => visibleNodes.value.map(n => ({ id: n.id, type: 'instrument', position: { x: n.x, y: n.y }, draggable: graphEditable.value, data: { projectId:project.value!.id,piano:pianoNote,active:graphActive.value,editable:editable.value&&!progress.value,driven:project.value!.graph.edges.some(e=>e.target===n.id&&e.target_port==='in'),setControl:graphControl,bang:bangControl,node: n, descriptor: nodeDescriptor(n, project.value!.graph.nodes, descriptors.value), open: openNode, edit: (id: string) => selectedNode.value = id, connectPort, toggleSelection, contextMenu: openNodeMenu } })))
const flowEdges = computed<FlowEdge[]>(() => visibleEdges.value.filter(e=>!inputDrag.hidden.value.has(e.id)).map(e => {
  const source = project.value!.graph.nodes.find(n => n.id === e.source)
  const d = source ? nodeDescriptor(source, project.value!.graph.nodes, descriptors.value) : undefined
  return { id: e.id, source: e.source, target: e.target, sourceHandle: e.source_port, targetHandle: e.target_port, type: 'signal', data: { projectId: project.value!.id, signal: d?.outputs.find(p => p.id === e.source_port)?.signal || 'control', channels: d?.outputs.find(p => p.id === e.source_port)?.fixed_channels || source?.channels || 1, active: graphActive.value && !stale.value } }
}))
provide('telemetry', telemetry)
provide('telemetryStale',stale)
const pendingPort = ref<{ node: string; port: string; direction: string } | null>(null)

function clone<T>(value: T): T { return JSON.parse(JSON.stringify(value)) }
function report(e: unknown) { error.value = e instanceof Error ? e.message : String(e) }
async function task(fn: () => Promise<void>) { error.value = ''; try { await fn() } catch (e) { report(e) } }
async function refresh() { summaries.value = await api<Summary[]>('/projects') }
async function authenticate() {
  busy.value = true
  await task(async () => {
    user.value = await api('/' + (bootstrap.value || registering.value ? 'register' : 'login'), 'POST', { username: username.value, password: password.value, invite: invitation })
    password.value = ''
    if (invitation) { const joined = await api<{ project_id: string }>('/join', 'POST', { token: invitation }); await refresh(); await openProject(joined.project_id); history.replaceState({}, '', '/') }
    else { await refresh(); if (summaries.value[0]) await openProject(summaries.value[0].id); else creating.value = true }
  }); busy.value = false
}
async function openProject(id: string) {
  await withProgress('Loading project and preparing clips', async()=>{
  const result = await api<{ project: Project; role: string }>(`/projects/${id}`)
  graphParent.value = null; project.value = result.project; role.value = result.role; bpmDraft.value = result.project.bpm
  settingsOpen.value = false; stage.value = false; selectedPart.value = result.project.parts.find(p => p.performer === user.value?.id)?.id || result.project.parts[0]?.id || ''; selectedNode.value = null; projectPicker.value = false; telemetry.value = null; receivedAt.value = 0; undo.value = []
  members.value = await api<Member[]>(`/projects/${id}/members`); connect(id)
  await refreshAudio(); systemOpen.value=false; consoleOpen.value=false
  setTimeout(() => fitView({ padding: 0.18, duration: 300, maxZoom:phoneCompact.value ? 0.5 : 1 }), 100)
  })
}
function visualizerSubscription(){if(socket?.readyState===WebSocket.OPEN)socket.send(JSON.stringify({type:'visualizers',enabled:graphActive.value && !stage.value && (tab.value==='graph'||!!selected.value?.kind.endsWith('_visualizer'))}))}
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
    if(message.type==='samples')void samplePanel.value?.refresh()
    if(message.type==='hardware_levels'){hardwareLevels.value=message;hardwareReceived.value=performance.now()}
    if(message.type==='project_save') acceptSaveStatus(id, message.save)
    if(message.type==='audio_engine_status'){engineEnabled.value=message.enabled;audioSettings.value.sample_rate=message.sample_rate;audioSettings.value.block_size=message.block_size}
    if(message.type==='system_audio'){audioSettings.value=message.settings}
    if (message.type === 'pong') { const rtt = performance.now() - message.client_time; if (rtt < bestRtt) { bestRtt = rtt; offset = message.server_time - (message.client_time + rtt / 2) } }
    if (message.type === 'engine_status' && message.server_time >= lastEngineStatus) {
      lastEngineStatus = message.server_time
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
      bpmDraft.value = message.bpm
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
  if (!project.value || saving.value) return
  const previous = clone(project.value)
  saving.value = true
  try { const saved = await api<Project>(`/projects/${next.id}`, 'PUT', next); if (record) undo.value = [...undo.value.slice(-49), previous]; if (project.value?.id === saved.id && saved.revision >= project.value.revision) project.value = saved }
  catch (e) { saveRequested.value = null; report(e); const latest = await api<{ project: Project }>(`/projects/${next.id}`); if (project.value?.id === latest.project.id && latest.project.revision >= project.value.revision) project.value = latest.project }
  finally { saving.value = false }
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
function assignSample(sample:SampleEntry){if(!project.value||!selected.value)return;const next=clone(project.value),node=next.graph.nodes.find(n=>n.id===selected.value!.id)!;node.parameters.asset=sample.asset!;node.channels=sample.channels;void task(()=>saveProject(next))}
function assignNodeIo(io: IoConfig) {
  if (!project.value || !selected.value || !graphEditable.value) return
  const next = clone(project.value), node = next.graph.nodes.find(n => n.id === selected.value!.id)
  if (node) { node.io = io; void task(() => saveProject(next)) }
}
function assignNodePart(id: string) {
  if (!project.value || !selected.value || !graphEditable.value) return
  const next = clone(project.value), node = next.graph.nodes.find(n => n.id === selected.value!.id)
  if (node?.kind === 'part_midi') { node.part_id = id || null; void task(() => saveProject(next)) }
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
      const sample=projectSamples.value.find(s=>s.id===data.sample)
      if(!sample)throw new Error('Add this sample to the current project before dragging it')
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
  const next = clone(project.value); next.graph.edges.push({ id: newId(), source: c.source, source_port: c.sourceHandle || 'out', target: c.target, target_port: c.targetHandle || 'in' }); void task(() => saveProject(next)); pendingPort.value = null
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
  if (!selected.value || !project.value || !editable.value) return
  if (selectedDescriptor.value?.parameters.find(p => p.id === key)?.structural) {
    if (!graphEditable.value) return
    const next = clone(project.value)
    const node=next.graph.nodes.find(n => n.id === selected.value!.id)!
    node.parameters[key] = value
    if(node.kind==='pitch_tracker'&&key==='slots')next.graph.edges=next.graph.edges.filter(e=>e.source!==node.id||!/^pitch[1-4]$/.test(e.source_port)||Number(e.source_port.slice(5))<=value)
    if(node.kind==='control_input') {
      const mode=node.parameters.mode??2,min=node.parameters.min??-100000,max=node.parameters.max??100000
      node.control_value=mode===4?(typeof node.control_value==='string'?node.control_value:''):mode===0?0:Math.max(min,Math.min(max,mode===1?Math.round(Number(node.control_value)||0):Number(node.control_value)||0))
    }
    void task(() => saveProject(next))
    return
  }
  if (saving.value && !parameterFlush) return
  if (!queuedParameters.size && !parameterFlush) remember()
  parameterPending.value = true
  queuedParameters.set(`${selected.value.id}/${key}`, { node: selected.value.id, parameter: key, value })
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
  for (const part of next.parts) if (part.instrument_node && nodeIds.includes(part.instrument_node)) part.instrument_node = null
  if (selectedNode.value && nodeIds.includes(selectedNode.value)) selectedNode.value = null
  selectedEdges.value = []
  void task(() => saveProject(next))
}
function disconnect(edge: GraphEdge) { removeEdges([edge.id]) }
async function uploadSample(file: File) { if (!project.value || !selected.value) return; const form = new FormData(); form.append('sample', file); const response = await fetch(`/api/projects/${project.value.id}/samples`, { method: 'POST', headers: { 'X-Pr0former': '1' }, body: form }); const result = await response.json(); if (!response.ok) throw new Error(result.error); const next = clone(project.value); const node = next.graph.nodes.find(n => n.id === selected.value!.id)!; node.parameters.asset = result.asset; node.channels = result.channels; await saveProject(next);await samplePanel.value?.refresh() }
const pendingControls=new Map<string,number|string>()
let controlsBusy=false
function graphControl(id:string,value:number|string){if(!editable.value)return;pendingControls.set(id,value);void flushControls()}
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
// Preserve press/release order even when HTTP requests take different amounts of time.
const pianoRequests = new Map<string,Promise<void>>()
function pianoNote(projectId:string,node:string,pitch:number,velocity:number) {
  const key = `${projectId}:${node}`
  const request = (pianoRequests.get(key) || Promise.resolve()).then(async () => {
    await api(`/projects/${projectId}/piano`, 'PUT', {node,pitch,velocity})
  }).catch(report)
  pianoRequests.set(key,request)
  void request.finally(() => { if(pianoRequests.get(key)===request)pianoRequests.delete(key) })
}
async function bangControl(node:string){if(!project.value||!editable.value||!graphActive.value||saving.value)return;await task(async()=>{await api(`/projects/${project.value!.id}/control`,'PUT',{node,revision:project.value!.revision})})}
function controlValue(value:number|string){if(selected.value?.kind==='toggle'){graphControl(selected.value.id,value);return}if(!project.value||!selected.value||!graphEditable.value)return;const next=clone(project.value);next.graph.nodes.find(n=>n.id===selected.value!.id)!.control_value=value;void task(()=>saveProject(next))}
function editCurve(parameters: Record<string, number>) {
  if (!project.value || !selected.value || !graphEditable.value) return
  const next = clone(project.value)
  Object.assign(next.graph.nodes.find(n => n.id === selected.value!.id)!.parameters, parameters)
  void task(() => saveProject(next))
}
function nodeChannels(width: number) { if (!project.value || !selected.value || !graphEditable.value) return; const next = clone(project.value); next.graph.nodes.find(n => n.id === selected.value!.id)!.channels = width; void task(() => saveProject(next)) }
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
  if (nodeMenu.value && !(event.target instanceof Element && event.target.closest('.node-context-menu'))) nodeMenu.value = null
}
function nodeMenuKey(event: KeyboardEvent) {
  if(event.key.toLowerCase()==='c'&&!event.ctrlKey&&!event.metaKey&&!event.altKey&&matchPlan.value&&graphEditable.value){event.preventDefault();event.stopPropagation();if(!event.repeat)void task(connectSelected);return}
  if (event.key.toLowerCase() === 'd' && !event.ctrlKey && !event.metaKey && !event.altKey && nodeMenu.value && graphEditable.value) { event.preventDefault(); event.stopPropagation(); if (!event.repeat) void task(()=>duplicateNode(nodeMenu.value!.id)); return }
  if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); closeNodeMenu() }
  if (event.key === 'Tab') nodeMenu.value = null
  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    event.preventDefault()
    const buttons = [...document.querySelectorAll<HTMLButtonElement>('.node-context-menu button:not(:disabled)')]
    const current = buttons.indexOf(document.activeElement as HTMLButtonElement)
    buttons[(current + (event.key === 'ArrowDown' ? 1 : buttons.length - 1)) % buttons.length]?.focus()
  }
}
async function transport(action: string) { if (!project.value) return; const run=async()=>{await api(`/projects/${project.value!.id}/transport`, 'POST', { action, bpm: bpmDraft.value, count_in_beats: action === 'play' ? countIn.value === 'bar' ? project.value!.beats_per_bar : Number(countIn.value) : 0 });await refreshAudio()};if(action==='activate')await withProgress('Starting audio engine and activating show',run);else await run() }
async function launchPart(playing: boolean) { if (project.value && part.value) await api(`/projects/${project.value.id}/clip`, 'POST', { part: part.value.id, playing }) }
function addPart() { if (!project.value) return; const next = clone(project.value); next.parts.push({ id: newId(), name: `Player ${next.parts.length + 1}`, performer: null, view: 'notation', clef: 'treble', notes: [], loop_beats: 8, instrument_node: null, midi_port: null, osc_destination: null, osc_address: '/pr0former/note' }); void task(() => saveProject(next)) }
function downloadXML() { if (!part.value || !project.value) return; const url = URL.createObjectURL(new Blob([exportMusicXML(part.value, project.value.beats_per_bar, project.value.beat_unit || 4)], { type: 'application/xml' })); const link = document.createElement('a'); link.href = url; link.download = `${part.value.name}.musicxml`; link.click(); URL.revokeObjectURL(url) }
async function uploadXML(event: Event) { const input = event.target as HTMLInputElement; const file = input.files?.[0]; if (!file || !project.value) return; await task(async () => { const result = importMusicXML(await file.text()); if (result.warnings.length && !window.confirm(`Import with these limitations?\n${result.warnings.join('\n')}`)) return; const next = clone(project.value!); next.parts = result.parts; for (const node of next.graph.nodes) if (node.part_id && !next.parts.some(part => part.id === node.part_id)) node.part_id = null; next.beats_per_bar = result.beatsPerBar; next.beat_unit = result.beatUnit; await saveProject(next) }); input.value = '' }
function updateMeter(beats: number, unit: number) { if (!project.value) return; const next = clone(project.value); next.beats_per_bar = beats; next.beat_unit = unit; void task(() => saveProject(next)) }
function updatePart(p: Part) { if (!project.value) return; const next = clone(project.value); next.parts = next.parts.map(x => x.id === p.id ? p : x); void task(() => saveProject(next)) }
function exportProject() { if (!project.value) return; const url = URL.createObjectURL(new Blob([JSON.stringify(project.value, null, 2)], { type: 'application/json' })); const a = document.createElement('a'); a.href = url; a.download = `${project.value.name}.pr0.json`; a.click(); URL.revokeObjectURL(url) }
async function importProject(event: Event) { const input = event.target as HTMLInputElement; const file = input.files?.[0]; if (!file || !project.value) return; await task(async () => { const imported = JSON.parse(await file.text()) as Project; imported.id = project.value!.id; imported.revision = project.value!.revision; await saveProject(imported) }); input.value = '' }

async function invite() { if (!project.value) return; const result = await api<{ path: string }>(`/projects/${project.value.id}/invite`, 'POST', { role: inviteRole.value }); inviteLink.value = location.origin + result.path }
function settingsSaved(saved: Project) {
  if (project.value?.id === saved.id && saved.revision >= project.value.revision) { project.value = saved; bpmDraft.value = saved.bpm }
  settingsOpen.value = false
  void task(refresh)
}
function enterStage() { stage.value = true; selectedNode.value = null; projectPicker.value = false; pendingPort.value = null; selectedEdges.value = [] }
async function toggleFullscreen() { if (document.fullscreenElement) await document.exitFullscreen(); else if (document.documentElement.requestFullscreen) await document.documentElement.requestFullscreen(); else fullscreen.value = !fullscreen.value }
async function signOut() { await api('/logout', 'POST'); stage.value = false; user.value = null; project.value = null; socket?.close(); clearTimeout(reconnect) }
async function togglePlayback() {
  if (!project.value || !conductor.value || transportBusy.value || progress.value) return
  transportBusy.value = true
  const id = project.value.id
  try {
    if (!active.value) {
      await transport('activate')
      if (project.value?.id !== id) return
      await transport('play')
    } else await transport(running.value || countingIn.value ? 'pause' : 'play')
  } finally { transportBusy.value = false }
}
function keydown(event: KeyboardEvent) {
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
  if (event.defaultPrevented || event.isComposing || nodeMenu.value || savingSubgraph.value || selectedNode.value || creating.value || settingsOpen.value || systemOpen.value || consoleOpen.value || gearOpen.value || progress.value || consoleProject) return
  if (event.target instanceof Element && event.target.closest('dialog,input,textarea,select,[contenteditable]:not([contenteditable="false"]),button,a,[role="button"]:not(.vue-flow__node)')) return
  if (event.code === 'Space' && !event.ctrlKey && !event.metaKey && !event.altKey && !event.shiftKey && conductor.value && project.value) {
    event.preventDefault(); event.stopPropagation()
    if (!event.repeat) void task(togglePlayback)
  } else if ((event.ctrlKey || event.metaKey) && !event.altKey && !event.shiftKey && event.key.toLowerCase() === 'z' && tab.value === 'graph' && !stage.value && editable.value) {
    event.preventDefault(); event.stopPropagation()
    if (!event.repeat) void task(undoEdit)
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
  await task(async () => { descriptors.value = await api<Descriptor[]>('/catalog'); const status = await api<{ bootstrap: boolean; active_project: string | null; graph_project:string|null }>('/status'); bootstrap.value = status.bootstrap; activeId.value = status.active_project; graphId.value = status.graph_project; try { user.value = await api('/me') } catch { return }; await refresh(); if (invitation) { const result = await api<{ project_id: string }>('/join', 'POST', { token: invitation }); await refresh(); await openProject(result.project_id); history.replaceState({}, '', '/') } else if (consoleProject) await openProject(consoleProject); else if (summaries.value[0]) await openProject(summaries.value[0].id); else creating.value = true })
})
onBeforeUnmount(() => { window.removeEventListener('resize',phoneLayoutChanged); cancelAnimationFrame(frame); clearInterval(ping); clearTimeout(reconnect); clearTimeout(saveTimer); socket?.close(); window.removeEventListener('keydown', keydown, true); window.removeEventListener('pointerdown', outsideNodeMenu); window.removeEventListener('offline', offline); window.removeEventListener('online', online) })
</script>

<template>
  <EngineConsole v-if="consoleProject && user" :project-id="consoleProject" standalone />
  <div v-else class="app-shell" :class="{ 'fullscreen-fallback': fullscreen, 'performance-mode': stage }">
    <header v-show="!stage" class="topbar"><a class="brand" href="/" aria-label="pr0former home"><span class="brand-mark">p<span>0</span></span><strong>pr<span>0</span>former</strong><sup>ALPHA</sup></a><div class="topbar-center"><span class="tiny-dot"></span> ELECTROACOUSTIC PERFORMANCE WORKSPACE</div><div class="topbar-right"><span class="server-indicator"><span class="status-dot" :class="{ live: connected }"></span>{{ user ? connected ? 'Server connected' : 'Connecting' : 'Local performance server' }}</span><button class="icon-button" aria-label="Fullscreen" @click="task(toggleFullscreen)"><Maximize :size="17" /></button><div v-if="user" class="gear-menu" @keydown.esc="gearOpen=false"><button class="icon-button" aria-label="Settings menu" aria-haspopup="menu" :aria-expanded="gearOpen" @click="gearOpen=!gearOpen"><Settings :size="19"/></button><div v-if="gearOpen" class="settings-menu" role="menu"><button role="menuitem" :disabled="!project||!editable||saving" @click="settingsOpen=true;gearOpen=false">Project settings</button><button role="menuitem" :disabled="!project" @click="systemOpen=true;gearOpen=false">System settings</button><button role="menuitem" :disabled="!project" @click="consoleOpen=true;gearOpen=false">Console</button></div></div><button v-if="user" class="avatar" :title="`Sign out ${user.username}`" @click="task(signOut)">{{ user.username.slice(0, 2).toUpperCase() }}</button></div></header>
    <div v-if="error" class="error-banner" role="alert">{{ error }}<button class="icon-button" aria-label="Dismiss error" @click="error = ''"><X :size="16" /></button></div>
    <main v-if="!user" class="welcome"><div class="welcome-copy"><div class="eyebrow"><span class="tiny-dot"></span> A SHARED SPACE FOR SOUND</div><h1>Compose the system.<br><em>Perform the unexpected.</em></h1><p>Scores, signals, and people.<br>One connected performance instrument.</p><div class="welcome-patch"><div class="mini-node amber">◷<small>CLOCK</small></div><span class="mini-wire"></span><div class="mini-node amber">×<small>MULTIPLY</small></div><span class="mini-wire cyan"></span><div class="mini-node cyan">∿<small>SOUND</small></div></div><span class="welcome-caption">BUILT FOR THE ROOM. OPEN TO POSSIBILITY.</span></div><form class="auth-card" @submit.prevent="authenticate"><div class="eyebrow">{{ bootstrap ? 'FIRST-TIME SETUP' : invitation ? 'YOU’RE INVITED' : 'WELCOME BACK' }}</div><h2>{{ bootstrap ? 'Create your first account' : registering ? 'Join the ensemble' : 'Enter your workspace' }}</h2><p>{{ bootstrap ? 'Your projects and audio stay on this server.' : 'Sign in to compose, connect, and perform.' }}</p><label>Username<input v-model="username" autocomplete="username" minlength="3" maxlength="64" required placeholder="Your username"></label><label>Password<input v-model="password" type="password" :autocomplete="registering || bootstrap ? 'new-password' : 'current-password'" required :minlength="registering || bootstrap ? 8 : 1" placeholder="Your password"></label><button class="button primary wide" :disabled="busy">{{ busy ? 'Connecting…' : bootstrap || registering ? 'Create account' : 'Sign in' }}<ChevronRight :size="17" /></button><button v-if="invitation && !bootstrap" type="button" class="text-button" @click="registering = !registering">{{ registering ? 'Already have an account? Sign in' : 'New here? Create an account' }}</button><div class="auth-footer"><Radio :size="14" /> Connect on the same physical network.</div></form></main>
    <template v-else>
      <div v-show="!stage" class="workspace-header"><div class="project-heading"><button class="project-icon" @click="projectPicker = !projectPicker" aria-label="Choose project"><FolderOpen :size="21" /></button><div><div class="eyebrow">PERFORMANCE / {{ project?.mode || 'NEW PROJECT' }}</div><button class="project-title" @click="projectPicker = !projectPicker">{{ project?.name || 'Your workspace' }}<ChevronDown :size="16" /></button><span v-if="project" class="revision" role="status">{{ checkpointBusy ? 'Saving revision…' : savedRevision ? `Revision ${savedRevision.revision}${unsaved ? ' · Unsaved changes' : ' · Saved'}` : 'Loading revision…' }}</span></div><span v-if="project" class="mode-pill">{{ active ? 'ACTIVE SHOW' : 'PREPARATION' }}</span></div><div class="workspace-actions"><button v-if="project" class="button small" @click="enterStage">Performance mode</button><button v-if="project && editable" class="button small" :disabled="checkpointBusy" title="Save revision (Cmd/Ctrl+S) · Autosave every minute" @click="requestSave"><Save :size="15" />Save revision</button></div></div>
      <div v-if="projectPicker" class="project-popover"><div class="eyebrow">PROJECTS</div><button v-for="p in summaries" :key="p.id" @click="task(() => openProject(p.id))"><span>{{ p.name }}</span><small>{{ p.mode }}</small></button><button @click="creating = true; projectPicker = false"><Plus :size="16" /> New project</button></div>
      <div v-if="project" v-show="!stage" class="workspace-tabs"><nav><button :class="{ active: tab === 'graph' }" @click="tab = 'graph'"><Network :size="16" /> Signal graph</button><button :class="{ active: tab === 'score' }" @click="tab = 'score'"><Music2 :size="16" /> Score & parts</button><button :class="{ active: tab === 'ensemble' }" @click="tab = 'ensemble'"><Users :size="16" /> Ensemble</button><button :class="{ active: tab === 'monitor' }" @click="tab = 'monitor'"><Headphones :size="16" /> Monitor</button></nav><div class="graph-legend"><span><i class="legend-audio"></i>Audio</span><span><i class="legend-control"></i>Control</span><span><i class="legend-spectral"></i>Spectral</span></div></div>
      <StageView v-if="project && stage" :project="project" :part="part" :beat="partBeat" :meter-beat="meterBeat" :bpm="active && telemetry ? telemetry.bpm : project.bpm" :active="active" :stale="stale" :running="running" :status="partStatus" :can-launch="canLaunchPart" :monitor-open="stageMonitor" @select="selectedPart = $event" @launch="playing => task(() => launchPart(playing))" @exit="stage = false; fullscreen = false" @fullscreen="task(toggleFullscreen)" @monitor="stageMonitor = !stageMonitor" />
      <div v-else-if="project && tab === 'graph'" class="graph-workspace">
        <aside v-if="library" class="node-library"><div class="library-heading"><button class="library-accordion-title" :aria-expanded="nodeLibraryOpen" aria-controls="node-library-content" @click="sampleLibraryOpen=false;nodeLibraryOpen=!nodeLibraryOpen"><ChevronDown :size="14" :class="{collapsed:!nodeLibraryOpen}" /> Node library</button><button class="icon-button" aria-label="Hide node library" @click="library = false"><LayoutGrid :size="15" /></button></div><div :class="{expanded:nodeLibraryOpen}" :inert="!nodeLibraryOpen" :aria-hidden="!nodeLibraryOpen" id="node-library-content" class="node-library-content"><label class="search-box"><Search :size="15" /><input v-model="search" placeholder="Find a node…" aria-label="Search nodes"><kbd>/</kbd></label><select v-model="category" aria-label="Node category"><option v-for="c in categories" :key="c">{{ c }}</option></select><div class="library-list"><button v-for="d in catalog" :key="d.kind" class="library-node" :disabled="!graphEditable" :title="d.description" :draggable="graphEditable" @dragstart="libraryDrag($event,d.kind)" @touchstart.prevent @pointerdown="libraryTouchStart($event,{kind:d.kind},d.label)" @click="addNode(d)"><span class="library-glyph" :class="d.outputs[0]?.signal || 'audio'">{{ d.symbol }}</span><span>{{ d.label }}<small>{{ d.category }}</small></span><Plus :size="13" class="add-sign" /></button></div></div><SubgraphLibrary :open="!nodeLibraryOpen&&!sampleLibraryOpen" @toggle="nodeLibraryOpen=sampleLibraryOpen?false:!nodeLibraryOpen;sampleLibraryOpen=false" ref="libraryPanel" :editable="graphEditable" @touch-insert="libraryTouchStart" @insert="(id,version)=>task(()=>importSubgraph(id,version))" /><SampleLibrary @touch="(event,sample)=>libraryTouchStart(event,{sample:sample.id},sample.name)" ref="samplePanel" :key="project.id" :project-id="project.id" :open="sampleLibraryOpen" :editable="graphEditable" @toggle="sampleLibraryOpen=!sampleLibraryOpen;nodeLibraryOpen=false" @entries="projectSamples=$event" @use="addSampleNode" /></aside>
        <div v-if="libraryTouchPreview" class="library-touch-preview" :style="{left:`${libraryTouchPreview.x + 16}px`,top:`${libraryTouchPreview.y - 24}px`}" aria-hidden="true">＋ {{ libraryTouchPreview.label }}</div><div ref="graphCanvas" class="graph-canvas" @dragover.prevent @drop.prevent="libraryDrop"><VueFlow :key="graphParent || 'root'" :nodes="flowNodes" :edges="flowEdges" :node-types="nodeTypes" :edge-types="edgeTypes" :min-zoom="0.2" :max-zoom="2" :nodes-connectable="graphEditable" :delete-key-code="null" :pan-activation-key-code="null" :selection-key-code="true" :multi-selection-key-code="['Control','Meta']" :selection-mode="SelectionMode.Partial" :pan-on-drag="[1,2]" :fit-view-options="{maxZoom:phoneCompact ? 0.5 : 1}" fit-view-on-init @selection-start="selectionGesture=true" @selection-end="finishSelection" @connect="onConnect" @node-drag-stop="nodeDrag" @selection-drag-stop="nodeDrag" @selection-context-menu="groupMenu" @edge-click="({ edge }) => selectedEdges = [edge.id]" @node-click="selectedEdges = []" @pane-click="selectedEdges = []"><Background :gap="24" :size="1" pattern-color="#394043" /><Controls position="bottom-left" :show-interactive="false" /></VueFlow><div class="canvas-top"><button v-if="!library" class="button small" @click="library = true"><LayoutGrid :size="14" /> Library</button><nav class="graph-breadcrumbs" aria-label="Graph path"><button class="text-button" @click="navigateGraph(null)">Root graph</button><template v-for="n in breadcrumbs" :key="n.id"><span> / </span><button class="text-button" @click="navigateGraph(n.id)">{{n.label}}</button></template></nav><span class="canvas-caption">{{ visibleNodes.length }} NODES <span>/</span> {{ visibleEdges.length }} CONNECTIONS</span><div class="canvas-tools"><button class="icon-button" :disabled="!undo.length || !graphEditable" aria-label="Undo" title="Undo (Ctrl/Cmd+Z)" @click="task(undoEdit)"><Undo2 :size="16" /></button><button class="icon-button" aria-label="Export project" @click="exportProject"><Download :size="16" /></button><label class="icon-button import-button" title="Import project JSON"><Upload :size="16" /><input type="file" accept=".json" :disabled="active || !editable" @change="importProject"></label></div></div><svg v-if="inputDrag.preview.value" style="position:fixed;inset:0;width:100vw;height:100vh;pointer-events:none;z-index:50" aria-hidden="true"><path v-for="(origin,index) in inputDrag.preview.value.origins" :key="index" :d="`M ${origin.x} ${origin.y} C ${origin.x+70} ${origin.y}, ${inputDrag.preview.value.x-70} ${inputDrag.preview.value.y}, ${inputDrag.preview.value.x} ${inputDrag.preview.value.y}`" fill="none" :stroke="origin.color" stroke-width="1.5" stroke-dasharray="6 5" /></svg><div v-if="inputDrag.preview.value" class="connection-hint">Move {{inputDrag.preview.value.ids.length}} connections to an input · Drop elsewhere to disconnect · Esc to cancel</div><div v-else-if="pendingPort" class="connection-hint">Choose a {{ pendingPort.direction === 'source' ? 'target input' : 'source output' }} · Esc to cancel</div><div class="canvas-bottom-note"><span class="tiny-dot"></span>{{ active ? 'SHOW ACTIVE · EDIT NODES AND CONNECTIONS LIVE' : 'PATCH YOUR PERFORMANCE' }}</div></div>
      </div>
      <div v-else-if="project && tab === 'score'" class="content-pane"><div class="part-tabs"><button v-for="p in project.parts" :key="p.id" :class="{ active: part?.id === p.id }" @click="selectedPart = p.id"><Music2 :size="15" />{{ p.name }}</button><button v-if="editable" :disabled="active || saving || project.parts.length >= 32" @click="addPart"><Plus :size="14" /> Add part</button></div><div class="score-actions"><button class="button small" @click="downloadXML"><Download :size="14" /> MusicXML</button><label class="button small">Import MusicXML<input type="file" accept=".xml,.musicxml" style="display:none" :disabled="active || !editable" @change="uploadXML"></label><button v-if="project.mode !== 'structured'" class="button small" :disabled="!active || stale || !canLaunchPart" @click="task(() => launchPart(true))"><Play :size="14" /> Launch part</button><button v-if="project.mode !== 'structured'" class="button small" :disabled="!active || stale || !canLaunchPart" @click="task(() => launchPart(false))"><Square :size="14" /> Stop part</button><output class="mode-pill" aria-label="Part playback status">{{ partStatus }}</output></div><div v-if="part && editable" class="part-routing"><label>Performer<select :value="part.performer || ''" :disabled="active" @change="updatePart({ ...part!, performer: ($event.target as HTMLSelectElement).value || null })"><option value="">Unassigned</option><option v-for="m in members" :key="m.id" :value="m.id">{{ m.username }}</option></select></label><label>Instrument / input<select :value="part.instrument_node || ''" :disabled="active" @change="updatePart({ ...part!, instrument_node: ($event.target as HTMLSelectElement).value || null })"><option value="">Acoustic / external only</option><option v-for="n in project.graph.nodes.filter(n => ['synth', 'fm_synth', 'browser_input', 'input'].includes(n.kind))" :key="n.id" :value="n.id">{{ n.label }}</option></select></label></div><ScoreEditor v-if="part" :part="part" :beat="partBeat" :editable="editable && !active && !saving" :beats-per-bar="project.beats_per_bar" :beat-unit="project.beat_unit || 4" @update="updatePart" @meter="updateMeter" /><p class="feature-note">Notation and piano roll edit the same notes. MusicXML import reports unsupported notation before replacing the score. Advanced engraving remains in development.</p></div>
      <div v-else-if="project && tab === 'ensemble'" class="content-pane"><div class="section-heading"><div><div class="eyebrow">PEOPLE IN THE PERFORMANCE</div><h2>Your ensemble</h2></div><span class="mode-pill">{{ members.length }} / 32 PLAYERS</span></div><div class="member-grid"><article v-for="m in members" :key="m.id" class="member-card"><span class="avatar">{{ m.username.slice(0, 2).toUpperCase() }}</span><div><h3>{{ m.username }}</h3><span>{{ m.role }}</span></div></article></div><section v-if="role === 'owner'" class="invite-panel"><h3>Invite a collaborator</h3><p>Create a single-use link valid for seven days.</p><div class="invite-controls"><select v-model="inviteRole"><option value="performer">Performer</option><option value="editor">Editor</option><option value="conductor">Conductor</option></select><button class="button primary" @click="task(invite)"><Plus :size="15" /> Create invitation</button></div><input v-if="inviteLink" :value="inviteLink" readonly aria-label="Invitation link" @focus="($event.target as HTMLInputElement).select()"></section></div>
      <div v-else-if="!project" class="empty-workspace"><Music2 :size="40" /><h2>A new space for your ensemble.</h2><button class="button primary" @click="creating = true"><Plus :size="16" /> Create a project</button></div>
      <MonitorWorkspace v-if="project" v-show="stage ? stageMonitor : tab === 'monitor'" :key="project.id" :project-id="project.id" :active="graphActive" :nodes="project.graph.nodes" :telemetry="telemetry" :hardware="hardwareLevels" :hardware-stale="!connected || now-hardwareReceived>500" :stale="stale" :age="now-receivedAt" :sample-rate="audioSettings.sample_rate" :block-size="audioSettings.block_size" :visible="stage ? stageMonitor : tab === 'monitor'" :stage="stage" />
      <footer v-if="project" class="transport-bar"><div class="transport-controls"><button class="icon-button stop-button" :disabled="!active || !conductor" aria-label="Stop" @click="task(() => transport('stop'))"><Square :size="16" fill="currentColor" /></button><button class="play-button" :disabled="!conductor || transportBusy || !!progress" :aria-label="running || countingIn ? 'Pause' : 'Play'" title="Space: activate and play / pause" @click="task(togglePlayback)"><Pause v-if="running || countingIn" :size="19" fill="currentColor" /><Play v-else :size="19" fill="currentColor" /></button><div v-if="countingIn" class="position-display count-in-position" role="status"><strong>{{ telemetry?.count_in_remaining || '→' }}</strong><small>COUNT IN</small></div><div v-else class="position-display"><strong>{{ String(Math.floor(meterBeat / project.beats_per_bar) + 1).padStart(3, '0') }}<span>:</span>{{ String(Math.floor(meterBeat % project.beats_per_bar) + 1).padStart(2, '0') }}</strong><small>BAR · BEAT</small></div></div><label class="count-in-control">Count in<select v-model="countIn" aria-label="Count in" :disabled="!conductor || running || countingIn || transportBusy" title="Clicks play through connected browser monitors before starting from the beginning"><option value="0">Off</option><option value="bar">1 bar ({{ project.beats_per_bar }} beats)</option><option v-for="beats in 32" :key="beats" :value="String(beats)">{{ beats }} {{ beats === 1 ? 'beat' : 'beats' }}</option></select></label><div class="tempo-control"><label for="tempo">TEMPO</label><input id="tempo" v-model.number="bpmDraft" type="number" min="1" max="400" :disabled="!graphActive || !conductor || !!tempoSource" :title="tempoSource ? 'Driven by ' + (project.graph.nodes.find(n=>n.id===tempoSource?.source)?.label || tempoSource.source) : 'Project tempo'" @change="task(() => transport('tempo'))"><span title="Quarter notes per minute">♩ BPM</span><div class="beat-lights"><i v-for="b in project.beats_per_bar" :key="b" :class="{ lit: running && Math.floor(meterBeat % project.beats_per_bar) === b - 1 }"></i></div></div><div class="transport-right"><span class="engine-label"><span class="status-dot" :class="{ live: graphActive && !stale }"></span>{{ graphActive ? stale ? 'ENGINE STALE' : 'ENGINE RUNNING' : 'ENGINE IDLE' }}<small>{{ audioSettings.sample_rate / 1000 }} kHz · {{ audioSettings.block_size }}-frame DSP blocks</small></span><button class="button" :disabled="!conductor||active||!!progress" @click="task(toggleEngine)">{{ graphActive ? 'Disable audio engine' : 'Enable audio engine' }}</button><button class="button" :class="{ primary: !active }" :disabled="!conductor||!!progress" @click="task(() => transport(active ? 'deactivate' : 'activate'))">{{ active ? 'Deactivate show' : 'Activate show' }}</button></div></footer>
    </template>
    <div v-if="nodeMenu" class="node-context-menu" role="menu" aria-label="Node actions" :style="{ left: `${nodeMenu.x}px`, top: `${nodeMenu.y}px` }" @keydown="nodeMenuKey"><button v-if="matchPlan" role="menuitem" :disabled="!graphEditable" @click="task(connectSelected)">Connect matching ports <kbd>C</kbd></button>
      <template v-if="nodeMenu.ids.length>1">
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
    <SystemSettings v-if="systemOpen && project" :project-id="project.id" :project="project" :saving="saving" :editable="role==='owner'" :active="!!activeId" @part="updatePart" @close="systemOpen=false" @saved="task(refreshAudio)" />
    <EngineConsole v-if="consoleOpen && project" :key="project.id" :project-id="project.id" @close="consoleOpen=false" />
    <TaskProgress v-if="progress" :title="progress" />
    <ProjectSettings v-if="settingsOpen && project" :key="project.id" :project="project" :active="active" :editable="editable" @close="settingsOpen = false" @saved="settingsSaved" />
    <NodeModal :route-target="telemetry?.route_targets?.[selected?.id??'']" :samples="projectSamples" @sample="assignSample" :input-error="telemetry?.midi_input_error" :io-status="telemetry?.node_io" @io="assignNodeIo" :parts="project?.parts" @part="assignNodePart" :project-id="project?.id" v-if="selected && selectedDescriptor && project" :visualization="telemetry?.visualizations?.[selected.id]" :sample-rate="audioSettings.sample_rate" :block-size="audioSettings.block_size" :interfaces="audioSettings.interfaces.filter(i=>i.enabled)" :node="selected" :descriptor="selectedDescriptor" :nodes="project.graph.nodes" :edges="project.graph.edges" :values="telemetry?.values[selected.id]" :stale="stale" :editable="editable" :active="graphActive" :saving="saving" @rename="renameNode" @expand="navigateGraph(selected!.id)" @close="selectedNode = null" @change="editParameter" @disconnect="disconnect" @source="id => selectedNode = id" @undo="task(undoEdit)" @remove="removeNode" @upload="file => task(() => withProgress('Importing and converting clip', () => uploadSample(file)))" @channels="nodeChannels" @control="controlValue" @curve="editCurve" />
    <div v-if="creating" class="overlay"><form class="dialog-card" @submit.prevent="createProject"><header><div><div class="eyebrow">START SOMETHING</div><h2>New performance</h2></div><button type="button" class="icon-button" aria-label="Close" @click="creating = false"><X :size="20" /></button></header><label>Project name<input v-model="newName" maxlength="120" required autofocus></label><label>Performance mode</label><label v-for="m in [{ id: 'structured', title: 'Structured', text: 'A repeatable score and a shared timeline.' }, { id: 'conducted', title: 'Conducted', text: 'One conductor, an evolving performance.' }, { id: 'freeform', title: 'Freeform', text: 'Independent players, a common pulse.' }]" :key="m.id" class="mode-choice" :class="{ chosen: newMode === m.id }"><input v-model="newMode" type="radio" :value="m.id"><div><strong>{{ m.title }}</strong><p>{{ m.text }}</p></div></label><p class="feature-note">Structured mode follows the shared score. Conducted and freeform modes also support individual part launching.</p><button class="button primary wide" :disabled="busy">{{ busy ? 'Creating…' : 'Create performance' }}<Plus :size="16" /></button></form></div>

  </div>
</template>
