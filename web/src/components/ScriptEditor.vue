<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount, ref, watch } from 'vue'
import { EditorState, Compartment } from '@codemirror/state'
import { EditorView, keymap, lineNumbers, highlightActiveLine, drawSelection } from '@codemirror/view'
import { javascript } from '@codemirror/lang-javascript'
import { autocompletion, completionKeymap } from '@codemirror/autocomplete'
import type { CompletionContext } from '@codemirror/autocomplete'
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands'
import { bracketMatching, foldGutter, syntaxHighlighting, HighlightStyle } from '@codemirror/language'
import { tags } from '@lezer/highlight'
import { api } from '../api'
import type { GraphNode, GraphEdge, ScriptConfig, ScriptStatus } from '../types'
import { starterScript, examples, completions, memberCompletions, removedScriptEdges } from '../scripting'

const props=defineProps<{projectId:string;node:GraphNode;edges:GraphEdge[];editable:boolean;saving:boolean;active:boolean;stale:boolean;status?:ScriptStatus;values?:Record<string,number>;apply:(script:ScriptConfig,nodeId:string,projectId:string)=>Promise<void>}>()
const host=ref<HTMLElement>(),draft=ref(props.node.script?.source??starterScript),error=ref(''),message=ref(''),busy=ref(false),checked=ref<ScriptConfig>(),example=ref('')
const dirty=computed(()=>draft.value!==(props.node.script?.source??''))
const removed=computed(()=>checked.value?removedScriptEdges(props.edges,props.node.id,checked.value).length:0)
function inputSources(name:string){return props.edges.filter(e=>e.target===props.node.id&&e.target_port===name).map(e=>`${e.source}.${e.source_port}`).join(', ')}
const readOnly=new Compartment()
let view:EditorView|undefined
function complete(ctx:CompletionContext) {
  const member=ctx.matchBefore(/\b\w+\.\w*/)
  if(member){const [object='']=member.text.split('.');return {from:member.from+object.length+1,options:memberCompletions[object]??['read','write','on','off','once','at_beat','at_sample','pulse'].map(label=>({label,type:'function'}))}}
  const word=ctx.matchBefore(/\w*/);if(!word||(!ctx.explicit&&word.from===word.to))return null
  return {from:word.from,options:completions}
}
onMounted(()=>{view=new EditorView({parent:host.value,state:EditorState.create({doc:draft.value,extensions:[
  lineNumbers(),highlightActiveLine(),drawSelection(),history(),bracketMatching(),foldGutter(),javascript(),
  keymap.of([{key:'Mod-Enter',run:()=>{void apply();return true;}},...completionKeymap,...defaultKeymap,...historyKeymap,indentWithTab]),
  autocompletion({override:[complete]}),readOnly.of(EditorState.readOnly.of(!props.editable||props.saving)),
  EditorView.contentAttributes.of({'aria-label':'JavaScript source',spellcheck:'false'}),
  EditorView.updateListener.of(update=>{if(update.docChanged){draft.value=update.state.doc.toString();checked.value=undefined;error.value='';message.value='';}}),
  syntaxHighlighting(HighlightStyle.define([{tag:tags.keyword,color:'#dfacff'},{tag:tags.string,color:'#b9df93'},{tag:tags.number,color:'#ffc477'},{tag:tags.comment,color:'#859a9d',fontStyle:'italic'},{tag:tags.function(tags.variableName),color:'#75dfe1'},{tag:tags.definition(tags.variableName),color:'#d8e8eb'}])),
  EditorView.theme({'&':{backgroundColor:'#182123',color:'#e0eceb',fontSize:'13px'},'.cm-scroller':{fontFamily:'ui-monospace, SFMono-Regular, Menlo, monospace',overflow:'auto'},'.cm-content':{minHeight:'280px'},'.cm-gutters':{backgroundColor:'#202a2c',color:'#899d9f',border:'none'},'.cm-activeLine':{background:'#86dddd0a'},'.cm-cursor':{borderLeftColor:'#a3ebeb'},'.cm-tooltip':{backgroundColor:'#293537',color:'#e0eceb',border:'1px solid #53696b'},'.cm-selectionBackground':{backgroundColor:'#37565b !important'}},{dark:true})
  ]})})})
watch(()=>[props.editable,props.saving,busy.value],()=>view?.dispatch({effects:readOnly.reconfigure(EditorState.readOnly.of(!props.editable||props.saving||busy.value))}))
watch(()=>props.node.script?.source,(source,previous)=>{if(source===draft.value){message.value='Applied';return;}if(draft.value===(previous??'')){replace(source??'');}else message.value='Saved source changed elsewhere. Your local draft is retained; review before applying.'})
function replace(source:string){view?.dispatch({changes:{from:0,to:view.state.doc.length,insert:source}});draft.value=source}
function insertExample(){if(!example.value)return;replace(examples[example.value]!);example.value=''}
async function check(){error.value='';message.value='';try{checked.value=await api<ScriptConfig>(`/projects/${props.projectId}/scripts/compile`,'POST',{source:draft.value});message.value='Compile successful';return checked.value;}catch(e){error.value=String(e);return undefined}}
async function validate(){if(busy.value||!props.editable)return;busy.value=true;try{await check();}finally{busy.value=false}}
async function apply(){if(busy.value||props.saving||!props.editable)return;const nodeId=props.node.id,projectId=props.projectId;busy.value=true;try{const script=await check();if(script){await props.apply(script,nodeId,projectId);message.value='Applied · runtime restarted';}}catch(e){error.value=String(e);}finally{busy.value=false}}
function openGuide(){window.open('/?help=1&topic=scripting','_blank','noopener')}
function openConsole(){window.open(`/?console=${encodeURIComponent(props.projectId)}`,'_blank','noopener')}
onBeforeUnmount(()=>view?.destroy())
</script>
<template>
  <section class="script-editor" aria-label="JavaScript editor" @keydown.stop>
    <div class="script-toolbar"><select aria-label="Script examples" v-model="example" :disabled="!editable||busy||saving" @change="insertExample"><option value="">Replace draft with example…</option><option v-for="(_,name) in examples" :key="name">{{name}}</option></select><button type="button" class="button small" @click="openGuide">Scripting guide</button><button type="button" class="button small" @click="openConsole">Console</button></div>
    <p class="feature-note">{{dirty?'Unapplied draft':'Saved source'}} · Ctrl/Cmd+Space completes helpers · Ctrl/Cmd+Enter applies · Escape then Tab leaves the editor.</p>
    <div ref="host" class="script-code" />
    <div class="script-toolbar"><button type="button" class="button small" :disabled="!editable||busy||saving" @click="validate">Check script</button><button type="button" class="button primary" :disabled="!editable||busy||saving" @click="apply">{{busy?'Compiling…':'Apply script'}}</button><span role="status">{{message}}</span></div>
    <p v-if="removed" class="feature-note">Applying this port definition removes {{removed}} cable(s) attached to deleted or renamed ports.</p>
    <p v-if="checked" class="feature-note">Checked inputs: {{checked.inputs.map(p=>p.name).join(', ')||'none'}} · outputs: {{checked.outputs.map(p=>p.name).join(', ')||'none'}} · MIDI input/output always available.</p>
    <pre v-if="error" class="script-error" role="alert">{{error}}</pre>
    <p class="feature-note">{{active?'Engine enabled · reactive responses have worker latency.':'Enable the engine to run this script.'}} Applying restarts this node’s local state. Compile errors keep its running version.</p>
    <section class="script-ports" v-if="node.script"><div><h3>Inputs</h3><p v-for="(port,i) in node.script.inputs" :key="port.name"><code>{{port.name}}</code> <output>{{active&&!stale?values?.[`_script_input_${i}`]??'—':inputSources(port.name)?'—':port.initial}}</output><span v-if="inputSources(port.name)"> from {{inputSources(port.name)}} · read-only</span></p></div><div><h3>Outputs</h3><p v-for="(port,i) in node.script.outputs" :key="port.name"><code>{{port.name}}</code> <output>{{active&&!stale?values?.[`_script_output_${i}`]??'—':'—'}}</output></p></div></section>
    <section v-if="active&&status" class="script-runtime"><p>{{stale?'Status stale':status.faulted?'Runtime stopped':'Runtime running'}} · {{status.events}} events · {{status.dropped}} dropped · {{status.late}} late outputs</p><pre v-if="status.error" class="script-error" role="alert">{{status.error}}</pre><div role="log" class="script-log" aria-label="Script console"><p v-for="(entry,i) in status.logs.slice(-12)" :key="i" :class="entry.level"><span>{{entry.sample}} · {{entry.level}}</span> {{entry.message}}</p></div></section>
  </section>
</template>
<style scoped>
.script-toolbar{display:flex;gap:8px;align-items:center;flex-wrap:wrap;margin:10px 0}.script-toolbar select{flex:1;min-width:180px}.script-code{border:1px solid #435456;border-radius:6px;overflow:hidden}.script-code :deep(.cm-editor){max-height:440px}.script-error{white-space:pre-wrap;color:#ffb0a5;background:#45282b;padding:12px;border-radius:5px;overflow-wrap:anywhere}.script-log{max-height:180px;overflow:auto;background:#182123;border-radius:5px;padding:8px;font:12px ui-monospace,monospace;white-space:pre-wrap;overflow-wrap:anywhere}.script-log p{margin:4px 0}.script-log span{color:#96a9aa}.script-log .error{color:#ffb0a5}.script-log .warn{color:#ffc477}.script-ports{display:grid;grid-template-columns:1fr 1fr;gap:16px}.script-ports h3{font-size:12px}.script-ports p{font-size:12px}.script-ports output{color:var(--amber);margin-left:8px}.script-runtime>p{font-size:12px;color:var(--muted)}
</style>
