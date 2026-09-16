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

export interface SampleQuery { text:string; tags:string[] }
/** `tag:a,b` tokens (case-insensitive) select samples carrying any listed tag; everything else is free text. */
export function parseSampleQuery(search:string):SampleQuery {
  const tags:string[]=[],text:string[]=[]
  for(const token of search.trim().split(/\s+/).filter(Boolean)){
    const match=/^tags?:(.*)$/i.exec(token)
    if(!match){text.push(token);continue}
    for(const tag of parseTags(match[1]))if(!tags.some(t=>t.toLowerCase()===tag.toLowerCase()))tags.push(tag)
  }
  return {text:text.join(' '),tags}
}
/** Free text and required chips must all match; with `tag:` terms a sample needs at least one of them and samples carrying more of them rank first (ties keep list order). */
export function searchSamples<T extends SampleEntry>(samples:T[],search:string,required:string[]=[]):T[] {
  const query=parseSampleQuery(search)
  return samples
    .map((sample,index)=>({sample,index,hits:query.tags.filter(tag=>hasTag(sample,tag)).length}))
    .filter(({sample,hits})=>matchesSample(sample,query.text,required)&&(!query.tags.length||hits>0))
    .sort((a,b)=>b.hits-a.hits||a.index-b.index)
    .map(({sample})=>sample)
}
/** Add or remove one tag in a query's `tag:` list, keeping the free text. */
export function toggleQueryTag(search:string,tag:string):string {
  const query=parseSampleQuery(search),tags=toggleTag(query.tags,tag)
  return [query.text,tags.length?`tag:${tags.join(',')}`:''].filter(Boolean).join(' ')
}
