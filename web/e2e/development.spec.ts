import {test,expect} from '@playwright/test'
test('enabled graph runs audio and clocks while the editable show timeline stays stopped',async({page})=>{
  const headers={'X-Pr0former':'1'},status=await(await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap?'register':'login'}`,{headers,data:{username:'browser-test',password:'test1234'}})
  let p=await(await page.request.post('/api/projects',{headers,data:{name:'Development engine',mode:'structured'}})).json()
  const node=(id:string,kind:string,x:number)=>({id,kind,label:id,x,y:0,channels:2,parameters:{}})
  p.graph={nodes:[node('Tone','oscillator',0),node('Cue','monitor_output',350),node('Clock','clock',650),node('Ticks','counter',950)],edges:[{id:'audio',source:'Tone',source_port:'out',target:'Cue',target_port:'in'},{id:'ticks',source:'Clock',source_port:'pulse',target:'Ticks',target_port:'trigger'}]}
  // Resolve the clock's actual pulse outlet from the server catalog.
  const catalog=await(await page.request.get('/api/catalog')).json();p.graph.edges[1].source_port=catalog.find((d:any)=>d.kind==='clock').outputs[0].id
  p.parts[0].instrument_node=null
  p=await(await page.request.put(`/api/projects/${p.id}`,{headers,data:p})).json()
  await page.addInitScript(()=>{const Native=RTCPeerConnection;window.RTCPeerConnection=class extends Native{constructor(c?:RTCConfiguration){super(c);(window as any).__peer=this}}})
  let latest:any
  page.on('websocket',socket=>socket.on('framereceived',({payload})=>{const m=JSON.parse(String(payload));if(m.type==='telemetry')latest=m}))
  await page.route(`**/api/projects/${p.id}/media`,async route=>{
    if(route.request().method()!=='POST'){await route.continue();return}
    const body=route.request().postDataJSON()
    body.sdp=body.sdp.replace(/^(a=candidate:\S+ \d+ \S+ \d+ )\S+( \d+ typ host.*)$/gm,'$1unreachable-pr0former-test.local$2')
    expect(body.sdp).toContain('unreachable-pr0former-test.local')
    const response=await route.fetch({postData:JSON.stringify(body)})
    expect((await response.json()).sdp).toContain('a=ice-lite')
    await route.fulfill({response})
  })
  await page.goto('/')
  await page.getByRole('button',{name:'Enable audio engine',exact:true}).click()
  await expect.poll(()=>latest?.graph_beat).toBeGreaterThan(.1)
  expect(latest.running).toBe(false);expect(latest.beat).toBe(0)
  const state=await(await page.request.get('/api/status')).json();expect(state.active_project).toBeNull();expect(state.graph_project).toBe(p.id)
  await page.getByRole('button',{name:'Monitor',exact:true}).click()
  await page.getByLabel('Monitor feed').selectOption('Cue')
  await page.getByRole('button',{name:'Connect monitor',exact:true}).click()
  await expect(page.locator('.browser-monitor .mode-pill')).toHaveText('CONNECTED')
  await expect.poll(()=>page.evaluate(async()=>{let energy=0;(await(window as any).__peer.getStats()).forEach((r:any)=>{if(r.type==='inbound-rtp'&&r.kind==='audio')energy=r.totalAudioEnergy||0});return energy})).toBeGreaterThan(0)
  p=(await(await page.request.get(`/api/projects/${p.id}`)).json()).project
  p.parts[0].name='Editable while engine runs';p.parts[0].notes[0].pitch=72
  expect((await page.request.put(`/api/projects/${p.id}`,{headers,data:p})).ok()).toBe(true)
  const ticks=latest.values.Ticks._out
  await expect.poll(()=>latest?.values?.Ticks?._out).toBeGreaterThan(ticks)
  expect(latest.running).toBe(false);expect(latest.beat).toBe(0)
  await page.getByRole('button',{name:'Disable audio engine',exact:true}).click()
  await expect(page.locator('.browser-monitor .mode-pill')).toHaveText('DISCONNECTED')
  expect((await(await page.request.get('/api/status')).json()).graph_project).toBeNull()
})
