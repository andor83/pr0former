import { test, expect } from '@playwright/test'

test('piano drag bends, glissandos through white and black keys, and cleans up', async ({page}) => {
  const headers={'X-Pr0former':'1'},status=await(await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap?'register':'login'}`,{headers,data:{username:'browser-test',password:'test1234'}})
  let p=await(await page.request.post('/api/projects',{headers,data:{name:'Piano drag',mode:'freeform'}})).json()
  const node=(id:string,kind:string,x:number,parameters={})=>({id,kind,label:id,x,y:0,channels:1,parameters})
  p.parts=[]
  p.graph={nodes:[node('Keys','piano',0),node('Receive','piano',420),node('Bend','midi_to_control',840,{message_type:14}),node('Bend value','add',1080,{b:0})],edges:[
    {id:'notes',source:'Keys',source_port:'midi',target:'Receive',target_port:'midi'},
    {id:'bend',source:'Receive',source_port:'midi',target:'Bend',target_port:'midi'},
    {id:'value',source:'Bend',source_port:'value',target:'Bend value',target_port:'a'}]}
  const saved=await page.request.put(`/api/projects/${p.id}`,{headers,data:p});expect(saved.ok()).toBe(true);p=await saved.json()
  const gesture=(data:object)=>page.request.put(`/api/projects/${p.id}/piano`,{headers,data:{node:'Keys',...data}})
  expect((await gesture({bend:8192})).status()).toBe(400)
  let latest:any
  const sent:any[]=[]
  page.on('websocket',socket=>socket.on('framesent',({payload})=>{const m=JSON.parse(String(payload));if(m.type==='piano')sent.push(m)}))
  page.on('websocket',socket=>socket.on('framereceived',({payload})=>{const m=JSON.parse(String(payload));if(m.type==='telemetry')latest=m}))
  await page.goto('/')
  await page.getByRole('button',{name:'Enable audio engine',exact:true}).click()
  const source=page.locator('.vue-flow__node[data-id="Keys"]'), receiver=page.locator('.vue-flow__node[data-id="Receive"]')
  const key=(name:string)=>source.getByRole('button',{name,exact:true})
  const c=key('C4 MIDI 60'),d=key('D4 MIDI 62'),black=key('C♯4 MIDI 61')
  await expect(c).toBeEnabled()
  // Initial graph fit starts after 100 ms and animates for 300 ms.
  await page.waitForTimeout(500)
  const cb=(await c.boundingBox())!,db=(await d.boundingBox())!,bb=(await black.boundingBox())!
  const x=cb.x+cb.width/2,y=cb.y+cb.height-10
  await page.mouse.move(x,y);await page.mouse.down()
  await expect(receiver.getByRole('button',{name:'C4 MIDI 60',exact:true})).toHaveAttribute('aria-pressed','true')
  await page.mouse.move(x,y-cb.height/2)
  await expect.poll(()=>latest?.values?.['Bend value']?._out).toBeGreaterThan(12000)
  await expect(receiver.getByRole('button',{name:'C4 MIDI 60',exact:true})).toHaveAttribute('aria-pressed','true')
  await page.mouse.move(db.x+db.width/2,y)
  await expect(receiver.getByRole('button',{name:'D4 MIDI 62',exact:true})).toHaveAttribute('aria-pressed','true')
  await expect(receiver.getByRole('button',{name:'C4 MIDI 60',exact:true})).toHaveAttribute('aria-pressed','false')
  await page.mouse.move(bb.x+bb.width/2,bb.y+bb.height/2)
  await expect(receiver.getByRole('button',{name:'C♯4 MIDI 61',exact:true})).toHaveAttribute('aria-pressed','true')
  await expect(receiver.getByRole('button',{name:'D4 MIDI 62',exact:true})).toHaveAttribute('aria-pressed','false')
  await page.mouse.move(bb.x+bb.width/2,y+cb.height+10)
  await expect.poll(()=>latest?.values?.['Bend value']?._out).toBe(0)
  await page.mouse.up()
  await expect(receiver.locator('.piano-key.lit')).toHaveCount(0)
  await expect.poll(()=>latest?.values?.['Bend value']?._out).toBe(8192)
  expect(sent.filter(m=>m.pitch!==undefined).map(({pitch,velocity})=>[pitch,velocity])).toEqual([[60,100],[60,0],[62,100],[62,0],[61,100],[61,0]])
  // Lost focus must release the key and recenter even if no pointerup arrives.
  await page.mouse.move(x,y);await page.mouse.down();await page.mouse.move(x,y-30)
  await expect.poll(()=>latest?.values?.['Bend value']?._out).toBeGreaterThan(8192)
  await page.evaluate(()=>window.dispatchEvent(new Event('blur')))
  await expect(receiver.locator('.piano-key.lit')).toHaveCount(0)
  await expect.poll(()=>latest?.values?.['Bend value']?._out).toBe(8192)
  await page.mouse.up()
  for(const data of [{bend:-1},{bend:16384},{bend:2.5},{pitch:60,velocity:100,bend:8192},{bend:8192,node:'Bend'}])expect([400,422]).toContain((await gesture(data)).status())
  expect((await(await page.request.get(`/api/projects/${p.id}`)).json()).project.revision).toBe(p.revision)
})
