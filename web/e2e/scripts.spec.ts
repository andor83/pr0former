import {test,expect} from '@playwright/test'
import type {Page} from '@playwright/test'
import {createSocket} from 'node:dgram'
const headers={'X-Pr0former':'1'}
async function setup(page:Page,source:string){
 const status=await(await page.request.get('/api/status')).json()
 await page.request.post(`/api/${status.bootstrap?'register':'login'}`,{headers,data:{username:'browser-test',password:'test1234'}})
 let p=await(await page.request.post('/api/projects',{headers,data:{name:'JavaScript test',mode:'freeform'}})).json()
 const compiled=await page.request.post(`/api/projects/${p.id}/scripts/compile`,{headers,data:{source}});expect(compiled.ok(),await compiled.text()).toBe(true)
 const script=await compiled.json()
 const node=(id:string,kind:string,x:number,parameters={})=>({id,kind,label:id,x,y:120,channels:1,parameters})
 p.parts=[];p.graph={nodes:[{...node('Script','js_control',150),script},node('Result','multiply',520,{b:2})],edges:script.outputs.some((v:any)=>v.name==='value')?[{id:'wire',source:'Script',source_port:'value',target:'Result',target_port:'a'}]:[]}
 const saved=await page.request.put(`/api/projects/${p.id}`,{headers,data:p});expect(saved.ok(),await saved.text()).toBe(true);p=await saved.json()
 let telemetry:any;page.on('websocket',s=>s.on('framereceived',({payload})=>{const v=JSON.parse(String(payload));if(v.type==='telemetry'&&v.project_id===p.id)telemetry=v}))
 await page.goto('/');await page.getByRole('button',{name:'Enable audio engine',exact:true}).click()
 await expect.poll(()=>telemetry?.scripts?.Script?.events??0).toBeGreaterThan(0)
 return {p,get:()=>telemetry,load:async()=>(await(await page.request.get(`/api/projects/${p.id}`)).json()).project}
}
async function edit(page:Page,source:string){
 const editor=page.getByRole('textbox',{name:'JavaScript source'});await editor.click();await editor.press('ControlOrMeta+a');await editor.fill(source)
}
test('IDE applies live ports, retains a failing draft, completes helpers, and logs to GUI Console',async({page})=>{
 const {p,get,load}=await setup(page,'const out=define_output("value");engine.on("ready",()=>{out.write(21);console.log("gui-log",{answer:42});});')
 await expect.poll(()=>get()?.values?.Result?._out).toBe(42)
 await page.getByRole('button',{name:'Edit Script',exact:true}).click()
 await expect(page.getByRole('textbox',{name:'JavaScript source'})).toBeVisible()
 await expect(page.locator('.script-code .cm-line span').first()).toBeVisible()
 const source='const input=define_input("amount", 9);\nconst out=define_output("value");\ninput.on("change",e=>out.write(input.read()));\nconsole.log("new-script");'
 await edit(page,source);await page.getByRole('button',{name:'Apply script',exact:true}).click()
 await expect.poll(()=>get()?.values?.Result?._out).toBe(18)
 await expect.poll(async()=>(await load()).graph.nodes[0].script.inputs[0].name).toBe('amount')
 await page.screenshot({path:'test-results/script-editor.png'})
 await edit(page,'const broken = ;');await page.getByRole('button',{name:'Apply script',exact:true}).click()
 await expect(page.locator('.script-error')).toContainText(/SyntaxError|expect/i)
 await expect.poll(()=>get()?.values?.Result?._out).toBe(18)
 expect((await load()).graph.nodes[0].script.source).toBe(source)
 await edit(page,'define_');await page.getByRole('textbox',{name:'JavaScript source'}).press('Control+Space')
 await expect(page.locator('.cm-tooltip-autocomplete')).toContainText('define_input')
 const consolePage=await page.context().newPage();await consolePage.goto(`/?console=${p.id}`)
 await expect(consolePage.getByRole('log')).toContainText('gui-log {"answer":42}')
 await expect(consolePage.getByRole('log')).toContainText('JavaScript [Script · Script] sample')
 await consolePage.close()
 const guide=await page.context().newPage();await guide.goto('/?help=1&topic=scripting');await expect(guide.getByRole('heading',{name:'JavaScript control scripting',exact:true})).toBeVisible();await guide.close()
 await page.getByRole('button',{name:'Close parameters'}).click();await page.getByRole('button',{name:'Disable audio engine',exact:true}).click()
})
test('worker failure leaves graph running and reapply restarts, forged manifests are rejected',async({page})=>{
 const {p,get,load}=await setup(page,'const out=define_output("value");let n=0;on_tick(()=>out.write(++n));')
 await expect.poll(()=>get()?.values?.Result?._out??0).toBeGreaterThan(0)
 const forged=await load();forged.graph.nodes[0].script.outputs[0].name='forged'
 const bad=await page.request.put(`/api/projects/${p.id}`,{headers,data:forged});expect(bad.ok()).toBe(false)
 await page.getByRole('button',{name:'Edit Script',exact:true}).click()
 await edit(page,'define_output("value");on_tick(()=>{while(true){}});');await page.getByRole('button',{name:'Apply script',exact:true}).click()
 await expect.poll(()=>get()?.scripts?.Script?.faulted).toBe(true)
 const sample=get().sample;await expect.poll(()=>get()?.sample??0).toBeGreaterThan(sample)
 await expect.poll(()=>get()?.values?.Result?._out).toBe(0)
 await edit(page,'const out=define_output("value");engine.on("ready",()=>out.write(17));');await page.getByRole('button',{name:'Apply script',exact:true}).click()
 await expect.poll(()=>get()?.values?.Result?._out).toBe(34)
 const first=(await load()).graph.nodes[0].script.revision
 await page.getByRole('button',{name:'Apply script',exact:true}).click();await expect.poll(async()=>(await load()).graph.nodes[0].script.revision).toBe(first+1)
 await expect.poll(()=>get()?.scripts?.Script?.faulted).toBe(false)
 await page.getByRole('button',{name:'Close parameters'}).click();await page.getByRole('button',{name:'Disable audio engine',exact:true}).click()
})
test('real OSC UDP and graph MIDI reach script bindings and named receivers',async({page})=>{
 const {p,get,load}=await setup(page,`const value=define_output("value");const bus=control.send("from-script");
 osc.bind("/complex").on("message",e=>{value.write(e.args[0].value);bus.write(e.args[0].value);console.log("OSC",e.args[1].value);});
 midi.on("note_on",e=>{value.write(e.note);console.log("MIDI",e.bytes);});
 bind_receive("from-script").on("change",e=>console.log("BUS",e.value));`)
 const project=await load();project.graph.nodes.push({id:'Keys',kind:'piano',label:'Keys',x:0,y:500,channels:1,parameters:{}},{id:'Receiver',kind:'receive_control',label:'Receiver',x:500,y:500,channels:1,parameters:{},control_value:'from-script'})
 project.graph.edges.push({id:'keys',source:'Keys',source_port:'midi',target:'Script',target_port:'midi'})
 expect((await page.request.put(`/api/projects/${p.id}`,{headers,data:project})).ok()).toBe(true)
 const sent=await page.request.put(`/api/projects/${p.id}/piano`,{headers,data:{node:'Keys',pitch:63,velocity:100}});expect(sent.ok()).toBe(true)
 await expect.poll(()=>get()?.values?.Result?._out).toBe(126)
 const probe=createSocket('udp4');await new Promise<void>(resolve=>probe.bind(0,'127.0.0.1',resolve));const port=(probe.address() as {port:number}).port;await new Promise<void>(resolve=>probe.close(()=>resolve()))
 const old=await(await page.request.get('/api/system/osc')).json();
 const configured=await page.request.put(`/api/projects/${p.id}/system/osc`,{headers,data:{...old.settings,receive_enabled:true,bind_addresses:['127.0.0.1'],receive_port:port}});expect(configured.ok(),await configured.text()).toBe(true)
 const sender=createSocket('udp4')
 try{
   const string=(value:string)=>{const b=Buffer.from(value+'\0');return Buffer.concat([b,Buffer.alloc((4-b.length%4)%4)])}
   const number=Buffer.alloc(4);number.writeFloatBE(0.75)
   const packet=Buffer.concat([string('/complex'),string(',fs'),number,string('all arguments')])
   await new Promise<void>((resolve,reject)=>sender.send(packet,port,'127.0.0.1',err=>err?reject(err):resolve()))
   await expect.poll(()=>get()?.values?.Result?._out).toBe(1.5)
   await expect.poll(()=>get()?.values?.Receiver?._out).toBe(0.75)
   await expect.poll(async()=>JSON.stringify(await(await page.request.get(`/api/projects/${p.id}/logs`)).json())).toContain('OSC all arguments')
   await expect.poll(async()=>JSON.stringify(await(await page.request.get(`/api/projects/${p.id}/logs`)).json())).toContain('BUS 0.75')
 }finally{sender.close();await page.request.put(`/api/projects/${p.id}/system/osc`,{headers,data:old.settings});}
 await page.getByRole('button',{name:'Disable audio engine',exact:true}).click()
})
