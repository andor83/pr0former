<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { Copy, Link2, Plus, Trash2, UserPlus, Users } from '@lucide/vue'
import { api } from '../api'
import type { Member, Project } from '../types'

const props=defineProps<{project:Project;members:Member[];userId:string;owner:boolean}>()
const emit=defineEmits<{refresh:[];project:[project:Project];error:[error:unknown]}>()
const selectedId=ref(''),query=ref(''),candidates=ref<Member[]>([]),candidateId=ref(''),addRole=ref('performer'),inviteRole=ref('performer'),inviteLink=ref(''),busy=ref(false)
const selected=computed(()=>props.members.find(member=>member.id===selectedId.value)||props.members[0])
const assignedParts=computed(()=>props.project.parts.filter(part=>part.performer===selected.value?.id))
const fullName=computed(()=>[selected.value?.first_name,selected.value?.last_name].filter(Boolean).join(' '))
function avatar(member?:Member){return member?.avatar_revision?`/api/users/${member.id}/avatar?v=${member.avatar_revision}`:''}
async function findUsers(){if(!props.owner)return;try{candidates.value=await api(`/projects/${props.project.id}/members/candidates?q=${encodeURIComponent(query.value)}`);if(!candidates.value.some(user=>user.id===candidateId.value))candidateId.value=candidates.value[0]?.id||''}catch(error){emit('error',error)}}
let searchTimer:ReturnType<typeof setTimeout>|undefined
watch(query,()=>{clearTimeout(searchTimer);searchTimer=setTimeout(findUsers,180)})
watch(()=>props.members, members=>{if(!members.some(member=>member.id===selectedId.value))selectedId.value=members[0]?.id||''},{immediate:true})
onMounted(findUsers)
async function addMember(){if(!candidateId.value)return;busy.value=true;try{await api(`/projects/${props.project.id}/members`,'POST',{user_id:candidateId.value,role:addRole.value});emit('refresh');await findUsers()}catch(error){emit('error',error)}finally{busy.value=false}}
async function invite(){busy.value=true;try{const result=await api<{path:string}>(`/projects/${props.project.id}/invite`,'POST',{role:inviteRole.value});inviteLink.value=location.origin+result.path}catch(error){emit('error',error)}finally{busy.value=false}}
async function copyInvite(){if(inviteLink.value)await navigator.clipboard.writeText(inviteLink.value)}
async function removeMember(member=selected.value){if(!member||member.role==='owner'||!confirm(`Remove ${member.username} from this project? Their assigned parts will become unassigned.`))return;busy.value=true;try{const result=await api<{project:Project;unassigned_parts:number}>(`/projects/${props.project.id}/members/${member.id}`,'DELETE');emit('project',result.project);emit('refresh')}catch(error){emit('error',error)}finally{busy.value=false}}
async function designateConductor(){const member=selected.value;if(!member||assignedParts.value.length)return;busy.value=true;try{const next=JSON.parse(JSON.stringify(props.project)) as Project;next.conductor=next.conductor===member.id?null:member.id;const saved=await api<Project>(`/projects/${props.project.id}`,'PUT',next);emit('project',saved)}catch(error){emit('error',error)}finally{busy.value=false}}

</script>

<template>
  <section class="ensemble-workspace">
    <aside class="ensemble-members">
      <header><div><div class="eyebrow">PROJECT ENSEMBLE</div><h2>Members</h2></div><span class="mode-pill">{{members.length}} / 32</span></header>
      <div class="ensemble-member-list">
        <div v-for="member in members" :key="member.id" class="ensemble-member-row" :class="{selected:member.id===selected?.id}">
          <button class="ensemble-member-select" @click="selectedId=member.id"><span class="member-avatar"><img v-if="avatar(member)" :src="avatar(member)" alt=""><span v-else>{{member.username.slice(0,2).toUpperCase()}}</span></span><span><strong>{{[member.first_name,member.last_name].filter(Boolean).join(' ')||member.username}}</strong><small>@{{member.username}} · {{member.role}}</small></span></button>
          <button v-if="owner&&member.role!=='owner'" class="icon-button danger ensemble-member-remove" :aria-label="`Remove ${member.username} from project`" title="Remove from project" :disabled="busy" @click="removeMember(member)"><Trash2 :size="15"/></button>
        </div>
      </div>
    </aside>
    <div v-if="selected" class="ensemble-detail">
      <header class="member-overview-header">
        <span class="member-avatar large"><img v-if="avatar(selected)" :src="avatar(selected)" alt=""><span v-else>{{selected.username.slice(0,2).toUpperCase()}}</span></span>
        <div><div class="eyebrow">ENSEMBLE MEMBER</div><h2>{{fullName||selected.username}}</h2><p>@{{selected.username}}</p></div>
        <span class="member-role">{{selected.role}}</span>
      </header>
      <div class="member-summary">
        <article><small>PROJECT ROLE</small><strong>{{selected.role}}</strong></article>
        <article><small>ASSIGNED PARTS</small><strong>{{assignedParts.length}}</strong></article>
        <article><small>ORGANIZATION</small><strong>{{selected.organization||'—'}}</strong></article>
      </div>
      <section class="member-parts"><h3>Assigned Parts</h3><div v-if="assignedParts.length" class="assigned-part-list"><span v-for="part in assignedParts" :key="part.id">{{part.name}}</span></div><p v-else>No parts are currently assigned to this member.</p></section>
      <section v-if="owner&&project.mode==='conducted'" class="conductor-assignment"><div><h3>Performance conductor</h3><p>The designated conductor receives the cue interface and cannot hold performer parts.</p></div><button class="button" :class="{primary:project.conductor===selected.id}" :disabled="busy||!!assignedParts.length" @click="designateConductor">{{project.conductor===selected.id?'Remove designation':'Designate conductor'}}</button></section>

      <button v-if="owner&&selected.role!=='owner'" class="button danger remove-member" :disabled="busy" @click="removeMember()"><Trash2 :size="15"/> Remove from Project</button>
      <section v-if="owner" class="ensemble-management">
        <div class="management-heading"><div><div class="eyebrow">MEMBERSHIP</div><h3>Add an Existing User</h3></div><UserPlus :size="20"/></div>
        <label>Find user<input v-model="query" autocomplete="off" placeholder="Search usernames…"></label>
        <div class="member-add-controls"><select v-model="candidateId" aria-label="Existing user"><option value="" disabled>{{candidates.length?'Select a user':'No matching users'}}</option><option v-for="candidate in candidates" :key="candidate.id" :value="candidate.id">{{candidate.username}}{{candidate.organization?` · ${candidate.organization}`:''}}</option></select><select v-model="addRole" aria-label="Project role"><option value="performer">Performer</option><option value="editor">Editor</option><option value="conductor">Conductor</option></select><button class="button primary" :disabled="busy||!candidateId||members.length>=32" @click="addMember"><Plus :size="15"/> Add Member</button></div>
      </section>
      <section v-if="owner" class="ensemble-management invite-management">
        <div class="management-heading"><div><div class="eyebrow">INVITATION LINK</div><h3>Invite a New Collaborator</h3></div><Link2 :size="20"/></div><p>Create a single-use link valid for seven days. New users can create an account while joining this project.</p>
        <div class="member-add-controls"><select v-model="inviteRole" aria-label="Invitation role"><option value="performer">Performer</option><option value="editor">Editor</option><option value="conductor">Conductor</option></select><button class="button" :disabled="busy" @click="invite"><Users :size="15"/> Create Invitation</button></div>
        <div v-if="inviteLink" class="invite-result"><input :value="inviteLink" readonly aria-label="Invitation link" @focus="($event.target as HTMLInputElement).select()"><button class="icon-button" title="Copy invitation" aria-label="Copy invitation" @click="copyInvite"><Copy :size="16"/></button></div>
      </section>
    </div>
  </section>
</template>

<style scoped>
.ensemble-workspace{display:grid;grid-template-columns:280px minmax(0,1fr);min-height:100%;margin:-32px -42px;background:#141b1c}.ensemble-members{border-right:1px solid var(--line);background:#171e20;padding:25px 14px}.ensemble-members>header{display:flex;align-items:center;justify-content:space-between;padding:0 8px 18px}.ensemble-members h2{font-size:20px;margin-top:3px}.ensemble-member-list{display:flex;flex-direction:column;gap:4px}.ensemble-member-row{display:flex;align-items:center;border:1px solid transparent;border-radius:7px}.ensemble-member-row:hover{background:#20292b}.ensemble-member-row.selected{background:#243032;border-color:#405154}.ensemble-member-select{display:flex;align-items:center;gap:11px;min-width:0;flex:1;padding:10px;text-align:left}.ensemble-member-select>span:last-child{min-width:0}.ensemble-member-remove{flex:0 0 32px;width:32px;height:32px;margin-right:5px}.ensemble-member-list strong{display:block;overflow:hidden;text-overflow:ellipsis;font-size:12px;white-space:nowrap}.ensemble-member-list small{display:block;margin-top:3px;color:var(--muted);font-size:9px;text-transform:capitalize}.member-avatar{display:grid;place-items:center;width:36px;height:36px;flex:0 0 36px;overflow:hidden;border:1px solid #405653;border-radius:7px;background:#263b37;color:#b7d8c6;font-size:10px}.member-avatar img{width:100%;height:100%;object-fit:cover}.member-avatar.large{width:72px;height:72px;flex-basis:72px;border-radius:12px;font-size:18px}.ensemble-detail{padding:34px 40px;overflow:auto}.member-overview-header{display:flex;align-items:center;gap:18px;padding-bottom:26px;border-bottom:1px solid var(--line)}.member-overview-header h2{margin-top:4px}.member-overview-header p{color:var(--muted);font-size:11px}.member-role{margin-left:auto;padding:6px 9px;border:1px solid #49615d;border-radius:4px;color:var(--cyan);font-size:9px;letter-spacing:1px;text-transform:uppercase}.member-summary{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:12px;margin:24px 0}.member-summary article{padding:16px;border:1px solid var(--line);border-radius:7px;background:#192123}.member-summary small{display:block;color:var(--muted);font-size:8px;letter-spacing:1.2px}.member-summary strong{display:block;margin-top:8px;font-size:13px;text-transform:capitalize}.member-parts{padding:20px 0;border-top:1px solid var(--line)}.member-parts p,.avatar-upload p,.ensemble-management p{margin-top:7px;color:var(--muted);font-size:11px}.assigned-part-list{display:flex;flex-wrap:wrap;gap:7px;margin-top:12px}.assigned-part-list span{padding:6px 9px;border:1px solid #405154;border-radius:4px;background:#20292b;font-size:10px}.avatar-upload{display:flex;align-items:center;justify-content:space-between;gap:20px;padding:20px 0;border-top:1px solid var(--line)}.avatar-upload input{display:none}.avatar-upload label.disabled{opacity:.4;pointer-events:none}.remove-member{margin:8px 0 26px;border-color:#694442;background:#322423}.ensemble-management{max-width:780px;margin-top:18px;padding:20px;border:1px solid var(--line);border-radius:8px;background:#192123}.management-heading{display:flex;align-items:center;justify-content:space-between;color:var(--cyan)}.management-heading h3{margin-top:3px;color:var(--white)}.ensemble-management>label{display:block;margin:17px 0 8px;color:#a9b8b9;font-size:10px}.ensemble-management>label input{display:block;width:100%;margin-top:7px}.member-add-controls{display:flex;align-items:center;gap:9px}.member-add-controls select:first-child{flex:1}.invite-management .member-add-controls{margin-top:15px}.invite-result{display:flex;gap:7px;margin-top:12px}.invite-result input{flex:1}.invite-result .icon-button{border:1px solid var(--line)}
.conductor-assignment{display:flex;align-items:center;justify-content:space-between;gap:20px;padding:20px 0;border-top:1px solid var(--line)}.conductor-assignment p{margin-top:7px;color:var(--muted);font-size:11px}
@media(max-width:850px){.ensemble-workspace{grid-template-columns:220px minmax(0,1fr);margin:-25px}.ensemble-detail{padding:26px 24px}.member-add-controls{align-items:stretch;flex-direction:column}.member-add-controls>*{width:100%}.member-summary{grid-template-columns:1fr}}
@media(max-width:600px){.ensemble-workspace{display:block;margin:-12px -8px}.ensemble-members{border-right:0;border-bottom:1px solid var(--line);padding:14px 8px}.ensemble-member-list{flex-direction:row;overflow-x:auto}.ensemble-member-row{flex:0 0 200px}.ensemble-detail{padding:22px 14px}.member-overview-header{align-items:flex-start}.member-avatar.large{width:56px;height:56px;flex-basis:56px}.member-role{display:none}.avatar-upload{align-items:flex-start;flex-direction:column}}
</style>
