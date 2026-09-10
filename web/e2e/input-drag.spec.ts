import {test,expect} from '@playwright/test'
test('input drags move whole bundles, disconnect off-input, cancel, and preserve output creation',async({page})=>{
  const headers={'X-Pr0former':'1'},status=await(await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap?'register':'login'}`,{headers,data:{username:'browser-test',password:'test1234'}})
  const p=await(await page.request.post('/api/projects',{headers,data:{name:'Move inputs',mode:'freeform'}})).json()
  const node=(id:string,kind:string,x:number,y:number)=>({id,kind,label:id,x,y,channels:1,parameters:{}})
  const edge=(id:string,source:string)=>({id,source,source_port:'out',target:'Old',target_port:'in'})
  p.parts=[];p.graph={nodes:[node('A','value',0,0),node('B','value',0,180),node('Old','toggle',300,0),node('New','toggle',550,180),node('Audio','gain',550,350)],edges:[edge('a','A'),edge('b','B')]}
  expect((await page.request.put(`/api/projects/${p.id}`,{headers,data:p})).ok()).toBe(true)
  const load=async()=>(await(await page.request.get(`/api/projects/${p.id}`)).json()).project
  const handle=(id:string,port:string)=>page.locator(`.vue-flow__node[data-id="${id}"] .vue-flow__handle[data-handleid="${port}"]`)
  const point=async(id:string,port:string)=>{await expect(handle(id,port)).toHaveClass(/\bconnectable\b/);const box=(await handle(id,port).boundingBox())!;return {x:box.x+box.width/2,y:box.y+box.height/2}}
  const drag=async(from:{x:number;y:number},to:{x:number;y:number})=>{await page.mouse.move(from.x,from.y);await page.mouse.down();await page.mouse.move(to.x,to.y,{steps:12});await page.mouse.up()}
  await page.goto('/');await expect(handle('Old','in')).toBeVisible()
  // Let the graph's fit-view settle so handle positions do not move mid-drag.
  for(let previous=await handle('Old','in').boundingBox();;){await page.waitForTimeout(150);const next=await handle('Old','in').boundingBox();if(previous&&next&&previous.x===next.x&&previous.y===next.y)break;previous=next}
  await drag(await point('Old','in'),await point('New','in'))
  await expect.poll(async()=>(await load()).graph.edges.map((e:any)=>e.target)).toEqual(['New','New'])
  await page.getByRole('button',{name:'Undo',exact:true}).click()
  await expect.poll(async()=>(await load()).graph.edges.map((e:any)=>e.target)).toEqual(['Old','Old'])
  // Empty input cannot create a reverse connection when dragged to an output.
  await drag(await point('New','in'),await point('A','out'))
  expect((await load()).graph.edges).toHaveLength(2)
  // Invalid input types are rejected by the server, restoring the original bundle.
  await drag(await point('Old','in'),await point('Audio','in'))
  await expect(page.locator('.error-banner')).toBeVisible()
  expect((await load()).graph.edges.map((e:any)=>e.target)).toEqual(['Old','Old'])
  // Escape cancels rather than committing a disconnect.
  const start=await point('Old','in');await page.mouse.move(start.x,start.y);await page.mouse.down();await page.mouse.move(start.x+100,start.y+90,{steps:8});await page.keyboard.press('Escape');await page.mouse.up()
  expect((await load()).graph.edges).toHaveLength(2)
  // Real Chromium touch events exercise pointer capture and input relocation.
  const cdp=await page.context().newCDPSession(page),a=await point('Old','in'),b=await point('New','in')
  await cdp.send('Input.dispatchTouchEvent',{type:'touchStart',touchPoints:[a]})
  await cdp.send('Input.dispatchTouchEvent',{type:'touchMove',touchPoints:[b]})
  await cdp.send('Input.dispatchTouchEvent',{type:'touchEnd',touchPoints:[]})
  await expect.poll(async()=>(await load()).graph.edges.map((e:any)=>e.target)).toEqual(['New','New'])
  // Output drag adds a fresh edge without moving either existing connection.
  await expect(page.locator('.error-banner')).not.toBeVisible()
  await drag(await point('A','out'),await point('Old','in'))
  await expect.poll(async()=>(await load()).graph.edges.length).toBe(3)
  await drag(await point('New','in'),{x:900,y:820})
  await expect.poll(async()=>(await load()).graph.edges.length).toBe(1)
  await page.getByRole('button',{name:'Undo',exact:true}).click()
  await expect.poll(async()=>(await load()).graph.edges.length).toBe(3)
  // An output is not a valid drop target for an input bundle.
  await drag(await point('New','in'),await point('B','out'))
  await expect.poll(async()=>(await load()).graph.edges.length).toBe(1)
  // Tapping ports still works, including a quick follow-up after a drag.
  await point('New','in');await handle('New','in').click();await handle('B','out').click()
  await expect.poll(async()=>(await load()).graph.edges.length).toBe(2)
})
