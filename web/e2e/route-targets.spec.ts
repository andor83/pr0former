import {test,expect} from '@playwright/test'
const headers={'X-Pr0former':'1'}
test('route targets keep typed drafts during telemetry, suggest existing names and unlock after disconnect',async({page})=>{
 const status=await(await page.request.get('/api/status')).json()
 await page.request.post(`/api/${status.bootstrap?'register':'login'}`,{headers,data:{username:'browser-test',password:'test1234'}})
 let p=await(await page.request.post('/api/projects',{headers,data:{name:'Target editing',mode:'freeform'}})).json()
 const node=(id:string,kind:string,target:string)=>({id,kind,label:id,x:0,y:0,channels:1,parameters:{},control_value:target})
 p.parts=[];p.graph={nodes:[node('Receive','receive_control','main'),node('Send','send_control','existing'),node('Duplicate','receive_control','existing'),node('Audio','send_audio','audio-only'),{...node('Source','control_input','dynamic'),parameters:{mode:4}},node('Driven','send_control','unused')],edges:[{id:'target-wire',source:'Source',source_port:'out',target:'Driven',target_port:'target'}]}
 expect((await page.request.put(`/api/projects/${p.id}`,{headers,data:p})).ok()).toBe(true)
 const load=async()=>(await(await page.request.get(`/api/projects/${p.id}`)).json()).project
 let frames=0;page.on('websocket',s=>s.on('framereceived',({payload})=>{if(JSON.parse(String(payload)).type==='telemetry')frames++}))
 await page.goto('/');await page.getByRole('button',{name:'Enable audio engine',exact:true}).click()
 // Nodes overlap deliberately: use their keyboard-accessible settings buttons directly.
 await page.getByRole('button',{name:'Edit Receive',exact:true}).evaluate((el:HTMLButtonElement)=>el.click())
 const input=page.getByLabel('Target name',{exact:true})
 await expect(input).toBeEnabled();await input.fill('new route')
 const start=frames;await expect.poll(()=>frames-start).toBeGreaterThan(3)
 await expect(input).toHaveValue('new route')
 await input.press('Tab')
 await expect.poll(async()=>(await load()).graph.nodes.find((n:any)=>n.id==='Receive').control_value).toBe('new route')
 // The suggestion list is ordinary markup (no datalist, which freezes Chrome on iOS); an empty query lists every known target.
 await input.fill('')
 const options=page.getByRole('listbox',{name:'Target name suggestions'}).getByRole('option')
 await expect.poll(()=>options.evaluateAll(els=>els.map(el=>el.querySelector('span')?.textContent))).toEqual(['dynamic','existing'])
 await input.fill('existing');await input.press('Tab')
 await expect.poll(async()=>(await load()).graph.nodes.find((n:any)=>n.id==='Receive').control_value).toBe('existing')
 await page.getByRole('button',{name:'Close parameters'}).click()
 await page.getByRole('button',{name:'Edit Driven',exact:true}).evaluate((el:HTMLButtonElement)=>el.click())
 await expect(input).toBeDisabled();await expect(input).toHaveValue('dynamic')
 await page.getByRole('button',{name:'Disconnect Source from Target name',exact:true}).click()
 await expect(input).toBeEnabled();await input.fill('manual');await input.press('Tab')
 await expect.poll(async()=>(await load()).graph.nodes.find((n:any)=>n.id==='Driven').control_value).toBe('manual')
 await page.getByRole('button',{name:'Close parameters'}).click()
 await page.getByRole('button',{name:'Disable audio engine',exact:true}).click()
})
