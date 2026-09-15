import {test,expect} from '@playwright/test'
const headers={'X-Pr0former':'1'}
test('script options keep the runtime log inside the scrolling list and expose MIDI passthrough',async({page})=>{
 test.setTimeout(120000)
 const status=await(await page.request.get('/api/status')).json()
 await page.request.post(`/api/${status.bootstrap?'register':'login'}`,{headers,data:{username:'browser-test',password:'test1234'}})
 const p=await(await page.request.post('/api/projects',{headers,data:{name:'Script modal',mode:'freeform'}})).json()
 const source='const out=define_output("value");engine.on("ready",()=>{out.write(1);for(let i=0;i<12;i++)console.log("line",i);});'
 const compiled=await page.request.post(`/api/projects/${p.id}/scripts/compile`,{headers,data:{source}});expect(compiled.ok(),await compiled.text()).toBe(true)
 p.parts=[];p.graph={nodes:[{id:'Script',kind:'js_control',label:'Script',x:150,y:120,channels:1,parameters:{},script:await compiled.json()}],edges:[]}
 const saved=await page.request.put(`/api/projects/${p.id}`,{headers,data:p});expect(saved.ok(),await saved.text()).toBe(true)
 let telemetry:any;page.on('websocket',s=>s.on('framereceived',({payload})=>{const v=JSON.parse(String(payload));if(v.type==='telemetry'&&v.project_id===p.id)telemetry=v}))
 await page.goto('/');await page.getByRole('button',{name:'Enable audio engine',exact:true}).click()
 await expect.poll(()=>telemetry?.scripts?.Script?.logs?.length??0).toBeGreaterThanOrEqual(12)
 await page.getByRole('button',{name:'Edit Script',exact:true}).click()
 const dialog=page.locator('dialog.script-modal'),list=dialog.locator('.parameter-list'),log=dialog.getByRole('log',{name:'Script console'}),footer=dialog.locator('.modal-footer')
 await expect(log).toContainText('line 11')
 await log.scrollIntoViewIfNeeded()
 await page.screenshot({path:'test-results/script-modal.png'})
 const [logBox,listBox,footerBox]=await Promise.all([log.boundingBox(),list.boundingBox(),footer.boundingBox()])
 // The log must sit inside the scrolling parameter list, above the footer, never spilling past either.
 expect(logBox!.y+logBox!.height,'log bottom vs list bottom').toBeLessThanOrEqual(listBox!.y+listBox!.height+1)
 expect(logBox!.y+logBox!.height,'log bottom vs footer top').toBeLessThanOrEqual(footerBox!.y+1)
 const passthru=dialog.getByRole('checkbox',{name:'MIDI passthrough',exact:true})
 await expect(passthru).toBeChecked()
 await passthru.scrollIntoViewIfNeeded();await page.screenshot({path:'test-results/script-modal-passthru.png'})
 await passthru.uncheck()
 await expect.poll(async()=>(await(await page.request.get(`/api/projects/${p.id}`)).json()).project.graph.nodes[0].parameters.midi_passthru).toBe(0)
 await expect(passthru).not.toBeChecked()
 await page.getByRole('button',{name:'Close parameters'}).click();await page.getByRole('button',{name:'Disable audio engine',exact:true}).click()
})
