<script setup lang="ts">
import { computed, ref } from 'vue'
import { formatTags, parseTags, toggleTag, type TagCount } from '../samples'
import TagChips from './TagChips.vue'
const props=defineProps<{ modelValue:string; cloud:TagCount[]; disabled?:boolean }>()
const emit=defineEmits<{ 'update:modelValue':[value:string] }>()
const tags=computed(()=>parseTags(props.modelValue))
const entry=ref('')
const applied=(tag:string)=>tags.value.some(t=>t.toLowerCase()===tag.toLowerCase())
function set(list:string[]){ emit('update:modelValue',formatTags(list)) }
function add(){ const typed=parseTags(entry.value); entry.value=''; if(typed.length)set([...tags.value,...typed]) }
function keydown(event:KeyboardEvent){
  if(event.key==='Enter'||event.key===','||(event.key==='Tab'&&entry.value.trim())){ event.preventDefault(); add() }
  else if(event.key==='Backspace'&&!entry.value&&tags.value.length){ set(tags.value.slice(0,-1)) }
}
// Tags already on this sample join the cloud even when no other sample uses them.
const cloud=computed<TagCount[]>(()=>{ const known=new Set(props.cloud.map(c=>c.tag.toLowerCase())); return [...props.cloud,...tags.value.filter(t=>!known.has(t.toLowerCase())).map(tag=>({tag,count:1}))] })
const size=computed(()=>{ const max=Math.max(2,...props.cloud.map(c=>c.count)); return (count:number)=>`${11+Math.round(8*Math.log(Math.max(1,count))/Math.log(max))}px` })
</script>
<template>
  <div class="tag-editor" :class="{disabled}">
    <TagChips :tags="tags" removable @remove="tag=>!disabled&&set(toggleTag(tags,tag))" />
    <input v-model="entry" :disabled="disabled" aria-label="Add tags" placeholder="Type a tag and press Enter, or pick from the cloud…" maxlength="64" @keydown="keydown" @blur="add">
    <div v-if="cloud.length" class="tag-cloud" role="group" aria-label="Tag cloud">
      <button v-for="c in cloud" :key="c.tag.toLowerCase()" type="button" class="tag-cloud-item" :class="{active:applied(c.tag)}" :style="{fontSize:size(c.count)}" :disabled="disabled" :aria-pressed="applied(c.tag)" :title="`${c.count} sample${c.count===1?'':'s'} · click to ${applied(c.tag)?'remove':'add'}`" @click="set(toggleTag(tags,c.tag))">{{c.tag}}</button>
    </div>
    <div class="help-section-title">Tags<HelpNote label="Tags">Click a tag in the cloud to add or remove it; larger tags are used by more samples. Tags are matched case-insensitively.</HelpNote></div>
  </div>
</template>
