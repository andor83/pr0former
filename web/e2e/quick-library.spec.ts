import {test,expect} from '@playwright/test'
const headers={'X-Pr0former':'1'}
test('quick search inserts saved groups and imports owned samples from another project',async({page})=>{
 const status=await(await page.request.get('/api/status')).json()
 await page.request.post(`/api/${status.bootstrap?'register':'login'}`,{headers,data:{username:'browser-test',password:'test1234'}})
 let origin=await(await page.request.post('/api/projects',{headers,data:{name:'Spotlight source',mode:'freeform'}})).json()
 origin.parts=[];origin.graph={nodes:[{id:'group',kind:'subgraph',label:'Spotlight group',x:0,y:0,channels:1,parameters:{}},{id:'value',kind:'value',parent:'group',label:'Inside',x:0,y:0,channels:1,parameters:{value:13}}],edges:[]}
 origin=await(await page.request.put(`/api/projects/${origin.id}`,{headers,data:origin})).json()
 const group=await page.request.post(`/api/projects/${origin.id}/subgraphs`,{headers,data:{node:'group',revision:origin.revision,name:'Spotlight saved group',public:false}});expect(group.ok()).toBe(true)
 const wav=Buffer.alloc(44+480*2);wav.write('RIFF');wav.writeUInt32LE(wav.length-8,4);wav.write('WAVEfmt ',8);wav.writeUInt32LE(16,16);wav.writeUInt16LE(1,20);wav.writeUInt16LE(1,22);wav.writeUInt32LE(48000,24);wav.writeUInt32LE(96000,28);wav.writeUInt16LE(2,32);wav.writeUInt16LE(16,34);wav.write('data',36);wav.writeUInt32LE(wav.length-44,40)
 expect((await page.request.post(`/api/projects/${origin.id}/samples`,{headers,multipart:{sample:{name:'Spotlight bell.wav',mimeType:'audio/wav',buffer:wav}}})).ok()).toBe(true)
 const sample=(await(await page.request.get(`/api/projects/${origin.id}/samples`)).json())[0]
 expect((await page.request.put(`/api/projects/${origin.id}/samples/${sample.id}`,{headers,data:{...sample,root_note:67}})).ok()).toBe(true)
 let p=await(await page.request.post('/api/projects',{headers,data:{name:'Spotlight destination',mode:'freeform'}})).json()
 p.parts=[];p.graph={nodes:[],edges:[]};expect((await page.request.put(`/api/projects/${p.id}`,{headers,data:p})).ok()).toBe(true)
 const load=async()=>(await(await page.request.get(`/api/projects/${p.id}`)).json()).project
 await page.goto('/')
 const browser=page.getByRole('dialog',{name:'Quick node browser'}),input=browser.getByRole('combobox')
 await page.locator('.graph-canvas').click({position:{x:500,y:100}});await page.keyboard.press('n');await expect(input).toBeFocused()
 await input.fill('Spotlight saved group')
 await expect(browser.getByRole('option',{name:'Spotlight saved group · Node group',exact:true})).toHaveCount(1)
 await input.press('Enter')
 await expect.poll(async()=>(await load()).graph.nodes.filter((n:any)=>n.kind==='subgraph').length).toBe(1)
 await page.locator('.graph-canvas').click({position:{x:500,y:100}});await page.keyboard.press('n');await expect(input).toBeFocused()
 await input.fill('Spotlight bell')
 await expect(browser.getByRole('option')).toHaveCount(1)
 await input.press('Enter')
 await expect.poll(async()=>(await load()).graph.nodes.filter((n:any)=>n.kind==='poly_sampler').length).toBe(1)
 const sampler=(await load()).graph.nodes.find((n:any)=>n.kind==='poly_sampler')
 expect(sampler.parameters.root_note).toBe(67)
 const linked=await(await page.request.get(`/api/projects/${p.id}/samples`)).json();expect(linked).toHaveLength(1);expect(linked[0].id).toBe(sample.id);expect(sampler.parameters.asset).toBe(linked[0].asset)
})
