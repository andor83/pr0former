import {test,expect} from '@playwright/test'
test('Granular Field shows configured ports, edits its field and sources, and enforces slot limits',async({page})=>{
 const headers={'X-Pr0former':'1'},status=await(await page.request.get('/api/status')).json()
 await page.request.post(`/api/${status.bootstrap?'register':'login'}`,{headers,data:{username:'browser-test',password:'test1234'}})
 let p=await(await page.request.post('/api/projects',{headers,data:{name:'Granular field',mode:'freeform'}})).json(),url=`/api/projects/${p.id}`
 const kick=await(await page.request.post(`${url}/samples/bundled-kick/add`,{headers,data:{}})).json()
 const snare=await(await page.request.post(`${url}/samples/bundled-snare/add`,{headers,data:{}})).json()
 const n=(id:string,kind:string,x:number,y:number,parameters={})=>({id,kind,label:id,x,y,channels:2,parameters})
 const edge=(source:string,source_port:string,target:string,target_port:string)=>({id:`${source}-${target}-${target_port}`,source,source_port,target,target_port})
 p.parts=[];p.graph={nodes:[n('Mic','browser_input',0,0),n('Xsrc','value',0,300,{value:-0.4}),{...n('Field','granular_field',360,0,{source_1_x:-0.7,source_2_x:0.6,x:0,y:0}),sample_choices:[{asset:kick.asset,name:kick.name,nickname:'Kick'}]},n('Listen','monitor_output',900,0)],edges:[edge('Field','out','Listen','in')]}
 expect((await page.request.put(url,{headers,data:p})).ok()).toBe(true)
 const read=async()=>(await(await page.request.get(url)).json()).project
 const field=async()=>(await read()).graph.nodes.find((x:any)=>x.id==='Field')
 let latest:any
 page.on('websocket',socket=>socket.on('framereceived',({payload})=>{const event=JSON.parse(String(payload));if(event.type==='telemetry')latest=event}))
 await page.addInitScript(()=>localStorage.setItem('pr0former.help','hidden'))
 await page.goto('/');const node=page.locator('.vue-flow__node[data-id="Field"]')
 // Only configured sample slots expose a setter; both live audio ports are always present; no control or MIDI inlets.
 await expect(node.locator('[data-handleid="sample_1"]')).toHaveCount(1)
 await expect(node.locator('[data-handleid="sample_2"]')).toHaveCount(0)
 await expect(node.locator('[data-handleid="live_1"]')).toHaveCount(1)
 await expect(node.locator('[data-handleid="live_2"]')).toHaveCount(1)
 for(const id of ['midi','velocity','gate','trigger','note_off','source_1_x','live_1_buffer_ms'])await expect(node.locator(`[data-handleid="${id}"]`)).toHaveCount(0)
 await expect(node.getByRole('img',{name:/Granular field: 1 sample, 0 live inputs/})).toBeVisible()
 // The field pad in the modal: keyboard and mouse edits persist parameters.
 await node.getByRole('button',{name:'Edit Field',exact:true}).click()
 const modal=page.getByRole('dialog',{name:/^Field/})
 const control=modal.getByRole('slider',{name:'Field control point handle'})
 await control.focus();await control.press('ArrowRight');await expect.poll(async()=>(await field()).parameters.x).toBe(0.01)
 await control.press('Shift+ArrowUp');await expect.poll(async()=>(await field()).parameters.y).toBe(0.1)
 const source=modal.getByRole('slider',{name:'Kick source handle'}),box=(await source.boundingBox())!
 await page.mouse.move(box.x+box.width/2,box.y+box.height/2);await page.mouse.down();await page.mouse.move(box.x+box.width/2+40,box.y+box.height/2,{steps:6});await page.mouse.up()
 await expect.poll(async()=>(await field()).parameters.source_1_x).toBeGreaterThan(-0.7)
 await expect(modal.getByRole('spinbutton',{name:'Sample 1 X',exact:true})).toHaveCount(0)
 await expect(modal.getByRole('spinbutton',{name:'Focus',exact:true})).toBeVisible()
 // Randomize pitch is a checkbox that relabels the pitch slider.
 await expect(modal.getByRole('spinbutton',{name:'Pitch',exact:true})).toBeVisible()
 await modal.getByRole('checkbox',{name:'Randomize pitch'}).check()
 await expect.poll(async()=>(await field()).parameters.randomize_pitch).toBe(1)
 await expect(modal.getByRole('spinbutton',{name:'Pitch randomness (± semitones)',exact:true})).toBeVisible()
 // Sources dialog: add, reorder (settings follow), remove.
 await modal.getByRole('button',{name:'Edit sources',exact:true}).click()
 const sources=page.getByRole('dialog',{name:'Sources'})
 await sources.getByLabel('Find field sample').fill(`${snare.name} (#${snare.asset})`)
 await sources.getByRole('button',{name:'Add sample',exact:true}).click()
 await expect.poll(async()=>(await field()).sample_choices.length).toBe(2)
 await expect.poll(async()=>(await field()).parameters.source_2_gain).toBe(1)
 await sources.getByLabel('Tune for slot 2').fill('7');await sources.getByLabel('Tune for slot 2').press('Tab')
 await expect.poll(async()=>(await field()).parameters.source_2_tune).toBe(7)
 await sources.getByRole('button',{name:'Move slot 2 up'}).click()
 await expect.poll(async()=>(await field()).sample_choices[0].name).toBe(snare.name)
 await expect.poll(async()=>(await field()).parameters.source_1_tune).toBe(7)
 await expect(sources.getByText('Connect audio to the Live 1 input')).toBeVisible()
 await sources.getByRole('button',{name:'Remove slot 2'}).click()
 await expect.poll(async()=>(await field()).sample_choices.length).toBe(1)
 await sources.getByRole('button',{name:'Close sources'}).click()
 await expect(modal.getByRole('button',{name:'Edit sources',exact:true})).toBeFocused()
 await page.getByRole('button',{name:'Close parameters'}).click()
 // Server contract: slots beyond the list and lists beyond eight are rejected.
 let bad=await read();bad.graph.edges.push(edge('Xsrc','out','Field','sample_3'));expect((await page.request.put(url,{headers,data:bad})).status()).toBe(400)
 bad=await read();bad.graph.nodes.find((x:any)=>x.id==='Field').sample_choices=Array.from({length:9},()=>({asset:kick.asset,name:kick.name,nickname:''}));expect((await page.request.put(url,{headers,data:bad})).status()).toBe(400)
 // A live input joins the field only while cabled; cabling both axes locks the control point (one cabled axis locks only that axis).
 p=await read();p.graph.edges.push(edge('Mic','out','Field','live_1'),edge('Xsrc','out','Field','x'),edge('Xsrc','out','Field','y'));expect((await page.request.put(url,{headers,data:p})).ok()).toBe(true)
 await expect(node.getByRole('img',{name:/1 sample, 1 live input/})).toBeVisible()
 await node.getByRole('button',{name:'Edit Field',exact:true}).click()
 await expect(modal.getByRole('slider',{name:'Live 1 source handle'})).toBeVisible()
 // With both axes cabled and no live values yet, the pad shows a placeholder rather than an invented point.
 await expect(modal.getByRole('slider',{name:'Field control point handle'})).toHaveCount(0)
 await expect(modal.getByText('waiting for cabled x/y')).toBeVisible()
 await expect(modal.getByText('Xsrc / out').first()).toBeVisible()
 p=await read();expect((await page.request.put(`${url}/parameter`,{headers,data:{node:'Field',parameter:'x',value:0.5,revision:p.revision}})).status()).toBe(400)
 await page.getByRole('button',{name:'Close parameters'}).click()
 // Live telemetry: weights follow the cabled control point; the live source has weight while cabled.
 await page.getByRole('button',{name:'Enable audio engine',exact:true}).click()
 await expect.poll(()=>latest?.values?.Field?._x).toBe(-0.4)
 // At (-0.4,-0.4) the (silent, unfed) live input at the origin is nearer than the remaining sample at (0.6, 0.49).
 await expect.poll(()=>latest?.values?.Field?._source_9_weight).toBeGreaterThan(0.5)
 await expect.poll(()=>latest?.values?.Field?._live_1_fill).toBeGreaterThan(0)
 // Steering the cabled point onto the sample makes the sample dominate and the cloud audible.
 p=await read();expect((await page.request.put(`${url}/parameter`,{headers,data:{node:'Xsrc',parameter:'value',value:0.6,revision:p.revision}})).ok()).toBe(true)
 await expect.poll(()=>latest?.values?.Field?._x).toBe(0.6)
 await expect.poll(()=>latest?.values?.Field?._source_1_weight).toBeGreaterThan(0.5)
 await expect.poll(()=>latest?.values?.Field?._peak).toBeGreaterThan(0)
 // Live values place the control point, and the cabled axes keep it locked.
 await node.getByRole('button',{name:'Edit Field',exact:true}).click()
 await expect(modal.getByRole('slider',{name:'Field control point handle'})).toHaveAttribute('aria-disabled','true')
 await expect(modal.getByRole('slider',{name:'Field control point handle'})).toHaveAttribute('aria-valuetext','x 0.60, y 0.60')
 await page.getByRole('button',{name:'Close parameters'}).click()
 await page.getByRole('button',{name:'Disable audio engine',exact:true}).click()
 // Catalog shape.
 const d=(await(await page.request.get('/api/catalog')).json()).find((d:any)=>d.kind==='granular_field')
 expect(d.inputs.map((p:any)=>[p.id,p.signal])).toEqual([['live_1','audio'],['live_2','audio']])
 const param=(id:string)=>d.parameters.find((p:any)=>p.id===id)
 expect([param('x').min,param('x').max,param('focus').min,param('focus').max,param('focus').default]).toEqual([-1,1,0.05,2,0.5])
 expect([param('randomize_pitch').default,param('live_1_buffer_ms').default,param('sample_8').max]).toEqual([0,500,1000000000])
 expect(d.parameters.every((p:any)=>!p.structural)).toBe(true)
 expect(d.documentation.explanation).toContain('no note')
})
