import {test,expect} from '@playwright/test'

test('monitor sidebar, process stats and physical device groups and individual channel peaks (simulated hardware)',async({page})=>{
  const headers={'X-Pr0former':'1'},status=await(await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap?'register':'login'}`,{headers,data:{username:'browser-test',password:'test1234'}})
  let p=await(await page.request.post('/api/projects',{headers,data:{name:'Monitor layout',mode:'freeform'}})).json()
  p.parts=[]
  const node=(id:string,kind:string,parameters={})=>({id,kind,label:id,x:0,y:0,channels:2,parameters})
  p.graph={nodes:[node('Native input','input'),node('Browser input','browser_input'),node('Tone','oscillator',{frequency:440,amplitude:0.8}),node('Main output','output',{gain:-24}),node('Headphones','monitor_output',{gain:-12})],edges:[{id:'main',source:'Tone',source_port:'out',target:'Main output',target_port:'in'},{id:'phones',source:'Tone',source_port:'out',target:'Headphones',target_port:'in'}]}
  p=await(await page.request.put(`/api/projects/${p.id}`,{headers,data:p})).json()
  expect((await page.request.put(`/api/projects/${p.id}/system/audio`,{headers,data:{sample_rate:48000,block_size:128,interfaces:[],input_interfaces:[]}})).ok()).toBeTruthy()
  // UI fixtures only: the test server explicitly disables native device access.
  let publish=true, active=true
  let discoveries=0, resourcePolls=0
  page.on('request',r=>{if(r.url().endsWith('/api/system/stats'))resourcePolls++})
  const inputDevice={id:101,name:'Eight-channel capture',channels:8,levels:Array.from({length:8},(_,i)=>({channel:i+1,peak:[0.1,0.5,1.2][i%3]}))}
  const outputDevice={id:202,name:'Sixteen-channel output',channels:16,levels:[{channel:1,peak:0.1},{channel:9,peak:1.2}]}
  await page.route('**/api/devices',route=>{discoveries++;return route.fulfill({json:{interfaces:[outputDevice],input_interfaces:[inputDevice,{id:102,name:'Idle microphone',channels:1}],midi_inputs:[],midi_outputs:[]}})})
  await page.routeWebSocket('**/api/projects/*/events',ws=>{
    const server=ws.connectToServer()
    server.onMessage(message=>{
      const event=JSON.parse(message.toString())
      if(event.type==='hardware_levels'){
        if(publish)ws.send(JSON.stringify({...event,project_id:active?p.id:null,inputs:[inputDevice],outputs:active?[outputDevice]:[]}))
      }else ws.send(message)
    })
  })
  await page.goto('/')
  await page.getByRole('button',{name:'Choose project'}).click()
  await page.getByRole('button',{name:'Monitor layout',exact:true}).click()
  await expect(page.locator('.workspace-actions').getByRole('button',{name:'Ensemble'})).toHaveCount(0)
  await expect(page.getByRole('button',{name:'Save revision',exact:true})).toHaveAttribute('title',/Revision \d+/)
  await expect(page.locator('.project-title')).toBeVisible()
  await page.getByRole('button',{name:'Monitor',exact:true}).click()
  const sidebar=page.getByRole('complementary',{name:'Monitor controls and statistics'})
  await expect(sidebar.getByRole('heading',{name:'Clock sync'})).toBeVisible()
  await expect(sidebar.getByRole('heading',{name:'Server process'})).toBeVisible()
  await expect(sidebar.getByLabel('Monitor feed')).toBeVisible()
  await expect(page.getByRole('region',{name:'Inputs VU meters'}).getByRole('meter')).toHaveCount(9)
  await expect(page.getByRole('region',{name:'Outputs VU meters'}).getByRole('meter')).toHaveCount(2)
  const discoveryCount=discoveries, statsCount=resourcePolls
  await expect.poll(()=>resourcePolls,{timeout:5000}).toBeGreaterThan(statsCount)
  expect(discoveries).toBe(discoveryCount) // process stats polling must not enumerate audio/MIDI devices
  const left=await sidebar.boundingBox(),right=await page.locator('.meter-banks').boundingBox()
  expect(right!.x).toBeGreaterThanOrEqual(left!.x+left!.width)
  expect(right!.width).toBeGreaterThan(left!.width*2)
  const input=await page.getByRole('region',{name:'Inputs VU meters'}).boundingBox(),output=await page.getByRole('region',{name:'Outputs VU meters'}).boundingBox()
  expect(output!.y).toBeGreaterThan(input!.y+input!.height)
  const resource=await(await page.request.get('/api/system/stats')).json()
  expect(resource.resident_bytes).toBeGreaterThan(0)
  await expect.poll(async()=> (await(await page.request.get('/api/system/stats')).json()).cpu_percent).toBeGreaterThanOrEqual(0)
  await expect(page.getByRole('meter',{name:'Idle microphone input channel 1 level'})).toHaveAttribute('aria-valuetext','No current hardware data')
  const capture=page.getByRole('region',{name:'Inputs VU meters'}).locator('.device-group[data-device="101"]')
  await expect(capture.getByRole('meter')).toHaveCount(8)
  await expect(capture.locator('[data-channel="1"]')).toHaveAttribute('data-zone','normal')
  await expect(capture.locator('[data-channel="2"]')).toHaveAttribute('data-zone','warning')
  await expect(capture.locator('[data-channel="3"]')).toHaveAttribute('data-zone','clip')
  const main=page.getByRole('region',{name:'Outputs VU meters'}).locator('[data-channel="9"]')
  await expect(main).toHaveAttribute('data-zone','clip')
  await expect(main.getByText('CLIP',{exact:true})).toBeVisible()
  await expect(main.locator('.vu-fill')).toHaveCSS('background-color','rgb(241, 109, 105)')
  await expect(main.getByRole('meter')).toHaveAttribute('aria-valuetext',/clipping/)
  await expect(page.getByRole('meter',{name:/Browser input|Headphones/})).toHaveCount(0)
  const geometry=()=>page.locator('.meter-banks').evaluate(root=>({
    strips:Array.from(root.querySelectorAll('.vu-strip')).map(el=>{const r=el.getBoundingClientRect();return [r.x,r.y,r.width,r.height]}),
    overflow:Array.from(root.querySelectorAll('.meter-bank,.device-groups,.device-group,.meter-bank-body,.vu-strip')).map(el=>el.scrollHeight-el.clientHeight),
    tracks:Array.from(root.querySelectorAll('.vu-track')).map(el=>el.getBoundingClientRect().height),
  }))
  const assertFits=async()=>{
    const g=await geometry()
    expect(Math.max(...g.overflow)).toBeLessThanOrEqual(1)
    expect(new Set(g.strips.map(r=>r[2])).size).toBe(1)
    expect(Math.min(...g.tracks)).toBeGreaterThan(30)
    return g
  }
  const before=await assertFits()
  inputDevice.levels.forEach((level,i)=>{level.peak=[0,0.99,0.0001][i%3]!})
  await expect(capture.locator('[data-channel="1"] .vu-value')).toContainText('−∞')
  await expect(capture.locator('[data-channel="2"] .vu-value')).toContainText('-0.1')
  expect((await assertFits()).strips).toEqual(before.strips)
  await page.screenshot({path:'test-results/monitor-desktop.png'})
  await page.setViewportSize({width:768,height:1024})
  await expect(sidebar).toBeVisible()
  await expect(page.getByRole('button',{name:'Save revision',exact:true})).toHaveAttribute('title',/Revision \d+/)
  await page.screenshot({path:'test-results/monitor-tablet.png'})
  await assertFits()
  await page.setViewportSize({width:900,height:600})
  await assertFits()
  await page.screenshot({path:'test-results/monitor-short.png'})
  await page.setViewportSize({width:390,height:844})
  await assertFits()
  await page.screenshot({path:'test-results/monitor-phone.png'})
  publish=false
  await expect(main).toHaveClass(/stale/)
  await expect(main.getByRole('meter')).toHaveAttribute('aria-valuetext','No current hardware data')
  active=false;publish=true
  await expect(page.getByRole('region',{name:'Outputs VU meters'}).getByRole('meter')).toHaveCount(0)
  await expect(capture.getByRole('meter')).toHaveCount(8)
  await expect(capture.locator('[data-channel="3"]')).not.toHaveClass(/stale/)
})
