import {test,expect} from '@playwright/test'
import {createSocket} from 'node:dgram'
test('OSC and MIDI bridges show traffic and Console Out logs control values without revisions',async({page})=>{
 const headers={'X-Pr0former':'1'},status=await(await page.request.get('/api/status')).json()
 await page.request.post(`/api/${status.bootstrap?'register':'login'}`,{headers,data:{username:'browser-test',password:'test1234'}})
 const receiver=createSocket('udp4');await new Promise<void>(r=>receiver.bind(0,'127.0.0.1',r));const port=receiver.address().port;await new Promise<void>(r=>receiver.close(r))
 const sender=createSocket('udp4')
 const str=(s:string)=>{const b=Buffer.alloc(Math.ceil((Buffer.byteLength(s)+1)/4)*4);b.write(s);return b}
 const send=async(address:string,args:(number|string)[])=>{const values=args.map(v=>{if(typeof v==='string')return str(v);const b=Buffer.alloc(4);b.writeInt32BE(v);return b});const packet=Buffer.concat([str(address),str(','+args.map(v=>typeof v==='string'?'s':'i').join('')),...values]);await new Promise<void>((r,j)=>sender.send(packet,port,'127.0.0.1',e=>e?j(e):r()))}
 let p=await(await page.request.post('/api/projects',{headers,data:{name:'Debug traffic',mode:'freeform'}})).json()
 const n=(id:string,kind:string,x:number,y:number,parameters={},address='/debug',destination='')=>({id,kind,label:id,x,y,channels:1,parameters,...(kind.includes('osc')?{io:{port:'',address,destination}}:{})})
 p.parts=[];p.graph={nodes:[n('OSC in','osc_input',0,0,{text:1},'/text'),n('OSC out','osc_output',500,0,{},'/copy',`127.0.0.1:${port}`),n('Debug','console_out',1000,0),n('OSC to MIDI','osc_to_midi',0,420,{},'/notes'),n('MIDI to OSC','midi_to_osc',500,420,{},'/forward',`127.0.0.1:${port}`)],edges:[{id:'out',source:'OSC in',source_port:'out',target:'OSC out',target_port:'in'},{id:'debug',source:'OSC in',source_port:'out',target:'Debug',target_port:'in'},{id:'midi',source:'OSC to MIDI',source_port:'midi',target:'MIDI to OSC',target_port:'midi'}]}
 const saved=await page.request.put(`/api/projects/${p.id}`,{headers,data:p});expect(saved.ok(),await saved.text()).toBeTruthy();p=await saved.json()
 const config={receive_enabled:true,bind_addresses:['127.0.0.1'],receive_port:port,send_enabled:true,send_address:'127.0.0.1',send_port:0}
 expect((await page.request.put(`/api/projects/${p.id}/system/osc`,{headers,data:config})).ok()).toBeTruthy()
 let latest:any;page.on('websocket',s=>s.on('framereceived',({payload})=>{const m=JSON.parse(String(payload));if(m.type==='telemetry')latest=m}))
 try{
  await page.goto('/');await page.getByRole('button',{name:'Enable audio engine',exact:true}).click();await expect.poll(()=>!!latest).toBeTruthy()
  await send('/text',['ready ♫']);await send('/notes',[64,100])
  await expect.poll(()=>latest?.osc_messages?.['OSC in']?.[1]).toBe('ready ♫');await expect.poll(()=>latest?.osc_messages?.['OSC out']?.[1]).toBe('ready ♫')
  for(const id of ['OSC to MIDI','MIDI to OSC'])await expect.poll(()=>latest?.values?.[id]?._midi_received).toBeGreaterThan(0)
  await expect.poll(async()=>JSON.stringify(await(await page.request.get(`/api/projects/${p.id}/logs`)).json())).toContain('ready ♫')
  for(const id of ['OSC in','OSC out']){
   const face=page.locator('.vue-flow__node').filter({has:page.getByRole('button',{name:`Edit ${id}`,exact:true})});await expect(face.getByRole('table')).toContainText('ready ♫')
   await page.getByRole('button',{name:`Edit ${id}`,exact:true}).click();await expect(page.getByRole('region',{name:'OSC message debug'})).toContainText('ready ♫');await expect(page.getByText('Accepted incoming OSC values.',{exact:false})).toHaveCount(0);await page.getByRole('button',{name:'Close parameters'}).click()
  }
  for(const id of ['OSC to MIDI','MIDI to OSC']){const face=page.locator('.vue-flow__node').filter({has:page.getByRole('button',{name:`Edit ${id}`,exact:true})});await expect(face.getByRole('table')).toContainText('100')}
  expect((await(await page.request.get(`/api/projects/${p.id}`)).json()).project.revision).toBe(p.revision)
 }finally{sender.close();await page.request.post(`/api/projects/${p.id}/engine`,{headers,data:{enabled:false}});await page.request.put(`/api/projects/${p.id}/system/osc`,{headers,data:{...config,receive_enabled:false}})}
})
