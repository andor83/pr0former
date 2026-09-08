import {test,expect} from '@playwright/test'
test('toggle persists manual state, follows input changes, and stays manually operable',async({page})=>{
  const headers={'X-Pr0former':'1'},status=await(await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap?'register':'login'}`,{headers,data:{username:'browser-test',password:'test1234'}})
  let p=await(await page.request.post('/api/projects',{headers,data:{name:'Toggle workspace',mode:'freeform'}})).json()
  const node=(id:string,kind:string,x:number,parameters={})=>({id,kind,label:id,x,y:0,channels:1,parameters})
  p.parts=[];p.graph={nodes:[node('Latch','toggle',300),node('Source','value',0,{value:0})],edges:[]}
  expect((await page.request.put(`/api/projects/${p.id}`,{headers,data:p})).ok()).toBe(true)
  const load=async()=>(await(await page.request.get(`/api/projects/${p.id}`)).json()).project
  const param=async(value:number)=>{const current=await load();expect((await page.request.put(`/api/projects/${p.id}/parameter`,{headers,data:{node:'Source',parameter:'value',value,revision:current.revision}})).ok()).toBe(true)}
  let latest:any;page.on('websocket',s=>s.on('framereceived',({payload})=>{const m=JSON.parse(String(payload));if(m.type==='telemetry')latest=m}))
  await page.goto('/')
  const checkbox=page.getByRole('checkbox',{name:'Toggle Latch',exact:true})
  await checkbox.check()
  await expect.poll(async()=>(await load()).graph.nodes[0].control_value).toBe(1)
  await page.reload();await expect(checkbox).toBeChecked()
  await page.getByRole('button',{name:'Enable audio engine',exact:true}).click()
  await expect.poll(()=>latest?.values.Latch._out).toBe(1)
  await checkbox.uncheck();await expect.poll(()=>latest?.values.Latch._out).toBe(0)
  const current=await load();current.graph.edges=[{id:'in',source:'Source',source_port:'out',target:'Latch',target_port:'in'}]
  expect((await page.request.put(`/api/projects/${p.id}`,{headers,data:current})).ok()).toBe(true)
  const revision=(await load()).revision
  await param(-2);await expect.poll(()=>latest?.values.Latch._out).toBe(1);await expect(checkbox).toBeChecked()
  expect((await load()).graph.nodes[0].control_value).toBe(0)
  expect((await load()).revision).toBe(revision+1)
  await checkbox.uncheck();await expect.poll(()=>latest?.values.Latch._out).toBe(0)
  await param(-3);await expect.poll(()=>latest?.values.Latch._out).toBe(1)
  await param(0);await expect.poll(()=>latest?.values.Latch._out).toBe(0)
  const patch=page.locator('.vue-flow__node[data-id="Latch"] .patch-node')
  expect(await patch.innerText()).toBe('')
  await checkbox.focus();await page.keyboard.press('Space');await expect.poll(()=>latest?.values.Latch._out).toBe(1)
  expect(latest.running).toBe(false)
  const saved=await load()
  expect((await page.request.put(`/api/projects/${p.id}/control`,{headers,data:{node:'Latch',revision:saved.revision,value:.5}})).status()).toBe(400)
  await page.screenshot({path:'test-results/toggle.png'})
  await page.getByRole('button',{name:'Disable audio engine',exact:true}).click()
  await expect(checkbox).toBeChecked()
})
