import {describe,it,expect} from 'vitest'
import {matchesProject,recentProjects} from './projectSearch'
import type {Summary} from './types'
const project=(id:string,opened?:number):Summary=>({id,name:'Étude for Ensemble',mode:'conducted',role:'owner',revision:12,bpm:137,beats_per_bar:7,beat_unit:8,owner:'Ada',opened})
describe('project metadata and recent access',()=>{
 it('searches text, tempo, meter, roles and revision without case sensitivity',()=>{
   for(const q of ['ETUDE','CONDUCTED 137','7/8','ada owner','12'])expect(matchesProject(project('a'),q)).toBe(true)
   expect(matchesProject(project('a'),'structured 137')).toBe(false)
 })
 it('takes the last five actual opens without reordering the source list',()=>{
   const all=Array.from({length:9},(_,i)=>project(String(i),i||undefined));const order=all.map(p=>p.id)
   expect(recentProjects(all).map(p=>p.id)).toEqual(['8','7','6','5','4']);expect(all.map(p=>p.id)).toEqual(order)
   expect(recentProjects([project('never')])).toEqual([])
 })
})
