import {test,expect} from '@playwright/test'
const headers={'X-Pr0former':'1'}
test('local MIDI forwards CC, learns, cancels with Escape/click away, and releases on disconnect',async({page})=>{
  const status=await(await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap?'register':'login'}`,{headers,data:{username:'browser-test',password:'test1234'}})
  let p=await(await page.request.post('/api/projects',{headers,data:{name:'Local MIDI regression',mode:'freeform'}})).json()
  const n=(id:string,kind:string,x:number)=>({id,kind,label:id,x,y:0,channels:1,parameters:{}})
  p.parts=[];p.graph={nodes:[n('Local','local_midi_input',0),n('Knobs','knobs',300),n('Keys','piano',850)],edges:[{id:'cc',source:'Local',source_port:'midi',target:'Knobs',target_port:'midi'},{id:'keys',source:'Local',source_port:'midi',target:'Keys',target_port:'midi'}]}
  const saved=await page.request.put(`/api/projects/${p.id}`,{headers,data:p});expect(saved.ok()).toBe(true);p=await saved.json()
  await page.addInitScript(()=>{
    class Input extends EventTarget{ id='twister';name='MIDI Fighter Twister';state='connected';async open(){return this} }
    const input=new Input(),access=Object.assign(new EventTarget(),{inputs:new Map([['twister',input]])})
    Object.defineProperty(navigator,'requestMIDIAccess',{value:async()=>access,configurable:true})
    ;(window as any).midi=(data:number[])=>input.dispatchEvent(Object.assign(new Event('midimessage'),{data:new Uint8Array(data)}))
    ;(window as any).unplug=()=>{input.state='disconnected';access.dispatchEvent(new Event('statechange'))}
  })
  let latest:any
  page.on('websocket',socket=>{socket.on('framereceived',({payload})=>{const m=JSON.parse(String(payload));if(m.type==='telemetry')latest=m})})
  await page.goto('/')
  await page.getByRole('button',{name:'Enable audio engine',exact:true}).click()
  await page.getByRole('button',{name:'Edit Local',exact:true}).click()
  await page.getByRole('button',{name:'Detect MIDI devices',exact:true}).click()
  await page.getByLabel('Local MIDI device',{exact:true}).selectOption('twister')
  await page.getByRole('button',{name:'Connect MIDI input',exact:true}).click()
  await expect(page.getByText('Connected · forwarding MIDI to the server',{exact:true})).toBeVisible()
  const revision=(await(await page.request.get(`/api/projects/${p.id}`)).json()).project.revision
  const debug=page.getByLabel('MIDI message debug',{exact:true})
  await page.getByLabel('MIDI input activity',{exact:true}).evaluate(el=>{
    ;(window as any).midiFlashed=false
    new MutationObserver(()=>{if(el.classList.contains('lit'))(window as any).midiFlashed=true}).observe(el,{attributes:true,attributeFilter:['class']})
  })
  for(const [bytes,type,detail,raw] of [
    [[0xb3,74,100],'Control change','CC 74','B3 4A 64'],
    [[0xe3,0,96],'Pitch bend','+4096 from center','E3 00 60'],
    [[0xc3,22],'Program change','Program 22 (0–127)','C3 16'],
    [[0xd3,70],'Channel pressure','Channel aftertouch','D3 46'],
    [[0xa3,60,90],'Poly pressure','Note 60','A3 3C 5A'],
  ] as const){
    await page.evaluate(data=>(window as any).midi(data),bytes)
    const latestMessage=debug.getByLabel('Latest MIDI message',{exact:true})
    await expect(latestMessage).toContainText(type)
    await expect(latestMessage).toContainText(detail)
    await expect(latestMessage).toContainText(raw)
    await expect(debug.getByLabel(`Last MIDI ${type.toLowerCase()}`,{exact:true})).toContainText(detail)
  }
  await expect(debug.getByLabel('MIDI received count',{exact:true})).toHaveText('5')
  await expect(debug.getByLabel('Observed MIDI messages',{exact:true}).locator('tbody tr')).toHaveCount(5)
  const activity=page.getByLabel('MIDI input activity',{exact:true})
  expect(await page.evaluate(()=>(window as any).midiFlashed)).toBe(true)
  await expect(activity).toHaveText('')
  await expect(activity).not.toHaveClass(/lit/)
  await debug.getByRole('button',{name:'Clear observed MIDI history',exact:true}).click()
  await expect(debug.getByText('No observations yet.',{exact:true})).toBeVisible()
  expect((await(await page.request.get(`/api/projects/${p.id}`)).json()).project.revision).toBe(revision)
  await page.getByRole('button',{name:'Close parameters',exact:true}).click()
  const ownershipError=await page.evaluate(id=>new Promise<string>((resolve,reject)=>{
    const socket=new WebSocket(`${location.protocol==='https:'?'wss:':'ws:'}//${location.host}/api/projects/${id}/events`)
    const timeout=setTimeout(()=>{socket.close();reject(new Error('No ownership response'))},5000)
    socket.onopen=()=>socket.send(JSON.stringify({type:'local_midi_connect',node:'Local'}))
    socket.onmessage=event=>{const message=JSON.parse(event.data);if(message.type==='local_midi_status'){clearTimeout(timeout);socket.close();resolve(message.error)}}
  }),p.id)
  expect(ownershipError).toContain('another session')
  const knob=page.getByRole('slider',{name:'Knob 1',exact:true})
  await knob.click()
  await expect.poll(()=>latest?.values.Knobs._learning).toBe(1)
  await page.evaluate(()=>(window as any).midi([0xb3,74,100]))
  await expect.poll(async()=>{const q=(await(await page.request.get(`/api/projects/${p.id}`)).json()).project;return q.graph.nodes[1].parameters.controller_1}).toBe(74)
  await expect.poll(()=>latest?.values.Knobs._control_1).toBeCloseTo(100/127)
  await expect(page.getByLabel('MIDI input activity')).toHaveAttribute('title',/Control change · Ch 4 · CC 74 · 100 · B3 4A 64/)
  const before=(await(await page.request.get(`/api/projects/${p.id}`)).json()).project.revision
  for(const action of ['escape','away']){
    await knob.click();await expect.poll(()=>latest?.values.Knobs._learning).toBe(1)
    if(action==='escape')await page.keyboard.press('Escape')
    else await page.getByRole('button',{name:'Edit Local',exact:true}).click()
    await expect.poll(()=>latest?.values.Knobs._learning).toBe(0)
    await page.evaluate(()=>(window as any).midi([0xb1,12,70]))
    await expect.poll(()=>latest?.values.Local._midi_data1).toBe(12)
    expect((await(await page.request.get(`/api/projects/${p.id}`)).json()).project.revision).toBe(before)
    if(action==='away')await page.getByRole('button',{name:'Close parameters',exact:true}).click()
  }
  await knob.dblclick()
  await expect.poll(async()=>((await(await page.request.get(`/api/projects/${p.id}`)).json()).project.graph.nodes[1].parameters.channel_1)).toBe(0)
  await expect(knob).toHaveClass(/unassigned/)
  await expect.poll(()=>latest?.values.Knobs._learning).toBe(0)
  const rejected=await page.request.put(`/api/projects/${p.id}/controller`,{headers,data:{node:'Knobs',index:0,value:.1}})
  expect(rejected.ok()).toBe(false)
  const frozen=latest.values.Knobs._control_1
  await knob.focus();await page.keyboard.press('ArrowUp')
  await page.evaluate(()=>(window as any).midi([0xb3,74,12]))
  await expect.poll(()=>latest?.values.Local._midi_data2).toBe(12)
  expect(latest.values.Knobs._control_1).toBe(frozen)
  await knob.click()
  await expect.poll(()=>latest?.values.Knobs._learning).toBe(1)
  await page.evaluate(()=>(window as any).midi([0xb5,21,90]))
  await expect.poll(async()=>((await(await page.request.get(`/api/projects/${p.id}`)).json()).project.graph.nodes[1].parameters.channel_1)).toBe(6)
  await expect(knob).not.toHaveClass(/unassigned/)
  await knob.dblclick()
  await expect(knob).toHaveClass(/unassigned/)
  await page.getByRole('button',{name:'Edit Knobs',exact:true}).click()
  await page.getByRole('combobox',{name:'1 · MIDI channel',exact:true}).selectOption('3')
  await expect.poll(async()=>((await(await page.request.get(`/api/projects/${p.id}`)).json()).project.graph.nodes[1].parameters.channel_1)).toBe(3)
  await page.getByRole('button',{name:'Close parameters',exact:true}).click()
  await expect(knob).not.toHaveClass(/unassigned/)
  await page.evaluate(()=>(window as any).midi([0x93,60,100]))
  const key=page.locator('.vue-flow__node[data-id="Keys"]').getByRole('button',{name:'C4 MIDI 60',exact:true})
  await expect(key).toHaveAttribute('aria-pressed','true')
  await page.evaluate(()=>(window as any).midi([0xc3,22]))
  await expect.poll(()=>latest?.values.Local._midi_status).toBe(0xc3)
  await page.evaluate(()=>(window as any).unplug())
  await expect(key).toHaveAttribute('aria-pressed','false')
  await page.getByRole('button',{name:'Edit Local',exact:true}).click()
  await expect(page.getByRole('alert')).toContainText('MIDI device disconnected')
})
