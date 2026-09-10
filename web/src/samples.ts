export interface SampleEntry { can_delete?:boolean;
  id:string; asset:number|null; name:string; description:string; tags:string; category:string; musical_key:string; bpm:number|null; root_note?:number|null; global:boolean; channels:number; sample_rate:number; frames:number; duration:number; revision:number; author:string; can_edit:boolean; can_publish:boolean
}
export function matchesSample(sample:SampleEntry,search:string) { return `${sample.name} ${sample.description} ${sample.tags} ${sample.category} ${sample.musical_key} ${sample.author} ${sample.bpm??''} ${sample.root_note==null?'':midiNoteLabel(sample.root_note)}`.toLowerCase().includes(search.toLowerCase()) }

export function midiNoteLabel(note:number){return `${['C','C♯','D','D♯','E','F','F♯','G','G♯','A','A♯','B'][note%12]}${Math.floor(note/12)-1} (MIDI ${note})`}
