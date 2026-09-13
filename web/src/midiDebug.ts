export type MidiDebugMessage = {type:string;channel:number;detail:string;value:number;max:number;raw:string}
export function decodeMidiDebug(status:number,data1:number,data2:number):MidiDebugMessage|null {
  if(![status,data1,data2].every(Number.isInteger)||status<0x80||status>0xef||data1<0||data1>127||data2<0||data2>127)return null
  const kind=status>>4,channel=(status&15)+1
  const types:Record<number,string>={8:'Note off',9:'Note on',10:'Poly pressure',11:'Control change',12:'Program change',13:'Channel pressure',14:'Pitch bend'}
  let type=types[kind]!,detail='',value=data2,max=127
  if(kind===8||kind===9||kind===10){detail=`Note ${data1}`;if(kind===9&&data2===0)type='Note off'}
  if(kind===11)detail=`CC ${data1}`
  if(kind===12){detail=`Program ${data1} (0–127)`;value=data1}
  if(kind===13){detail='Channel aftertouch';value=data1}
  if(kind===14){value=data1+(data2<<7);max=16383;detail=`${value-8192>=0?'+':''}${value-8192} from center`}
  const bytes=[status,data1,...([12,13].includes(kind)?[]:[data2])]
  return {type,channel,detail,value,max,raw:bytes.map(byte=>byte.toString(16).padStart(2,'0').toUpperCase()).join(' ')}
}
