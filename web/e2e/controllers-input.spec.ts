import {test,expect} from '@playwright/test'
const headers={'X-Pr0former':'1'}
test.afterEach(async({page})=>{const p=(await(await page.request.get('/api/projects')).json());for(const item of Array.isArray(p)?p:[]){if(item.name==='Controller regression'||item.name==='Browser uplink regression')await page.request.post(`/api/projects/${item.id}/transport`,{headers,data:{action:'deactivate'}})}})
test('controller gestures send CC, learn assignments, and expose dynamic outlets',async({page})=>{
  const status=await(await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap?'register':'login'}`,{headers,data:{username:'browser-test',password:'test1234'}})
  let p=await(await page.request.post('/api/projects',{headers,data:{name:'Controller regression',mode:'freeform'}})).json()
  p.parts=[];p.graph={nodes:[{id:'knobs',kind:'knobs',label:'Knobs',x:0,y:0,channels:1,parameters:{count:2,controller_1:74,channel_1:6}},{id:'sliders',kind:'sliders',label:'Sliders',x:500,y:0,channels:1,parameters:{count:2}}],edges:[{id:'midi',source:'knobs',source_port:'midi',target:'sliders',target_port:'midi'}]}
  const save=await page.request.put(`/api/projects/${p.id}`,{headers,data:p});expect(save.ok()).toBeTruthy();p=await save.json()
  let latest:any
  page.on('websocket',s=>s.on('framereceived',({payload})=>{const m=JSON.parse(String(payload));if(m.type==='telemetry')latest=m}))
  await page.goto('/');await page.getByRole('button',{name:'Enable audio engine',exact:true}).click()
  const knob=page.getByRole('slider',{name:'Knob 1',exact:true}),slider=page.getByRole('slider',{name:'Slider 1',exact:true})
  await expect(knob).toHaveAttribute('aria-disabled','false')
  await slider.click()
  await expect.poll(()=>latest?.values.sliders._learning).toBe(1)
  await knob.focus();await page.keyboard.press('ArrowUp')
  await expect.poll(async()=>{const q=(await(await page.request.get(`/api/projects/${p.id}`)).json()).project;return q.graph.nodes[1].parameters.controller_1}).toBe(74)
  const b=(await knob.boundingBox())!
  await page.mouse.move(b.x+b.width/2,b.y+b.height/2);await page.mouse.down();await page.mouse.move(b.x+b.width/2,b.y-40,{steps:8});await page.mouse.up()
  await expect.poll(()=>latest?.values.knobs._control_1).toBeGreaterThan(0.4)
  await expect.poll(()=>latest?.values.sliders._control_1).toBeGreaterThan(0.4)
  expect(await page.getByRole('slider',{name:'Knob 3',exact:true}).count()).toBe(0)
  await page.screenshot({path:'test-results/controllers.png'})
  await page.getByRole('button',{name:'Disable audio engine',exact:true}).click()
})
test('local audio input modal starts capture and real Opus uplink reaches the graph',async({page})=>{
  const status=await(await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap?'register':'login'}`,{headers,data:{username:'browser-test',password:'test1234'}})
  let p=await(await page.request.post('/api/projects',{headers,data:{name:'Browser uplink regression',mode:'freeform'}})).json()
  p.parts=[];p.graph={nodes:[{id:'mic',kind:'browser_input',label:'Local audio input',x:0,y:0,channels:2,parameters:{}},{id:'out',kind:'output',label:'Output',x:300,y:0,channels:2,parameters:{}}],edges:[{id:'audio',source:'mic',source_port:'out',target:'out',target_port:'in'}]}
  const saved=await page.request.put(`/api/projects/${p.id}`,{headers,data:p});expect(saved.ok()).toBeTruthy();p=await saved.json()
  await page.addInitScript(()=>{
    navigator.mediaDevices.getUserMedia=async()=>{
      const Generator=(window as any).MediaStreamTrackGenerator,Data=(window as any).AudioData
      const track=new Generator({kind:'audio'}),writer=track.writable.getWriter();let samples=0
      void(async()=>{try{while(track.readyState==='live'){
        const pcm=new Float32Array(960);for(let i=0;i<pcm.length;i++)pcm[i]=Math.sin((samples+i)*2*Math.PI*440/48000)*0.2
        const frame=new Data({format:'f32-planar',sampleRate:48000,numberOfFrames:960,numberOfChannels:1,timestamp:samples/48000*1e6,data:pcm})
        await writer.write(frame);frame.close();samples+=960;await new Promise(r=>setTimeout(r,20))
      }}catch{}})();return new MediaStream([track])
    }
  })
  let peak=0
  page.on('websocket',s=>s.on('framereceived',({payload})=>{const m=JSON.parse(String(payload));if(m.type==='telemetry')peak=Math.max(peak,m.values?.mic?._peak??0)}))
  await page.goto('/');await page.getByRole('button',{name:'Enable audio engine',exact:true}).click()
  await page.getByRole('button',{name:'Edit Local audio input',exact:true}).click()
  await page.getByRole('button',{name:'Connect this input to graph',exact:true}).click()
  await expect.poll(()=>peak,{timeout:20000}).toBeGreaterThan(0.01)
  await page.getByRole('button',{name:'Disconnect',exact:true}).click()
  await page.getByRole('button',{name:'Disable audio engine',exact:true}).click()
})
