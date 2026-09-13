import {test,expect} from '@playwright/test'

test.afterEach(async({page})=>{const s=await(await page.request.get('/api/status')).json();if(s.active_project)await page.request.post(`/api/projects/${s.active_project}/transport`,{headers:{'X-Pr0former':'1'},data:{action:'deactivate'}})})

test('conductor editor cues, MIDI first-signal learning, cancellation, device/channel matching and set selection',async({page})=>{
  await page.addInitScript(()=>{
    const inputs=['keys-a','keys-b'].map(id=>Object.assign(new EventTarget(),{id,name:id,state:'connected'}))
    const access=Object.assign(new EventTarget(),{inputs:new Map(inputs.map(i=>[i.id,i]))})
    Object.defineProperty(navigator,'requestMIDIAccess',{value:async()=>access,configurable:true})
    ;(window as any).midi=(device:string,data:number[])=>{const event=new Event('midimessage');Object.assign(event,{data:new Uint8Array(data)});access.inputs.get(device)!.dispatchEvent(event)}
  })
  const headers={'X-Pr0former':'1'},status=await(await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap?'register':'login'}`,{headers,data:{username:'browser-test',password:'test1234'}})
  const me=await(await page.request.get('/api/me')).json()
  let project=await(await page.request.post('/api/projects',{headers,data:{name:'Conductor MIDI workflow',mode:'conducted'}})).json()
  project.conductor=me.id;project.conducted.count_in_pulses=0;project.bpm=120
  project.parts[0].loop_beats=32;project.parts[0].name='Long fragment with a readable performer label'
  project.conducted.sets=[{id:'a',name:'Opening',parts:[project.parts[0].id]},{id:'b',name:'Middle',parts:[]},{id:'c',name:'Ending',parts:[]}]
  project=await(await page.request.put(`/api/projects/${project.id}`,{headers,data:project})).json()
  const saved=async()=> (await(await page.request.get(`/api/projects/${project.id}`)).json()).project
  const midi=async(data:number[],device='keys-a')=>page.evaluate(({data,device})=>(window as any).midi(device,data),{data,device})
  let latest:any
  page.on('websocket',ws=>ws.on('framereceived',({payload})=>{const m=JSON.parse(String(payload));if(m.type==='telemetry')latest=m}))
  await page.goto('/')
  await page.getByRole('button',{name:'Conductor',exact:true}).click()
  await page.getByRole('button',{name:'Connect local MIDI',exact:true}).click()
  await expect(page.getByRole('button',{name:'Disconnect local MIDI',exact:true})).toBeVisible()
  await page.getByRole('button',{name:'Bind MIDI',exact:true}).click()
  const tile=page.locator(`[data-midi="part:${project.parts[0].id}"]`)
  await tile.click()
  await expect(page.getByRole('status').filter({hasText:'Waiting for MIDI'})).toBeVisible()
  await midi([0xf8]);await midi([0x80,60,0])
  expect((await saved()).conducted.midi_bindings).toHaveLength(0)
  await midi([0x92,60,100])
  await expect.poll(async()=>(await saved()).conducted.midi_bindings).toEqual([{action:'part',target:project.parts[0].id,source:'local',device:'keys-a',status:146,control:60}])
  await page.keyboard.press('Escape')
  await expect(page.getByRole('button',{name:'Bind MIDI',exact:true})).toBeVisible()
  // Cues work in preparation without entering the stage.
  await page.getByRole('button',{name:'Enable audio engine',exact:true}).click()
  await page.getByRole('button',{name:'Play',exact:true}).click()
  await expect.poll(()=>latest?.running).toBe(true)
  await page.getByRole('button',{name:'ARM',exact:true}).click()
  await page.getByRole('button',{name:'PLAY ARMED',exact:true}).click()
  await expect(tile).toHaveClass(/playing/)
  const fraction=()=>tile.evaluate(t=>parseFloat((t as HTMLElement).style.getPropertyValue('--progress-left')))
  const first=await fraction();await expect.poll(fraction).toBeGreaterThan(first+2)
  const dimensions=await tile.evaluate(t=>{const fill=t.querySelector('.part-progress')!,name=t.querySelector('h3')!,actions=t.querySelector('.tile-actions')!;return {left:fill.getBoundingClientRect().left-t.getBoundingClientRect().left,gap:actions.getBoundingClientRect().top-name.getBoundingClientRect().bottom}})
  expect(dimensions.left).toBeLessThan(3);expect(dimensions.gap).toBeGreaterThan(0)
  await page.screenshot({path:'/tmp/pr0-conductor-editor-midi.png'})
  // Wrong channel/device cannot toggle. A matched release followed by press can.
  await midi([0x82,60,0]);await midi([0x91,60,100]);await midi([0x92,60,100],'keys-b')
  await expect(tile).toHaveClass(/playing/)
  await midi([0x92,60,100]);await expect(tile).not.toHaveClass(/playing/)
  await midi([0x92,60,100]);await expect(tile).not.toHaveClass(/playing/)
  await midi([0x82,60,0]);await midi([0x92,60,100]);await expect(tile).toHaveClass(/playing/)
  // Escape and outside clicks both cancel an uncommitted capture.
  await page.getByRole('button',{name:'Bind MIDI',exact:true}).click();await page.locator('[data-midi="stop"]').click()
  await page.keyboard.press('Escape');await midi([0xb0,22,127]);expect((await saved()).conducted.midi_bindings).toHaveLength(1)
  await page.getByRole('button',{name:'Bind MIDI',exact:true}).click();await page.locator('[data-midi="stop"]').click()
  await page.locator('.part-board>header h2').click();await midi([0xb0,23,127]);expect((await saved()).conducted.midi_bindings).toHaveLength(1)
  await expect(page.getByRole('button',{name:'Bind MIDI',exact:true})).toBeVisible()
  // Learn a continuous set selector at zero, then traverse its full range.
  await page.getByRole('button',{name:'Bind MIDI',exact:true}).click();await page.locator('[data-midi="select_set"]').click();await midi([0xb3,7,0])
  await expect.poll(async()=>(await saved()).conducted.midi_bindings.length).toBe(2)
  await page.keyboard.press('Escape');await midi([0xb3,7,127]);await expect(page.locator('.set-button.selected')).toContainText('Ending')
  await midi([0xb3,7,64]);await expect(page.locator('.set-button.selected')).toContainText('Middle')
  await midi([0xb3,7,0]);await expect(page.locator('.set-button.selected')).toContainText('Opening')
  await page.getByRole('button',{name:'Bind MIDI',exact:true}).click();await page.locator('[data-midi="next_set"]').click();await midi([0xb0,8,127])
  await expect.poll(async()=>(await saved()).conducted.midi_bindings.length).toBe(3)
  await page.keyboard.press('Escape');await midi([0xb0,8,0]);await midi([0xb0,8,127]);await expect(page.locator('.set-button.selected')).toContainText('Middle')
  await midi([0xb0,8,127]);await expect(page.locator('.set-button.selected')).toContainText('Middle')
  await midi([0xb0,8,0]);await midi([0xb0,8,127]);await expect(page.locator('.set-button.selected')).toContainText('Ending')
  await page.getByRole('button',{name:'Stop all next pulse',exact:true}).click()
  // Learning remains available under the performance edit lock and does not cue.
  await page.getByRole('button',{name:'Perform',exact:true}).click()
  await page.getByRole('button',{name:'Bind MIDI',exact:true}).click();await page.locator('[data-midi="play"]').click();await midi([0xb1,14,127])
  await expect.poll(async()=>(await saved()).conducted.midi_bindings.length).toBe(4)
  await page.keyboard.press('Escape');await page.getByRole('button',{name:'End performance',exact:true}).click()
  // Shared bindings survive reload; the conductor's previously enabled input reconnects.
  await page.reload();await page.getByRole('button',{name:'Conductor',exact:true}).click()
  await expect(page.getByRole('button',{name:'Disconnect local MIDI',exact:true})).toBeVisible()
  await midi([0xb3,7,127]);await expect(page.locator('.set-button.selected')).toContainText('Ending')
  await page.request.post(`/api/projects/${project.id}/transport`,{headers,data:{action:'deactivate'}})
})

test('an editor learns from the designated conductor and assigns devices; other members cannot spoof input or bind',async({page,browser})=>{
  test.setTimeout(60000)
  const headers={'X-Pr0former':'1'},status=await(await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap?'register':'login'}`,{headers,data:{username:'browser-test',password:'test1234'}})
  let p=await(await page.request.post('/api/projects',{headers,data:{name:'Shared conductor MIDI',mode:'conducted'}})).json()
  const makeMember=async(role:string,name:string)=>{
    const invite=await(await page.request.post(`/api/projects/${p.id}/invite`,{headers,data:{role}})).json()
    const context=await browser.newContext()
    await context.request.post('/api/register',{headers,data:{username:name,password:'test1234',invite:invite.token}})
    await context.request.post('/api/join',{headers,data:{token:invite.token}})
    const user=await(await context.request.get('/api/me')).json()
    return {context,user}
  }
  const conductor=await makeMember('performer','midi-conductor'),editor=await makeMember('editor','midi-editor'),viewer=await makeMember('performer','midi-viewer')
  p.conductor=conductor.user.id;p.conducted.count_in_pulses=0
  p=await(await page.request.put(`/api/projects/${p.id}`,{headers,data:p})).json()
  const instrument=await conductor.context.newPage()
  await instrument.addInitScript(()=>{
    const inputs=['keys-a','keys-b'].map(id=>Object.assign(new EventTarget(),{id,name:id,state:'connected'}))
    const access=Object.assign(new EventTarget(),{inputs:new Map(inputs.map(i=>[i.id,i]))})
    Object.defineProperty(navigator,'requestMIDIAccess',{value:async()=>access})
    ;(window as any).midi=(device:string,data:number[])=>access.inputs.get(device)!.dispatchEvent(Object.assign(new Event('midimessage'),{data:new Uint8Array(data)}))
  })
  await instrument.goto('/');await instrument.getByRole('button',{name:'Conductor',exact:true}).click()
  await instrument.getByRole('button',{name:'Connect local MIDI',exact:true}).click()
  const editing=await editor.context.newPage();await editing.goto('/');await editing.getByRole('button',{name:'Conductor',exact:true}).click()
  await expect(editing.getByText('Local MIDI comes from the designated conductor.',{exact:true})).toBeVisible()
  await expect(editing.getByRole('button',{name:'Connect local MIDI',exact:true})).toHaveCount(0)
  await editing.getByRole('button',{name:'Bind MIDI',exact:true}).click()
  await editing.getByRole('combobox',{name:'MIDI binding source',exact:true}).selectOption('local')
  await expect(editing.getByRole('combobox',{name:'MIDI binding device',exact:true}).locator('option')).toHaveCount(3)
  await editing.locator('[data-midi="next_set"]').click()
  await instrument.evaluate(()=>(window as any).midi('keys-a',[0xb2,20,127]))
  const saved=async()=>(await(await page.request.get(`/api/projects/${p.id}`)).json()).project
  await expect.poll(async()=>(await saved()).conducted.midi_bindings.length).toBe(1)
  await editing.keyboard.press('Escape');await editing.getByText('Bindings (1)',{exact:true}).click()
  await editing.getByRole('combobox',{name:'Next set MIDI device',exact:true}).selectOption(JSON.stringify({source:'local',device:'keys-b'}))
  await expect.poll(async()=>(await saved()).conducted.midi_bindings[0].device).toBe('keys-b')
  // Use a separate unauthorized socket to exercise the server checks directly.
  const attack=await viewer.context.newPage();await attack.goto('/')
  const denied=await attack.evaluate(async(id)=>{
    const ws=new WebSocket(`${location.protocol==='https:'?'wss':'ws'}://${location.host}/api/projects/${id}/events`)
    const errors:string[]=[]
    return await new Promise<string[]>((resolve,reject)=>{
      const timer=setTimeout(()=>{ws.close();reject(new Error('No MIDI permission response'))},5000)
      ws.onopen=()=>{
        ws.send(JSON.stringify({type:'conductor_midi_local_devices',devices:[{id:'fake',name:'Fake'}]}))
        ws.send(JSON.stringify({type:'conductor_midi_mode',enabled:true}))
      }
      ws.onmessage=e=>{const m=JSON.parse(e.data);if(m.type==='conductor_midi_error'){errors.push(m.error);if(errors.length===2){clearTimeout(timer);ws.close();resolve(errors)}}}
    })
  },p.id)
  expect(denied).toEqual(['Only the designated conductor supplies local MIDI','Conductor or editor access required'])
  // Invalid server-device mappings are rejected even when sent as a full project.
  const invalid=await saved();invalid.conducted.midi_bindings[0].status=248
  expect((await page.request.put(`/api/projects/${p.id}`,{headers,data:invalid})).status()).toBe(400)
  expect((await viewer.context.request.put(`/api/projects/${p.id}`,{headers,data:await saved()})).status()).toBe(403)
  await conductor.context.close();await editor.context.close();await viewer.context.close()
})
