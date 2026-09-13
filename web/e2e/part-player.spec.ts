import { test, expect } from '@playwright/test'

test('independent part player options, looping, retrigger, stop, and live position', async ({ page }) => {
  const headers={'X-Pr0former':'1'}
  const status=await (await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap?'register':'login'}`,{headers,data:{username:'browser-test',password:'test1234'}})
  const p=await (await page.request.post('/api/projects',{headers,data:{name:'Algorithmic snippets',mode:'structured'}})).json()
  p.mode='freeform';p.bpm=120;p.score=null
  p.parts=[{...p.parts[0],id:'phrase',name:'Two-note phrase',loop_beats:2,instrument_node:null,midi_port:null,notes:[60,64].map((pitch,i)=>({id:`n${i}`,pitch,beat:i,duration:0.7,velocity:100,rest:false,tied:false}))}]
  const node=(id:string,kind:string,x:number,y:number)=>({id,kind,label:id,x,y,channels:2,parameters:{}})
  const edge=(source:string,source_port:string,target:string,target_port:string)=>({id:`${source}-${target}-${target_port}`,source,source_port,target,target_port})
  p.graph={nodes:[node('Play','trigger',0,0),node('Repeat','trigger',0,130),node('Stop','trigger',0,260),node('Snippet','part_player',200,0),node('Sound','synth',500,0),node('Listen','monitor_output',800,0),node('Events','counter',500,450),node('MIDI destination','midi_output',800,450)],edges:[edge('Play','out','Snippet','play'),edge('Repeat','out','Snippet','repeat'),edge('Stop','out','Snippet','stop'),edge('Snippet','midi','Sound','midi'),edge('Snippet','midi','MIDI destination','midi'),edge('Snippet','trigger','Events','trigger'),edge('Sound','out','Listen','in')]}
  const url=`/api/projects/${p.id}`
  const save=await page.request.put(url,{headers,data:p});expect(save.ok(),await save.text()).toBe(true)
  const read=async()=>(await (await page.request.get(url)).json()).project
  let latest:any,peak=0
  page.on('websocket',socket=>socket.on('framereceived',({payload})=>{const event=JSON.parse(String(payload));if(event.type==='telemetry'){latest=event;peak=Math.max(peak,event.values?.Sound?._peak||0)}}))
  await page.addInitScript(()=>localStorage.setItem('pr0former.help','hidden'))
  await page.goto('/')
  await page.getByRole('button',{name:'Edit Snippet',exact:true}).click()
  await expect(page.getByLabel('Source part',{exact:true})).toHaveValue('')
  await page.getByLabel('Source part',{exact:true}).selectOption('phrase')
  await expect.poll(async()=>(await read()).graph.nodes.find((n:any)=>n.id==='Snippet').part_id).toBe('phrase')
  await page.getByRole('button',{name:'Close parameters'}).click()
  const snippet=page.locator('.vue-flow__node[data-id="Snippet"]')
  await expect(snippet.getByText('Two-note phrase',{exact:true})).toBeVisible()
  await page.getByRole('button',{name:'Enable audio engine',exact:true}).click()
  await expect.poll(()=>latest?.running).toBe(false)
  await page.getByRole('button',{name:'Trigger Play',exact:true}).click()
  await expect.poll(()=>latest?.values?.Events?._out).toBe(2)
  await expect.poll(()=>latest?.values?.Snippet?._playing).toBe(0)
  expect(latest.running).toBe(false);expect(peak).toBeGreaterThan(0.001)
  await expect(snippet.getByLabel('Part player status')).toHaveText('Stopped')
  await page.getByRole('button',{name:'Trigger Repeat',exact:true}).click()
  await expect.poll(()=>latest?.values?.Snippet?._repeating).toBe(1)
  await expect(snippet.getByLabel('Part player position')).toHaveText(/Bar 1 · Beat [12]/)
  await expect.poll(()=>latest?.values?.Events?._out).toBeGreaterThanOrEqual(5)
  // Show transport does not stop or relaunch this node.
  for(const action of ['play','pause','stop']){
    expect((await page.request.post(`${url}/transport`,{headers,data:{action,count_in_beats:0}})).ok()).toBe(true)
    await expect.poll(()=>latest?.values?.Snippet?._playing).toBe(1)
  }
  await page.getByRole('button',{name:'Trigger Play',exact:true}).click()
  await expect.poll(()=>latest?.values?.Snippet?._repeating).toBe(0)
  await expect.poll(()=>latest?.values?.Snippet?._playing).toBe(0)
  await page.getByRole('button',{name:'Trigger Repeat',exact:true}).click()
  await expect.poll(()=>latest?.values?.Snippet?._playing).toBe(1)
  await page.getByRole('button',{name:'Trigger Stop',exact:true}).click()
  await expect.poll(()=>latest?.values?.Snippet?.gate).toBe(0)
  await expect(snippet.getByLabel('Part player status')).toHaveText('Stopped')
  await page.screenshot({path:'/tmp/pr0-part-player.png'})
  const saved=await read();expect(saved.graph.nodes.find((n:any)=>n.id==='Snippet').parameters).toEqual({})
  const invalid=structuredClone(saved);invalid.graph.nodes.find((n:any)=>n.id==='Snippet').part_id='missing'
  expect((await page.request.put(url,{headers,data:invalid})).status()).toBe(400)
  await page.reload();await expect(snippet.getByText('Two-note phrase',{exact:true})).toBeVisible()
  const catalog=await (await page.request.get('/api/catalog')).json()
  expect(catalog.find((d:any)=>d.kind==='part_player').documentation.explanation).toContain('freeform')
  await page.getByRole('button',{name:'Disable audio engine',exact:true}).click()
})
