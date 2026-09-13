import { test, expect } from '@playwright/test'

test('sample shortlist, nicknames, manual and connected selection reach real sampler audio', async ({ page }) => {
  const headers = { 'X-Pr0former': '1' }
  const status = await (await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap ? 'register' : 'login'}`, { headers, data: { username: 'browser-test', password: 'test1234' } })
  let project = await (await page.request.post('/api/projects', { headers, data: { name: 'Sample palette', mode: 'structured' } })).json()
  const url = `/api/projects/${project.id}`
  const samples: any[] = []
  for (const id of ['bundled-kick','bundled-snare']) {
    const response = await page.request.post(`${url}/samples/${id}/add`, { headers, data: {} })
    expect(response.ok()).toBe(true)
    samples.push(await response.json())
  }
  const node = (id: string, kind: string, x: number, y: number) => ({ id, kind, label: id, x, y, channels: 2, parameters: {} })
  const edge = (source: string, source_port: string, target: string, target_port: string) => ({ id: `${source}-${target}-${target_port}`,source,source_port,target,target_port })
  project.parts.forEach((p: any) => { p.instrument_node=null; p.notes=[] })
  project.graph = { nodes: [node('Palette','sample_selector',0,0),node('Sampler','poly_sampler',300,0),node('Keys','piano',0,430),node('Listen','monitor_output',750,0),node('Index source','value',610,330)], edges: [edge('Palette','out','Sampler','sample_id'),edge('Keys','midi','Sampler','midi'),edge('Sampler','out','Listen','in')] }
  expect((await page.request.put(url, { headers, data: project })).ok()).toBe(true)
  const read = async () => (await (await page.request.get(url)).json()).project
  const choices = async () => (await read()).graph.nodes.find((n: any)=>n.id==='Palette').sample_choices || []
  let latest: any
  let peak=0
  page.on('websocket',socket=>socket.on('framereceived',({payload})=>{
    const event=JSON.parse(String(payload))
    if(event.type==='telemetry'){latest=event;peak=Math.max(peak,event.values?.Sampler?._peak||0)}
  }))
  await page.goto('/')
  await page.getByRole('button',{name:'Edit Palette',exact:true}).click()
  const modal=page.getByRole('dialog',{name:/^Palette/})
  for(const sample of samples){
    await modal.getByLabel('Find shortlist sample').fill(sample.name)
    await modal.getByRole('button',{name:'Add sample',exact:true}).click()
    await expect.poll(async()=> (await choices()).length).toBe(samples.indexOf(sample)+1)
  }
  await modal.getByLabel('Nickname for sample 0').fill('Kick')
  await modal.getByLabel('Nickname for sample 0').press('Tab')
  await expect.poll(async()=> (await choices())[0].nickname).toBe('Kick')
  await modal.getByLabel('Nickname for sample 1').fill('Snare')
  await modal.getByLabel('Nickname for sample 1').press('Tab')
  await expect.poll(async()=> (await choices())[1].nickname).toBe('Snare')
  await page.getByRole('button',{name:'Close parameters'}).click()
  const palette=page.locator('.vue-flow__node[data-id="Palette"]')
  await expect(palette.getByRole('button',{name:'Select sample 0: Kick',exact:true})).toBeVisible()
  await expect(palette.getByRole('button',{name:'Select sample 1: Snare',exact:true})).toBeVisible()
  await page.getByRole('button',{name:'Enable audio engine',exact:true}).click()
  await expect.poll(()=>latest?.values?.Palette?._out).toBe(samples[0].asset)
  await expect.poll(()=>latest?.values?.Sampler?._sample_id).toBe(samples[0].asset)
  for(const [i,name] of ['Kick','Snare'].entries()){
    await palette.getByRole('button',{name:`Select sample ${i}: ${name}`,exact:true}).click()
    await expect.poll(()=>latest?.values?.Sampler?._sample_id).toBe(samples[i].asset)
    peak=0
    expect((await page.request.put(`${url}/piano`,{headers,data:{node:'Keys',pitch:60,velocity:100}})).ok()).toBe(true)
    await expect.poll(()=>peak).toBeGreaterThan(0.001)
    await page.request.put(`${url}/piano`,{headers,data:{node:'Keys',pitch:60,velocity:0}})
  }
  await page.getByRole('button',{name:'Edit Sampler',exact:true}).click()
  await expect(page.getByLabel('Project sample',{exact:true})).toBeDisabled()
  await expect(page.getByText('Palette / out',{exact:true})).toBeVisible()
  await page.getByRole('button',{name:'Close parameters'}).click()
  await page.screenshot({path:'/tmp/pr0-sample-selector.png'})

  let driven=await read()
  driven.graph.edges.push(edge('Index source','out','Palette','index'))
  expect((await page.request.put(url,{headers,data:driven})).ok()).toBe(true)
  await expect(palette.getByRole('button',{name:'Select sample 0: Kick',exact:true})).toBeDisabled()
  await expect.poll(()=>latest?.values?.Sampler?._sample_id).toBe(samples[0].asset)
  for(const [value,asset] of [[1,samples[1].asset],[1.9,samples[1].asset],[12,0],[0,samples[0].asset]]){
    const p=await read()
    expect((await page.request.put(`${url}/parameter`,{headers,data:{node:'Index source',parameter:'value',value,revision:p.revision}})).ok()).toBe(true)
    await expect.poll(()=>latest?.values?.Sampler?._sample_id).toBe(asset)
  }
  const connected=await read()
  expect((await page.request.put(`${url}/parameter`,{headers,data:{node:'Palette',parameter:'index',value:1,revision:connected.revision}})).status()).toBe(400)

  await page.getByRole('button',{name:'Disable audio engine',exact:true}).click()
  for(const change of [
    (p:any)=>p.graph.nodes[0].sample_choices[0].asset=0,
    (p:any)=>p.graph.nodes[0].sample_choices[0].asset=999999999,
    (p:any)=>p.graph.nodes[0].sample_choices[0].nickname='x'.repeat(81),
    (p:any)=>p.graph.nodes[0].sample_choices=Array(65).fill(p.graph.nodes[0].sample_choices[0]),
    (p:any)=>p.graph.nodes[1].sample_choices=p.graph.nodes[0].sample_choices,
  ]){
    const invalid=await read();change(invalid)
    expect((await page.request.put(url,{headers,data:invalid})).status()).toBe(400)
  }
  await page.getByRole('button',{name:'Edit Palette',exact:true}).click()
  await modal.getByRole('button',{name:'Move sample 1 up',exact:true}).click()
  await expect.poll(async()=> (await choices())[0].nickname).toBe('Snare')
  await modal.getByRole('button',{name:'Remove sample 1',exact:true}).click()
  await expect.poll(async()=> (await choices()).length).toBe(1)
  await page.getByRole('button',{name:'Close parameters'}).click()
  await page.reload()
  await expect(palette.getByRole('button',{name:'Select sample 0: Snare',exact:true})).toBeVisible()
  await expect(page.getByRole('alert')).toHaveCount(0)
  const catalog=await (await page.request.get('/api/catalog')).json()
  const doc=catalog.find((d:any)=>d.kind==='sample_selector').documentation
  expect(doc.explanation).toContain('nicknames')
  expect(doc.graph.edges.some((e:any)=>e.target_port==='sample_id')).toBe(true)
})

test('subgraph library carries selector-only samples into another project', async ({ request }) => {
  const headers={'X-Pr0former':'1'}
  const status=await (await request.get('/api/status')).json()
  await request.post(`/api/${status.bootstrap?'register':'login'}`,{headers,data:{username:'browser-test',password:'test1234'}})
  let p=await (await request.post('/api/projects',{headers,data:{name:'Shortlist library source',mode:'freeform'}})).json()
  const sample=await (await request.post(`/api/projects/${p.id}/samples/bundled-kick/add`,{headers,data:{}})).json()
  p.parts=[]
  p.graph={nodes:[{id:'Pack',kind:'subgraph',label:'Pack',x:0,y:0,channels:2,parameters:{}},{id:'Palette',kind:'sample_selector',label:'Palette',parent:'Pack',x:0,y:0,channels:2,parameters:{},sample_choices:[{asset:sample.asset,name:sample.name,nickname:'Kick'}]}],edges:[]}
  const saved=await request.put(`/api/projects/${p.id}`,{headers,data:p});expect(saved.ok()).toBe(true);p=await saved.json()
  const library=await (await request.post(`/api/projects/${p.id}/subgraphs`,{headers,data:{node:'Pack',revision:p.revision,name:'Sample palette pack',public:false}})).json()
  let destination=await (await request.post('/api/projects',{headers,data:{name:'Shortlist library destination',mode:'freeform'}})).json()
  const inserted=await request.post(`/api/projects/${destination.id}/subgraphs/insert`,{headers,data:{library_id:library.id,version:1,revision:destination.revision,parent:null,x:0,y:0}})
  expect(inserted.ok()).toBe(true);destination=await inserted.json()
  const choice=destination.graph.nodes.find((n:any)=>n.kind==='sample_selector').sample_choices[0]
  expect(choice.nickname).toBe('Kick');expect(choice.name).toBe(sample.name);expect(choice.asset).not.toBe(sample.asset)
  const entries=await (await request.get(`/api/projects/${destination.id}/samples`)).json()
  const imported=entries.find((s:any)=>s.asset===choice.asset)
  expect(imported).toBeTruthy()
  expect((await request.get(`/api/projects/${destination.id}/samples/${imported.id}/audio`)).ok()).toBe(true)
})
