import {expect,test} from 'vitest'
import {decodeMidiDebug as decode} from './midiDebug'
test('decodes CC, channel, pressure and exact MIDI bytes',()=>{
  expect(decode(0xb3,74,100)).toEqual({type:'Control change',channel:4,detail:'CC 74',value:100,max:127,raw:'B3 4A 64'})
  expect(decode(0xc0,10,0)).toMatchObject({type:'Program change',value:10,raw:'C0 0A'})
  expect(decode(0xd2,64,0)).toMatchObject({type:'Channel pressure',channel:3,value:64,raw:'D2 40'})
  expect(decode(0xa0,60,90)).toMatchObject({type:'Poly pressure',detail:'Note 60',value:90})
  expect(decode(0x90,60,0)?.type).toBe('Note off')
})
test('shows full 14-bit pitch bend with signed center offset',()=>{
  expect(decode(0xe0,0,0)).toMatchObject({value:0,detail:'-8192 from center',max:16383})
  expect(decode(0xe0,0,64)).toMatchObject({value:8192,detail:'+0 from center'})
  expect(decode(0xef,127,127)).toMatchObject({value:16383,detail:'+8191 from center',channel:16})
})
test('rejects malformed or unsupported messages',()=>{
  for(const data of [[0,0,0],[0xf0,1,2],[0xb0,128,0],[0x90,60,-1],[NaN,0,0],[0x90,60.5,1]])expect(decode(data[0]!,data[1]!,data[2]!)).toBeNull()
})
