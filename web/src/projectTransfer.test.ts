import { describe, expect, test } from 'vitest'
import { archiveProject, readProjectArchive } from './projectTransfer'
import type { Project } from './types'

const project = { id:'old', revision:7, name:'Portable', mode:'freeform', schema_version:1, bpm:120, beats_per_bar:4, beat_unit:4, conductor:'conductor', local_audio_assignments:{mic:'member'}, conducted:{count_in_pulses:0,pulse_unit:4,sets:[],midi_bindings:[{action:'play',target:'',source:'local',device:'x',status:176,control:1}]}, graph:{nodes:[],edges:[]}, parts:[{id:'part',name:'Part',performer:'member',view:'grid',clef:'treble',notes:[],loop_beats:4,instrument_node:null,osc_address:'/note'}] } as unknown as Project

describe('project transfer',()=>{
  test('removes workspace identity from portable archives',()=>{
    const archive=archiveProject(project)
    expect(archive.format).toBe('pr0former-project')
    expect(archive.project).toMatchObject({id:'',revision:0,conductor:null,local_audio_assignments:{}})
    expect(archive.project.parts[0].performer).toBeNull()
    expect(archive.project.conducted?.midi_bindings).toEqual([])
    expect(project.id).toBe('old')
  })
  test('accepts legacy raw project JSON',()=>{
    expect(readProjectArchive(project).name).toBe('Portable')
  })
})
