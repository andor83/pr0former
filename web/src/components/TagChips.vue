<script setup lang="ts">
defineProps<{ tags:string[]; active?:string[]; counts?:Record<string,number>; interactive?:boolean; removable?:boolean }>()
const emit=defineEmits<{ toggle:[tag:string]; remove:[tag:string] }>()
const isActive=(active:string[]|undefined,tag:string)=>!!active?.some(t=>t.toLowerCase()===tag.toLowerCase())
</script>
<template>
  <div v-if="tags.length" class="tag-chips" role="list">
    <template v-for="tag in tags" :key="tag.toLowerCase()">
      <button v-if="interactive" type="button" role="listitem" class="tag-chip" :class="{active:isActive(active,tag)}" :aria-pressed="isActive(active,tag)" :title="isActive(active,tag)?`Stop filtering by ${tag}`:`Filter by ${tag}`" @click.stop="emit('toggle',tag)">{{tag}}<em v-if="counts?.[tag.toLowerCase()]">{{counts[tag.toLowerCase()]}}</em></button>
      <span v-else role="listitem" class="tag-chip" :class="{active:isActive(active,tag)}">{{tag}}<button v-if="removable" type="button" class="tag-remove" :aria-label="`Remove tag ${tag}`" @click.stop="emit('remove',tag)">×</button></span>
    </template>
  </div>
</template>
