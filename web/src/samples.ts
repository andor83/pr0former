export interface SampleEntry {
  id:string; asset:number|null; name:string; description:string; tags:string; category:string; musical_key:string; bpm:number|null; global:boolean; channels:number; sample_rate:number; frames:number; duration:number; revision:number; author:string; can_edit:boolean; can_publish:boolean
}
export function matchesSample(sample:SampleEntry,search:string) { return `${sample.name} ${sample.description} ${sample.tags} ${sample.category} ${sample.musical_key} ${sample.author} ${sample.bpm??''}`.toLowerCase().includes(search.toLowerCase()) }
