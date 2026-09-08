import {test,expect} from '@playwright/test'
const headers={'X-Pr0former':'1'}
async function setup(page:any,name:string){const status=await(await page.request.get('/api/status')).json();await page.request.post(`/api/${status.bootstrap?'register':'login'}`,{headers,data:{username:'browser-test',password:'test1234'}});return(await page.request.post('/api/projects',{headers,data:{name,mode:'freeform'}})).json()}
const node=(id:string,kind:string,x:number,y:number,parameters:any={})=>({id,kind,label:id,x,y,channels:2,parameters,...(/^(send|receive)_/.test(kind)?{control_value:'main'}:{})})
const edge=(id:string,source:string,target:string,target_port:string,source_port='out')=>({id,source,target,target_port,source_port})
test('multiple controls use arrival order and upper-node priority; compact Value displays its stored setting',async({page})=>{
  const p=await setup(page,'Control merge');p.parts=[];p.graph={nodes:[node('Upper','value',0,0,{value:10}),node('Lower','value',0,250,{value:20}),node('Stored','value',400,0,{value:0}),node('Gate','toggle',200,350)],edges:[edge('lower','Lower','Stored','value'),edge('upper','Upper','Stored','value'),edge('gate','Gate','Stored','trigger')]}
  expect((await page.request.put(`/api/projects/${p.id}`,{headers,data:p})).ok()).toBe(true)
  const load=async()=>(await(await page.request.get(`/api/projects/${p.id}`)).json()).project
  const param=async(id:string,value:number)=>{const current=await load();expect((await page.request.put(`/api/projects/${p.id}/parameter`,{headers,data:{node:id,parameter:'value',value,revision:current.revision}})).ok()).toBe(true)}
  let latest:any;page.on('websocket',s=>s.on('framereceived',({payload})=>{const m=JSON.parse(String(payload));if(m.type==='telemetry')latest=m}))
  await page.goto('/');await page.getByRole('button',{name:'Enable audio engine',exact:true}).click()
  const stored=page.locator('.vue-flow__node[data-id="Stored"] .patch-node')
  await expect.poll(()=>latest?.values.Stored.value).toBe(10);expect(latest.values.Stored._out).toBe(0)
  await expect(stored.getByLabel('Stored value')).toHaveText('10');await expect(stored).toContainText('Set value');await expect(stored).toContainText('Trigger');await expect(stored).not.toContainText('Stored')
  await param('Lower',21);await expect.poll(()=>latest?.values.Stored.value).toBe(21)
  await page.getByRole('checkbox',{name:'Toggle Gate',exact:true}).check();await expect.poll(()=>latest?.values.Stored._out).toBe(21)
  await stored.focus();await page.keyboard.press('Enter')
  const modal=page.getByRole('dialog');await expect(modal.getByRole('button',{name:/Lower.*out/})).toContainText('current')
  await expect(modal.getByRole('button',{name:'Disconnect Upper from Set value',exact:true})).toBeVisible()
  await expect(modal.getByRole('button',{name:'Disconnect Lower from Set value',exact:true})).toBeVisible()
  await modal.getByRole('button',{name:'Disconnect Lower from Set value',exact:true}).click()
  await expect.poll(()=>latest?.values.Stored.value).toBe(10)
  await page.getByRole('button',{name:'Close parameters'}).click()
  await page.screenshot({path:'test-results/value-merge.png'})
  await page.getByRole('button',{name:'Disable audio engine',exact:true}).click()
})
test('named control audio and spectral sends route locally and accept text targets',async({page})=>{
  const p=await setup(page,'Named routing');p.parts=[]
  const target={...node('Target','control_input',0,500,{mode:4}),control_value:'main'}
  p.graph={nodes:[node('Value','value',0,0,{value:23}),node('Control send','send_control',250,0),node('Control receive','receive_control',550,0),node('Tone','oscillator',0,250,{frequency:440,amplitude:.2}),node('Audio send','send_audio',250,250),node('Audio receive','receive_audio',550,250),node('Meter','audio_visualizer',850,250,{size:256}),target,node('FFT','fft',0,750,{size:256,overlap:4}),node('Spectral send','send_spectral',250,750,{size:256,overlap:4}),node('Spectral receive','receive_spectral',550,750,{size:256,overlap:4}),node('Spectrum','spectral_visualizer',850,750,{size:256,overlap:4})],edges:[edge('control','Value','Control send','in'),edge('audio','Tone','Audio send','in'),edge('monitor','Audio receive','Meter','in'),edge('name','Target','Control receive','target'),edge('fft','Tone','FFT','in'),edge('sendspec','FFT','Spectral send','in'),edge('spectrum','Spectral receive','Spectrum','in')]}
  expect((await page.request.put(`/api/projects/${p.id}`,{headers,data:p})).ok()).toBe(true)
  const load=async()=>(await(await page.request.get(`/api/projects/${p.id}`)).json()).project
  let latest:any;page.on('websocket',s=>s.on('framereceived',({payload})=>{const m=JSON.parse(String(payload));if(m.type==='telemetry')latest=m}))
  await page.goto('/');await page.getByRole('button',{name:'Enable audio engine',exact:true}).click()
  await expect.poll(()=>latest?.values['Control receive']._out).toBe(23)
  await expect.poll(()=>Math.max(...(latest?.visualizations.Meter.channels[0].magnitude||[0]))).toBeGreaterThan(.01)
  await expect.poll(()=>Math.max(...(latest?.visualizations.Spectrum.channels[0].magnitude||[0]))).toBeGreaterThan(.01)
  await page.getByRole('button',{name:'Edit Control receive',exact:true}).click();await expect(page.getByLabel('Target name',{exact:true})).toBeDisabled();await expect(page.getByLabel('Target name',{exact:true})).toHaveValue('main')
  const changed=await load();const r=await page.request.put(`/api/projects/${p.id}/control`,{headers,data:{node:'Target',value:'elsewhere',revision:changed.revision}});expect(r.ok()).toBe(true)
  await expect.poll(()=>latest?.values['Control receive']._out).toBe(0);await expect(page.getByLabel('Target name',{exact:true})).toHaveValue('elsewhere')
  const invalid=await load();invalid.graph.nodes.find((n:any)=>n.id==='Control receive').control_value='manual';expect((await page.request.put(`/api/projects/${p.id}`,{headers,data:invalid})).status()).toBe(400)
  await page.screenshot({path:'test-results/named-routing.png'})
  await page.getByRole('button',{name:'Close parameters'}).click();await page.getByRole('button',{name:'Disable audio engine',exact:true}).click()
})
test('connect matching ports uses selection order and C from the context menu',async({page})=>{
  const p=await setup(page,'Match ports');p.parts=[];p.graph={nodes:[node('Left','piano',0,0),node('Right','piano',400,0)],edges:[]}
  expect((await page.request.put(`/api/projects/${p.id}`,{headers,data:p})).ok()).toBe(true)
  const load=async()=>(await(await page.request.get(`/api/projects/${p.id}`)).json()).project
  await page.goto('/')
  const left=page.locator('.vue-flow__node[data-id="Left"] .patch-node'),right=page.locator('.vue-flow__node[data-id="Right"] .patch-node')
  await right.click({position:{x:30,y:40}});await page.keyboard.down('Control');await left.click({position:{x:30,y:40}});await page.keyboard.up('Control')
  await left.click({button:'right',position:{x:30,y:40}})
  await expect(page.getByRole('menuitem',{name:/Connect matching ports/})).toBeVisible();await page.keyboard.press('c')
  await expect.poll(async()=>(await load()).graph.edges.length).toBe(5)
  expect((await load()).graph.edges.every((e:any)=>e.source==='Right'&&e.target==='Left'&&e.source_port===e.target_port)).toBe(true)
  await page.getByRole('button',{name:'Undo',exact:true}).click();await expect.poll(async()=>(await load()).graph.edges.length).toBe(0)
  await page.locator('.vue-flow__pane').click({position:{x:10,y:100}})
  const a=(await left.boundingBox())!,b=(await right.boundingBox())!
  await page.mouse.move(b.x+b.width+15,b.y-15);await page.mouse.down();await page.mouse.move(a.x-15,a.y+a.height+15,{steps:15});await page.mouse.up()
  await expect(page.locator('.vue-flow__node.selected')).toHaveCount(2)
  await page.keyboard.press('c')
  await expect.poll(async()=>(await load()).graph.edges.length).toBe(5)
  expect((await load()).graph.edges.every((e:any)=>e.source==='Left'&&e.target==='Right')).toBe(true)

})
