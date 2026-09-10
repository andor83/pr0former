import type {Summary} from './types'
export function matchesMetadata(value:unknown,query:string){
  const normalize=(s:string)=>s.normalize('NFKD').replace(/[\u0300-\u036f]/g,'').toLowerCase()
  const text=normalize(JSON.stringify(value))
  return normalize(query).trim().split(/\s+/).every(word=>text.includes(word))
}
export function matchesProject(p:Summary,query:string){return matchesMetadata({...p,tempo:`${p.bpm} BPM`,meter:`${p.beats_per_bar}/${p.beat_unit}`},query)}
export function recentProjects(projects:Summary[]){return projects.filter(p=>p.opened!=null).sort((a,b)=>b.opened!-a.opened!).slice(0,5)}
