import {test,expect} from '@playwright/test'
const headers={'X-Pr0former':'1'}
test('container frames sit behind nodes, snap and grow around dropped nodes, carry members, resize and explain themselves',async({page})=>{
 test.setTimeout(120000)
 const status=await(await page.request.get('/api/status')).json()
 await page.request.post(`/api/${status.bootstrap?'register':'login'}`,{headers,data:{username:'browser-test',password:'test1234'}})
 const p=await(await page.request.post('/api/projects',{headers,data:{name:'Container frames',mode:'freeform'}})).json()
 const node=(id:string,kind:string,x:number,y:number,parameters:Record<string,number>={},extra:Record<string,unknown>={})=>({id,kind,label:id,x,y,channels:1,parameters,...extra})
 p.parts=[];p.graph={nodes:[node('Group','container',100,100,{color:0,width:520,height:320}),node('Inside','oscillator',116,160),node('Loose','oscillator',900,120)],edges:[]}
 const saved=await page.request.put(`/api/projects/${p.id}`,{headers,data:p});expect(saved.ok(),await saved.text()).toBe(true)
 const load=async()=>(await(await page.request.get(`/api/projects/${p.id}`)).json()).project
 const find=async(id:string)=>(await load()).graph.nodes.find((n:any)=>n.id===id)
 await page.goto('/')
 const frame=page.locator('.vue-flow__node[data-id="Group"]'),inside=page.locator('.vue-flow__node[data-id="Inside"]'),loose=page.locator('.vue-flow__node[data-id="Loose"]')
 await expect(frame.locator('.container-node')).toBeVisible()
 // Behind ordinary nodes.
 const z=async(l:typeof frame)=>Number(await l.evaluate(el=>getComputedStyle(el).zIndex))
 expect(await z(frame)).toBeLessThan(await z(inside))
 // The engine tolerates a UI-only node.
 await page.getByRole('button',{name:'Enable audio engine',exact:true}).click()
 await expect(page.getByRole('button',{name:'Disable audio engine',exact:true})).toBeVisible()
 // Moving the frame carries the node inside it and leaves the loose node alone.
 // A tiny first step spends the drag threshold so the real distance lands exactly.
 const drag=async(l:typeof frame,dx:number,dy:number,at={x:0.5,y:0.5})=>{const b=(await l.boundingBox())!;const x=b.x+b.width*at.x,y=b.y+b.height*at.y;await page.mouse.move(x,y);await page.mouse.down();await page.mouse.move(x+2,y+2,{steps:2});await page.mouse.move(x+dx/2,y+dy/2,{steps:4});await page.mouse.move(x+dx,y+dy,{steps:6});await page.mouse.up()}
 // Patch nodes are 204 flow units wide, so their rendered width gives the zoom.
 const zoom=(await inside.locator('.patch-node').boundingBox())!.width/204
 const before={group:await find('Group'),inside:await find('Inside'),loose:await find('Loose')}
 // Pointer drags lose their first step to the drag threshold, so distances are checked loosely; the member/frame deltas must match exactly.
 await drag(frame,120*zoom,0,{x:0.5,y:0.06})
 await expect.poll(async()=>(await find('Group')).x).toBeGreaterThan(before.group.x+60)
 const after={group:await find('Group'),inside:await find('Inside'),loose:await find('Loose')}
 expect(after.inside.x-before.inside.x).toBeCloseTo(after.group.x-before.group.x,0)
 expect(after.inside.y-before.inside.y).toBeCloseTo(after.group.y-before.group.y,0)
 expect(after.loose).toMatchObject({x:before.loose.x,y:before.loose.y})
 // Dropping a node into the frame snaps it to the grid and grows the frame if needed.
 const frameBox=(await frame.boundingBox())!,looseBox=(await loose.boundingBox())!
 await drag(loose,frameBox.x+frameBox.width*0.62-looseBox.x,frameBox.y+frameBox.height*0.72-looseBox.y,{x:0.5,y:0.1})
 await expect.poll(async()=>(await find('Loose')).x).toBeLessThan(after.group.x+(await find('Group')).parameters.width)
 const dropped=await find('Loose'),grown=await find('Group')
 expect(Math.round(dropped.x-grown.x-16)%228).toBe(0)
 expect(Math.round(dropped.y-grown.y-44-16)%24).toBe(0)
 expect(grown.parameters.width).toBeGreaterThanOrEqual(dropped.x-grown.x+204+16)
 expect(grown.parameters.height).toBeGreaterThan(320)
 // Resizing from the bottom-right corner persists the new size.
 const handle=frame.locator('.container-resize'),hb=(await handle.boundingBox())!
 await page.mouse.move(hb.x+hb.width/2,hb.y+hb.height/2);await page.mouse.down();await page.mouse.move(hb.x+hb.width/2+150*zoom,hb.y+hb.height/2+90*zoom,{steps:8});await page.mouse.up()
 await expect.poll(async()=>(await find('Group')).parameters.width).toBeGreaterThan(grown.parameters.width+80)
 expect((await find('Group')).parameters.height).toBeGreaterThan(grown.parameters.height+40)
 await page.screenshot({path:'test-results/container-graph.png'})
 // Options: sixteen colours and an explanation that feeds the info hover beside the title.
 await frame.getByRole('button',{name:'Edit Group',exact:true}).click()
 const modal=page.getByRole('dialog',{name:/^Group/})
 await expect(modal.getByRole('radio')).toHaveCount(16)
 await modal.getByRole('radio',{name:'Amber',exact:true}).click()
 await expect.poll(async()=>(await find('Group')).parameters.color).toBe(5)
 // Explanations are documentation, not control text: well over the 256-byte control bound must persist intact.
 const essay='Two oscillators that feed the mix. '+'Detune the second slightly and ride the amplitude for movement. '.repeat(12)
 expect(essay.length).toBeGreaterThan(600)
 await modal.getByLabel('Container explanation',{exact:true}).fill(essay)
 await modal.getByLabel('Container explanation',{exact:true}).press('Tab')
 await expect.poll(async()=>(await find('Group')).control_value).toBe(essay.trim())
 await expect(page.getByRole('alert')).toHaveCount(0)
 await page.screenshot({path:'test-results/container-options.png'})
 await page.getByRole('button',{name:'Close parameters'}).click()
 await frame.getByRole('button',{name:'About Group',exact:true}).hover()
 await expect(page.getByRole('tooltip')).toContainText('Two oscillators that feed the mix.')
 await page.getByRole('button',{name:'Disable audio engine',exact:true}).click()
})
test('the container is catalogued under Layout and its documentation example renders the frame',async({page})=>{
 const status=await(await page.request.get('/api/status')).json()
 await page.request.post(`/api/${status.bootstrap?'register':'login'}`,{headers,data:{username:'browser-test',password:'test1234'}})
 const catalog=await(await page.request.get('/api/catalog')).json()
 expect(catalog.find((d:any)=>d.kind==='container')).toMatchObject({category:'Layout',inputs:[],outputs:[]})
 await page.goto('/?help=1')
 await page.getByRole('button',{name:/^All nodes/}).click()
 await page.getByLabel('Search node documentation').fill('Container')
 await page.locator('.node-doc-card').filter({has:page.getByRole('heading',{name:'Container',exact:true})}).getByRole('button',{name:'Show example graph'}).click()
 const example=page.getByRole('dialog',{name:'Container example graph',exact:true})
 await expect(example.locator('.container-node')).toBeVisible()
 await expect(example.locator('.vue-flow__node')).toHaveCount(3)
 await page.screenshot({path:'test-results/container-docs.png'})
})
