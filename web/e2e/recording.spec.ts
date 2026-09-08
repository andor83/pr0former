import {test,expect} from '@playwright/test'
import {existsSync,readdirSync,readFileSync} from 'node:fs'
import {join,dirname} from 'node:path'
function files(root:string):string[]{if(!existsSync(root))return [];return readdirSync(root,{withFileTypes:true}).flatMap(e=>e.isDirectory()?files(join(root,e.name)):[join(root,e.name)])}
test('record archives input channels and separate named takes privately, finalizing on disable',async({page})=>{
  const headers={'X-Pr0former':'1'},status=await(await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap?'register':'login'}`,{headers,data:{username:'browser-test',password:'test1234'}})
  let p=await(await page.request.post('/api/projects',{headers,data:{name:'Archive workspace',mode:'freeform'}})).json()
  const node=(id:string,kind:string,x:number,channels:number,parameters={})=>({id,kind,label:id,x,y:0,channels,parameters})
  p.parts=[];p.graph={nodes:[node('Archive','record',0,1),node('Tone','oscillator',260,8,{frequency:440,amplitude:.2}),node('Start','value',520,1,{value:0}),node('Stop','value',780,1,{value:0})],edges:[{id:'in',source:'Tone',source_port:'out',target:'Archive',target_port:'in'},...['start','stop'].map((target_port,i)=>({id:target_port,source:['Start','Stop'][i],source_port:'out',target:'Archive',target_port}))]}
  const saved=await page.request.put(`/api/projects/${p.id}`,{headers,data:p});expect(saved.ok(),await saved.text()).toBe(true)
  const load=async()=>(await(await page.request.get(`/api/projects/${p.id}`)).json()).project
  const param=async(node:string,value:number)=>{const current=await load();const r=await page.request.put(`/api/projects/${p.id}/parameter`,{headers,data:{node,parameter:'value',value,revision:current.revision}});expect(r.ok(),await r.text()).toBe(true)}
  const root=join(process.env.PR0_TEST_DATA!,'recordings')
  const takes=()=>files(root).filter(f=>f.endsWith('.json')).map(f=>({path:f,...JSON.parse(readFileSync(f,'utf8'))})).filter(t=>t.project_id===p.id)
  let latest:any
  page.on('websocket',s=>s.on('framereceived',({payload})=>{const m=JSON.parse(String(payload));if(m.type==='telemetry')latest=m}))
  await page.goto('/')
  await page.getByRole('button',{name:'Enable audio engine',exact:true}).click()
  await page.getByRole('button',{name:'Edit Archive',exact:true}).click()
  await page.getByLabel('Node name',{exact:true}).fill('Night performance')
  await page.getByLabel('Node name',{exact:true}).press('Tab')
  await expect(page.getByRole('heading',{name:'Night performance',exact:true})).toBeVisible()
  await param('Start',1)
  await expect.poll(()=>latest?.values.Archive._record_seconds||0).toBeGreaterThan(.1)
  await expect(page.getByRole('region',{name:'Recording status'})).toContainText('8 input channels')
  await expect(page.getByRole('region',{name:'Recording status'})).toContainText('Recording')
  expect(latest.running).toBe(false)
  const current=await load();current.graph.nodes.find((n:any)=>n.id==='Tone').label='Audio source'
  expect((await page.request.put(`/api/projects/${p.id}`,{headers,data:current})).ok()).toBe(true)
  await param('Stop',1)
  await expect.poll(()=>latest?.values.Archive._recording).toBe(0)
  await expect.poll(()=>takes().filter(t=>t.complete).length).toBe(1)
  const take=takes()[0]
  expect(take.channels).toBe(8);expect(take.sample_rate).toBe(latest.sample_rate);expect(take.bits_per_sample).toBe(32);expect(take.name).toBe('Night performance');expect(take.file).toMatch(/^Night_performance-.+_\d+\.wav$/)
  const wav=readFileSync(join(dirname(take.path),take.file))
  expect(wav.toString('ascii',0,4)).toBe('RIFF');expect(wav.readUInt16LE(22)).toBe(8);expect(wav.readUInt32LE(24)).toBe(take.sample_rate)
  const publicResponse=await page.request.get(`/recordings/${take.file}`)
  expect((await publicResponse.body()).subarray(0,4).toString()).not.toBe('RIFF')
  await param('Start',0);await param('Stop',0);await param('Start',1)
  await expect.poll(()=>latest?.values.Archive._recording).toBe(1)
  await expect.poll(()=>latest?.values.Archive._record_seconds||0).toBeGreaterThan(.1)
  await page.screenshot({path:'test-results/recording.png'})
  await page.getByRole('button',{name:'Close parameters'}).click()
  await page.getByRole('button',{name:'Disable audio engine',exact:true}).click()
  await expect.poll(()=>takes().filter(t=>t.complete).length).toBe(2)
  expect(new Set(takes().map(t=>t.file)).size).toBe(2)
})
