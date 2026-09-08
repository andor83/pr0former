import {test,expect} from '@playwright/test'
test('pitch tracker detects audio, displays ranked slots, connects MIDI numbers and removes disabled outputs',async({page})=>{
  const headers={'X-Pr0former':'1'},status=await(await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap?'register':'login'}`,{headers,data:{username:'browser-test',password:'test1234'}})
  const p=await(await page.request.post('/api/projects',{headers,data:{name:'Pitch tracking',mode:'freeform'}})).json()
  const n=(id:string,kind:string,x:number,parameters={})=>({id,kind,label:id,x,y:0,channels:2,parameters})
  p.parts=[];p.graph={nodes:[n('Tone','oscillator',0,{frequency:440,amplitude:.5}),n('Tracker','pitch_tracker',350),n('Result','value',850)],edges:[{id:'audio',source:'Tone',source_port:'out',target:'Tracker',target_port:'in'},{id:'note',source:'Tracker',source_port:'pitch1',target:'Result',target_port:'value'}]}
  expect((await page.request.put(`/api/projects/${p.id}`,{headers,data:p})).ok()).toBe(true)
  const load=async()=>(await(await page.request.get(`/api/projects/${p.id}`)).json()).project
  let latest:any;page.on('websocket',s=>s.on('framereceived',({payload})=>{const m=JSON.parse(String(payload));if(m.type==='telemetry')latest=m}))
  await page.goto('/');const tracker=page.locator('.vue-flow__node[data-id="Tracker"]')
  await expect(tracker.getByLabel('Pitch 1 MIDI note',{exact:true})).toHaveText('—')
  await page.getByRole('button',{name:'Enable audio engine',exact:true}).click()
  await expect(tracker.getByLabel('Pitch 1 MIDI note',{exact:true})).toHaveText('69')
  await expect.poll(()=>latest?.values.Result.value).toBe(69)
  expect(latest.running).toBe(false)
  await page.getByRole('button',{name:'Edit Tracker',exact:true}).click()
  await page.getByLabel('Pitch slots',{exact:true}).selectOption('4')
  await expect(page.getByLabel('FFT size',{exact:true})).toHaveValue('8192')
  await page.getByRole('button',{name:'Close parameters',exact:true}).click()
  await expect(tracker.locator('.tracker-slot')).toHaveCount(4)
  await expect(tracker.getByLabel('Pitch 4 MIDI note',{exact:true})).toHaveText('—')
  const boxes=await tracker.locator('.tracker-slot').evaluateAll(els=>els.map(el=>el.getBoundingClientRect().x))
  expect(boxes.every((x,i)=>i===0||x>boxes[i-1]!)).toBe(true)
  const chord=await load();chord.graph.nodes[0].channels=1;chord.graph.nodes[0].parameters.amplitude=.8
  chord.graph.nodes.push({...n('Merge','channel_merge',0),y:350,channels:4})
  for(const [i,frequency,amplitude] of [[2,329.6276,.6],[3,261.6256,.4],[4,391.9954,.2]]){
    chord.graph.nodes.push({...n(`Tone${i}`,'oscillator',-300,{frequency,amplitude}),y:(i!-1)*200,channels:1})
    chord.graph.edges.push({id:`tone${i}`,source:`Tone${i}`,source_port:'out',target:'Merge',target_port:`ch_${i}`})
  }
  chord.graph.edges[0].source='Merge'
  chord.graph.edges.push({id:'tone1',source:'Tone',source_port:'out',target:'Merge',target_port:'ch_1'})
  expect((await page.request.put(`/api/projects/${p.id}`,{headers,data:chord})).ok()).toBe(true)
  for(const [i,note] of [69,64,60,67].entries())await expect(tracker.getByLabel(`Pitch ${i+1} MIDI note`,{exact:true})).toHaveText(String(note))
  const graph=await load();graph.graph.edges[1].source_port='pitch4'
  expect((await page.request.put(`/api/projects/${p.id}`,{headers,data:graph})).ok()).toBe(true)
  await expect.poll(()=>latest?.values.Result.value).toBe(67)
  await page.screenshot({path:'test-results/pitch-tracker.png'})
  await page.getByRole('button',{name:'Edit Tracker',exact:true}).click();await page.getByLabel('Pitch slots',{exact:true}).selectOption('1')
  await expect.poll(async()=>(await load()).graph.edges.some((e:any)=>e.id==='note')).toBe(false)
  await page.getByRole('button',{name:'Close parameters',exact:true}).click()
  const invalid=await load();invalid.graph.edges.push({id:'bad',source:'Tracker',source_port:'pitch4',target:'Result',target_port:'value'})
  expect((await page.request.put(`/api/projects/${p.id}`,{headers,data:invalid})).status()).toBe(400)
  const current=await load();for(const node of current.graph.nodes)if(node.kind==='oscillator')node.parameters.amplitude=0
  expect((await page.request.put(`/api/projects/${p.id}`,{headers,data:current})).ok()).toBe(true)
  await expect(tracker.getByLabel('Pitch 1 MIDI note',{exact:true})).toHaveText('—')
  await page.getByRole('button',{name:'Disable audio engine',exact:true}).click()
})
