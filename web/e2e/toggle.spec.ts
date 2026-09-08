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
  await expect.poll(()=>latest?.values.Latch._checked).toBe(1)
  await checkbox.uncheck();await expect.poll(()=>latest?.values.Latch._checked).toBe(0)
  const current=await load();current.graph.edges=[{id:'in',source:'Source',source_port:'out',target:'Latch',target_port:'in'}]
  expect((await page.request.put(`/api/projects/${p.id}`,{headers,data:current})).ok()).toBe(true)
  const revision=(await load()).revision
  await param(2);await expect.poll(()=>latest?.values.Latch._checked).toBe(1);await expect(checkbox).toBeChecked()
  expect((await load()).graph.nodes[0].control_value).toBe(0)
  expect((await load()).revision).toBe(revision+1)
  await checkbox.uncheck();await expect.poll(()=>latest?.values.Latch._checked).toBe(0)
  await param(3);await expect.poll(()=>latest?.values.Latch._checked).toBe(1)
  await param(-1);await expect.poll(()=>latest?.values.Latch._checked).toBe(0)
  await param(0);await expect.poll(()=>latest?.values.Latch._checked).toBe(0)
  const patch=page.locator('.vue-flow__node[data-id="Latch"] .patch-node')
  expect(await patch.innerText()).toBe('')
  await checkbox.focus();await page.keyboard.press('Space');await expect.poll(()=>latest?.values.Latch._checked).toBe(1)
  expect(latest.running).toBe(false)
  const saved=await load()
  expect((await page.request.put(`/api/projects/${p.id}/control`,{headers,data:{node:'Latch',revision:saved.revision,value:.5}})).status()).toBe(400)
  const textGraph=await load();textGraph.graph.nodes.find((n:any)=>n.id==='Source').kind='control_input';textGraph.graph.nodes.find((n:any)=>n.id==='Source').parameters={mode:4};textGraph.graph.nodes.find((n:any)=>n.id==='Source').control_value='check'
  expect((await page.request.put(`/api/projects/${p.id}`,{headers,data:textGraph})).ok()).toBe(true)
  await expect.poll(()=>latest?.values.Latch._checked).toBe(1);await expect(checkbox).toBeChecked()
  await page.screenshot({path:'test-results/toggle.png'})
  await page.getByRole('button',{name:'Disable audio engine',exact:true}).click()
  await expect(checkbox).toBeChecked()
})

test('bangs send explicit one and zero values to a toggle with border-aligned ports',async({page})=>{
  const headers={'X-Pr0former':'1'},status=await(await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap?'register':'login'}`,{headers,data:{username:'browser-test',password:'test1234'}})
  const p=await(await page.request.post('/api/projects',{headers,data:{name:'Bang values',mode:'freeform'}})).json()
  const node=(id:string,kind:string,x:number,y:number,parameters={})=>({id,kind,label:id,x,y,channels:1,parameters})
  const edge=(source:string,target:string,target_port:string)=>({id:source+target,source,source_port:'out',target,target_port})
  p.parts=[];p.graph={nodes:[node('On','trigger',0,0),node('Off','trigger',0,180),node('One','value',200,0,{value:1}),node('Zero','value',200,180,{value:0}),node('Latch','toggle',500,0)],edges:[edge('On','One','trigger'),edge('Off','Zero','trigger'),edge('One','Latch','in'),edge('Zero','Latch','in')]}
  expect((await page.request.put(`/api/projects/${p.id}`,{headers,data:p})).ok()).toBe(true)
  await page.goto('/');await page.getByRole('button',{name:'Enable audio engine',exact:true}).click()
  const checkbox=page.getByRole('checkbox',{name:'Toggle Latch',exact:true})
  for(let i=0;i<3;i++){
    await page.getByRole('button',{name:'Trigger On',exact:true}).click();await expect(checkbox).toBeChecked()
    await page.getByRole('button',{name:'Trigger Off',exact:true}).click();await expect(checkbox).not.toBeChecked()
  }
  await checkbox.check()
  await page.getByRole('button',{name:'Trigger Off',exact:true}).click();await expect(checkbox).not.toBeChecked()
  for(const id of ['On','One','Latch']){
    const patch=page.locator(`.vue-flow__node[data-id="${id}"] .patch-node`),box=(await patch.boundingBox())!
    for(const side of ['left','right']){
      const handles=patch.locator(`.vue-flow__handle-${side}`)
      for(let i=0;i<await handles.count();i++){
        const handle=(await handles.nth(i).boundingBox())!
        expect(Math.abs(handle.x+handle.width/2-(side==='left'?box.x:box.x+box.width))).toBeLessThan(2)
      }
    }
  }
  const value=page.locator('.vue-flow__node[data-id="One"]')
  expect((await value.getByLabel('Value trigger input').boundingBox())!.y).toBeLessThan((await value.getByLabel('Set value input').boundingBox())!.y)
  await page.screenshot({path:'test-results/bang-value-toggle.png'})
  await page.getByRole('button',{name:'Disable audio engine',exact:true}).click()
})

test('checked toggle emits once, flashes Trigger briefly, and forwards an explicit off event',async({page})=>{
  const headers={'X-Pr0former':'1'},status=await(await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap?'register':'login'}`,{headers,data:{username:'browser-test',password:'test1234'}})
  const p=await(await page.request.post('/api/projects',{headers,data:{name:'Toggle events',mode:'freeform'}})).json()
  const node=(id:string,kind:string,x:number,y=0)=>({id,kind,label:id,x,y,channels:1,parameters:{}})
  p.parts=[];p.graph={nodes:[node('Switch','toggle',0),node('Indicator','trigger',250),node('Count','counter',500),node('Mirror','toggle',250,200)],edges:[{id:'flash',source:'Switch',source_port:'out',target:'Indicator',target_port:'in'},{id:'count',source:'Switch',source_port:'out',target:'Count',target_port:'trigger'},{id:'mirror',source:'Switch',source_port:'out',target:'Mirror',target_port:'in'}]}
  expect((await page.request.put(`/api/projects/${p.id}`,{headers,data:p})).ok()).toBe(true)
  let latest:any;page.on('websocket',s=>s.on('framereceived',({payload})=>{const m=JSON.parse(String(payload));if(m.type==='telemetry')latest=m}))
  await page.goto('/');await page.getByRole('button',{name:'Enable audio engine',exact:true}).click()
  const checkbox=page.getByRole('checkbox',{name:'Toggle Switch',exact:true}),mirror=page.getByRole('checkbox',{name:'Toggle Mirror',exact:true}),indicator=page.getByRole('button',{name:'Trigger Indicator',exact:true})
  for(let count=1;count<=2;count++){
    await checkbox.check()
    await expect.poll(()=>latest?.values.Count._out).toBe(count)
    await expect.poll(()=>latest?.values.Indicator._trigger_sequence).toBe(count)
    await expect(mirror).toBeChecked();await expect(checkbox).toBeChecked()
    await expect(indicator).toHaveAttribute('aria-pressed','false')
    await expect.poll(()=>latest?.values.Switch._out).toBe(0)
    await checkbox.uncheck();await expect(mirror).not.toBeChecked()
    expect(latest.values.Count._out).toBe(count)
  }
  await checkbox.check();await expect(mirror).toBeChecked();await expect(indicator).toHaveAttribute('aria-pressed','false')
  await page.screenshot({path:'test-results/toggle-events.png'})
  await page.getByRole('button',{name:'Disable audio engine',exact:true}).click()
  await expect(checkbox).toBeChecked()
})
