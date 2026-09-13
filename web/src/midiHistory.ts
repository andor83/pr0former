import {computed,ref,watch,type MaybeRefOrGetter,toValue} from 'vue'
import {decodeMidiDebug,type MidiDebugMessage} from './midiDebug'
export type MidiObservation=MidiDebugMessage&{sequence:number;arrivals:number}
export function useMidiHistory(values:MaybeRefOrGetter<Record<string,number>|undefined>,live:MaybeRefOrGetter<boolean>,limit=20){
  const history=ref<MidiObservation[]>([])
  let last=-1
  const current=computed(()=>{
    const v=toValue(values)
    return v?._midi_received?decodeMidiDebug(v._midi_status!,v._midi_data1!,v._midi_data2!):null
  })
  watch(()=>[toValue(values),toValue(live)],()=>{
    if(!toValue(live)||!current.value)return
    const sequence=toValue(values)!._midi_received!
    if(sequence===last)return
    if(sequence<last)history.value=[]
    const arrivals=last<0||sequence<last?1:sequence-last
    last=sequence
    history.value=[{...current.value,sequence,arrivals},...history.value].slice(0,limit)
  },{immediate:true})
  return {history,current}
}
