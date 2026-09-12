export interface SampleEntry { can_delete?:boolean;
  id:string; asset:number|null; name:string; description:string; tags:string; category:string; musical_key:string; bpm:number|null; root_note?:number|null; global:boolean; channels:number; sample_rate:number; frames:number; duration:number; revision:number; author:string; can_edit:boolean; can_publish:boolean
}
export function matchesSample(sample:SampleEntry,search:string,tags:string[]=[]) { return `${sample.name} ${sample.description} ${sample.tags} ${sample.category} ${sample.musical_key} ${sample.author} ${sample.bpm??''} ${sample.root_note==null?'':midiNoteLabel(sample.root_note)}`.toLowerCase().includes(search.toLowerCase()) && tags.every(tag=>hasTag(sample,tag)) }

export interface TagCount { tag:string; count:number }
const TAG_LIMIT=32, TAG_LENGTH=32
/** Split the stored comma-separated tag text into trimmed, de-duplicated tags (case-insensitive, first spelling wins). */
export function parseTags(tags:string|null|undefined):string[] {
  const seen=new Set<string>(),out:string[]=[]
  for(const raw of String(tags??'').split(/[,;\n]+/)){
    const tag=raw.trim().replace(/\s+/g,' ').slice(0,TAG_LENGTH),key=tag.toLowerCase()
    if(!tag||seen.has(key))continue
    seen.add(key);out.push(tag)
    if(out.length>=TAG_LIMIT)break
  }
  return out
}
export function formatTags(tags:string[]):string { return parseTags(tags.join(',')).join(', ') }
export function hasTag(sample:{tags:string},tag:string):boolean { const key=tag.trim().toLowerCase(); return parseTags(sample.tags).some(t=>t.toLowerCase()===key) }
export function toggleTag(list:string[],tag:string):string[] { const key=tag.toLowerCase(); return list.some(t=>t.toLowerCase()===key)?list.filter(t=>t.toLowerCase()!==key):[...list,tag] }
/** Every tag in use across the given samples, most used first, then alphabetical. */
export function tagCloud(samples:{tags:string}[]):TagCount[] {
  const counts=new Map<string,TagCount>()
  for(const sample of samples)for(const tag of parseTags(sample.tags)){const key=tag.toLowerCase(),entry=counts.get(key);if(entry)entry.count++;else counts.set(key,{tag,count:1})}
  return [...counts.values()].sort((a,b)=>b.count-a.count||a.tag.localeCompare(b.tag,undefined,{sensitivity:'base'}))
}

export function midiNoteLabel(note:number){return `${['C','C♯','D','D♯','E','F','F♯','G','G♯','A','A♯','B'][note%12]}${Math.floor(note/12)-1} (MIDI ${note})`}
