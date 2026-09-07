import {test,expect} from '@playwright/test'

test('visualizers preserve typed inputs and display multichannel block history, bins, and phase',async({page})=>{
  const headers={'X-Pr0former':'1'},credentials={username:'browser-test',password:'test1234'}
  const status=await(await page.request.get('/api/status')).json()
  expect((await page.request.post(`/api/${status.bootstrap?'register':'login'}`,{headers,data:credentials})).ok()).toBeTruthy()
  let project=await(await page.request.post('/api/projects',{headers,data:{name:'Visualizer integration',mode:'structured'}})).json()
  const node=(id:string,kind:string,x:number,y:number,parameters:Record<string,number>={},channels=2)=>({id,kind,label:id,x,y,channels,parameters})
  project.parts=[]
  project.graph={nodes:[
    {...node('Text source','control_visualizer',0,0,{},1),control_value:'ready ♫'},
    node('Text display','control_visualizer',430,0,{},1),
    {...node('Number source','control_visualizer',0,280,{},1),control_value:42.5},
    node('Number display','control_visualizer',430,280,{},1),
    node('Tone','oscillator',0,570,{frequency:1000,amplitude:0.25}),
    node('Audio display','audio_visualizer',430,570,{size:256}),
    node('Transform','fft',860,570,{size:256,overlap:4}),
    node('Spectral display','spectral_visualizer',1150,570,{size:256,overlap:4}),
    node('Inverse','ifft',1570,570,{size:256,overlap:4}),
    node('Output','output',1870,570,{gain:-12}),
  ],edges:[['Text source','Text display'],['Number source','Number display'],['Tone','Audio display'],['Audio display','Transform'],['Transform','Spectral display'],['Spectral display','Inverse'],['Inverse','Output']].map(([source,target],i)=>({id:`v-${i}`,source,source_port:'out',target,target_port:'in'}))}
  const saved=await page.request.put(`/api/projects/${project.id}`,{headers,data:project});expect(saved.ok()).toBeTruthy();project=await saved.json()
  const invalid=JSON.parse(JSON.stringify(project));invalid.graph.nodes.push(node('Math','add',0,1000,{},1));invalid.graph.edges.push({id:'bad-text',source:'Text display',source_port:'out',target:'Math',target_port:'a'})
  const rejected=await page.request.put(`/api/projects/${project.id}`,{headers,data:invalid});expect(rejected.status()).toBe(400);expect((await rejected.json()).error).toContain('String control')
  let latest:any;const errors:string[]=[]
  page.on('pageerror',e=>errors.push(e.message))
  page.on('websocket',socket=>socket.on('framereceived',({payload})=>{const m=JSON.parse(String(payload));if(m.type==='telemetry'&&m.project_id===project.id)latest=m}))
  await page.goto('/')
  await page.getByRole('button',{name:'Choose project'}).click()
  await page.locator('.project-popover').getByText('Visualizer integration',{exact:true}).click()
  await page.getByRole('button',{name:'Activate show',exact:true}).click()
  await page.getByRole('button',{name:'Play',exact:true}).click()
  await expect.poll(()=>latest?.visualizations?.['Text display']?.value).toBe('ready ♫')
  expect(latest.visualizations['Number display'].value).toBe(42.5)
  await expect.poll(()=>latest?.visualizations?.['Audio display']?.ready).toBe(true)
  const spectrum=latest.visualizations['Spectral display']
  expect(spectrum.channels).toHaveLength(2);expect(spectrum.channels[0].phase).toHaveLength(129);expect(spectrum.history[0].length).toBe(spectrum.columns*32*2)
  expect(Math.max(...spectrum.channels[0].magnitude)).toBeGreaterThan(1)
  const block=latest.block_size,sample=latest.sample,sequence=latest.visualizations['Audio display'].sequence
  await expect.poll(()=>latest?.sample).toBeGreaterThan(sample)
  expect((latest.sample-sample)/block).toBe(latest.visualizations['Audio display'].sequence-sequence)
  await page.locator('.vue-flow__controls-fitview').click()
  await expect(page.locator('.control-visualizer-value').filter({hasText:'ready ♫'})).toHaveCount(2)
  await page.screenshot({path:'../docs/visualizer-nodes.png',fullPage:true})
  await page.getByRole('button',{name:'Edit Spectral display',exact:true}).click()
  const modal=page.getByRole('dialog')
  await expect(modal.getByRole('img',{name:'Channel 1 spectrogram, FFT bins and phase'})).toBeVisible()
  await expect(modal.getByText('Phase · −π…π rad').first()).toBeVisible()
  const pixels=await modal.locator('canvas').first().evaluate((canvas:HTMLCanvasElement)=>{const p=canvas.getContext('2d')!.getImageData(0,0,canvas.width,canvas.height).data;return [...p].some(v=>v>150)})
  expect(pixels).toBe(true)
  await page.screenshot({path:'../docs/spectral-visualizer.png',fullPage:true})
  await modal.getByRole('button',{name:'Close parameters'}).click()
  await page.getByRole('button',{name:'Edit Text display',exact:true}).click()
  await expect(page.getByRole('dialog').getByText('CONNECTED · input is read-only')).toBeVisible()
  await expect(page.getByLabel('Disconnected input value')).toHaveCount(0)
  await page.getByRole('button',{name:'Close parameters'}).click()
  await page.getByRole('button',{name:'Deactivate show',exact:true}).click()
  await page.getByRole('button',{name:'Edit Text source',exact:true}).click()
  await page.getByLabel('Disconnected input value').fill('new message')
  await page.getByLabel('Disconnected input value').press('Tab')
  await expect.poll(async()=>(await(await page.request.get(`/api/projects/${project.id}`)).json()).project.graph.nodes.find((n:any)=>n.id==='Text source').control_value).toBe('new message')
  expect(errors).toEqual([])
})
