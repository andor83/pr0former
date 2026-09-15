import {test,expect} from '@playwright/test'

for(const backend of ['PulseAudio / PipeWire','WASAPI (shared)'])test(`${backend} exposes separately selectable endpoints with friendly labels`,async({page})=>{
  const headers={'X-Pr0former':'1'},status=await(await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap?'register':'login'}`,{headers,data:{username:'browser-test',password:'test1234'}})
  const p=await(await page.request.post('/api/projects',{headers,data:{name:backend,mode:'freeform'}})).json()
  const interfaces=[{id:701,name:backend.startsWith('WASAPI')?'wasapi:endpoint-a':'pulse:DEVICE=alsa_output.hdmi',label:'Display HDMI',backend,channels:2,enabled:true,correct_latency:false,latency_ms:0},{id:702,name:backend.startsWith('WASAPI')?'wasapi:endpoint-b':'pulse:DEVICE=alsa_output.usb',label:'Studio USB',backend,channels:8,enabled:true,correct_latency:false,latency_ms:0}]
  const config={sample_rate:48000,block_size:128,interfaces,input_interfaces:[]}
  await page.route('**/api/audio/config',r=>r.fulfill({json:config}))
  await page.route('**/api/system/audio',r=>r.fulfill({json:config}))
  await page.route('**/api/devices',r=>r.fulfill({json:{...config,midi_inputs:[],midi_outputs:[],active_inputs:[]}}))
  await page.route('**/api/system/audio/linux',r=>r.fulfill({json:backend.startsWith('WASAPI')?{supported:false}:{supported:true,available:true,backend,endpoints:[{name:'alsa_output.hdmi',description:'Display HDMI',direction:'Output',default:true,state:'RUNNING',channel_map:'front-left,front-right',active_port:'hdmi-output-0',ports:[{name:'hdmi-output-0',description:'HDMI / DisplayPort',availability:'available'}]}],cards:[{name:'card',description:'Built-in audio',active_profile:'output:hdmi-stereo',profiles:[{name:'output:hdmi-stereo',description:'HDMI stereo',availability:'yes'}],ports:[{name:'analog',description:'Analog line out',availability:'not available'}]}]}}))
  let routeId=0
  await page.route(`**/api/projects/${p.id}`,async r=>{
    if(r.request().method()!=='PUT')return r.continue()
    const next=r.request().postDataJSON();routeId=next.graph.nodes.find((n:any)=>n.id==='out').parameters.interface;next.revision++
    await r.fulfill({json:next})
  })
  await page.route(`**/api/projects/${p.id}/parameter`,async r=>{
    const edit=r.request().postDataJSON();routeId=edit.value
    const next=structuredClone(p);next.revision=edit.revision+1
    next.graph.nodes.find((n:any)=>n.id===edit.node).parameters[edit.parameter]=edit.value
    await r.fulfill({json:next})
  })
  await page.goto('/')
  await page.getByRole('button',{name:'Edit Audio output',exact:true}).click()
  const select=page.getByLabel('Audio interface',{exact:true})
  await expect(select.locator('option[value="701"]')).toHaveText('Display HDMI')
  await expect(select.locator('option[value="702"]')).toHaveText('Studio USB')
  await select.selectOption('702');await expect.poll(()=>routeId).toBe(702)
  await expect(select).toHaveValue('702')
  await page.getByRole('button',{name:'Close parameters'}).click()
  await page.getByRole('button',{name:'Settings menu',exact:true}).click()
  await page.getByRole('menuitem',{name:'System settings',exact:true}).click()
  const dialog=page.getByRole('dialog',{name:'System settings',exact:true})
  await expect(dialog.getByRole('checkbox',{name:'Display HDMI',exact:true})).toBeChecked()
  if(!backend.startsWith('WASAPI')){
    const topology=page.getByRole('region',{name:'Linux audio devices'})
    await expect(topology.getByText('HDMI / DisplayPort · available · active')).toBeVisible()
    await topology.locator('summary').click()
    await expect(topology.getByText('Analog line out · not available')).toBeVisible()
    await page.screenshot({path:'/tmp/pr0former-linux-audio.png'})
  }else await expect(page.getByRole('region',{name:'Linux audio devices'})).toHaveCount(0)
})
