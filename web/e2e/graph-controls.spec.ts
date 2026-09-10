import {test,expect} from '@playwright/test'

test('graphical controls, group context menu, duplication and live subgraph conversion',async({page})=>{
  const headers={'X-Pr0former':'1'},status=await(await page.request.get('/api/status')).json()
  expect(status.build.git_commit).toMatch(/^[a-f0-9]{40}$/)
  await page.request.post(`/api/${status.bootstrap?'register':'login'}`,{headers,data:{username:'browser-test',password:'test1234'}})
  let p=await(await page.request.post('/api/projects',{headers,data:{name:'Graph controls',mode:'freeform'}})).json()
  const n=(id:string,kind:string,x:number,y:number,parameters={},control_value?:number|string)=>({id,kind,label:id,x,y,channels:1,parameters,control_value})
  p.parts=[];p.graph={nodes:[n('A','control_input',0,0,{mode:2,min:0,max:10},2),n('B','add',350,0,{b:3}),n('Result','control_input',700,0,{mode:2}),n('Manual','control_input',0,350,{mode:2,min:0,max:10},0),n('Count','counter',350,350)],edges:[{id:'ab',source:'A',source_port:'out',target:'B',target_port:'a'},{id:'br',source:'B',source_port:'out',target:'Result',target_port:'in'}]}
  p=await(await page.request.put(`/api/projects/${p.id}`,{headers,data:p})).json()
  const load=async()=>(await(await page.request.get(`/api/projects/${p.id}`)).json()).project
  let latest:any;page.on('websocket',socket=>socket.on('framereceived',({payload})=>{const m=JSON.parse(String(payload));if(m.type==='telemetry')latest=m}))
  await page.goto('/');await page.getByRole('button',{name:'Play',exact:true}).click()
  await expect.poll(()=>latest?.values?.Result?._out).toBe(5)
  await expect(page.getByLabel('Result value')).toHaveCount(0)
  await page.getByLabel('A value').fill('4.5');await page.getByLabel('A value').press('Tab')
  await expect.poll(()=>latest?.values?.Result?._out).toBe(7.5)
  const select=async()=>{await page.locator('.vue-flow__pane').click({position:{x:10,y:100}});await page.locator('.vue-flow__node[data-id="A"] .patch-node').click({position:{x:35,y:45}});await page.keyboard.down('Control');await page.locator('.vue-flow__node[data-id="B"] .patch-node').click({position:{x:35,y:45}});await page.keyboard.up('Control');await expect(page.locator('.vue-flow__node.selected')).toHaveCount(2)}
  await select()
  await page.locator('.vue-flow__node[data-id="A"] .patch-node').focus();await page.keyboard.press('d')
  await expect.poll(async()=>(await load()).graph.nodes.length).toBe(7)
  await page.getByRole('button',{name:'Undo',exact:true}).click();await expect.poll(async()=>(await load()).graph.nodes.length).toBe(5)
  await select();await page.locator('.vue-flow__node[data-id="A"] .patch-node').click({button:'right',position:{x:30,y:45}})
  await expect(page.getByRole('menuitem')).toHaveCount(3)
  await expect(page.getByRole('menuitem',{name:'Make subgraph',exact:true})).toBeVisible()
  await page.getByRole('menuitem',{name:'Make subgraph',exact:true}).click()
  await expect.poll(async()=>(await load()).graph.nodes.find((n:any)=>n.id==='A').parent).toBeTruthy()
  await expect.poll(()=>latest?.values?.Result?._out).toBe(7.5)
  const grouped=await load();expect(grouped.graph.nodes.find((n:any)=>n.id==='B').parent).toBe(grouped.graph.nodes.find((n:any)=>n.id==='A').parent)
  await page.getByRole('button',{name:'Edit Manual',exact:true}).click()
  await expect(page.getByRole('spinbutton',{name:'Minimum',exact:true})).toBeVisible()
  await expect(page.getByRole('spinbutton',{name:'Maximum',exact:true})).toBeVisible()
  await page.getByLabel('Control type').selectOption('1');await expect.poll(async()=>(await load()).graph.nodes.find((n:any)=>n.id==='Manual').parameters.mode).toBe(1)
  await page.getByRole('button',{name:'Close parameters'}).click()
  await page.getByLabel('Manual value').fill('2.5');await page.getByLabel('Manual value').press('Tab');await expect(page.getByText('Enter an integer from 0 to 10')).toBeVisible()
  await page.getByLabel('Manual value').fill('8');await page.getByLabel('Manual value').press('Tab');await expect.poll(()=>latest?.values?.Manual?._out).toBe(8)
  const editMode=async(mode:string)=>{await page.getByRole('button',{name:'Edit Manual',exact:true}).click();await page.getByLabel('Control type').selectOption(mode);await expect.poll(async()=>(await load()).graph.nodes.find((n:any)=>n.id==='Manual').parameters.mode).toBe(Number(mode));await page.getByRole('button',{name:'Close parameters'}).click()}
  await editMode('3');await page.getByLabel('Manual slider').evaluate((element:HTMLInputElement)=>{element.value='6';element.dispatchEvent(new Event('input',{bubbles:true}))});await expect.poll(()=>latest?.values?.Manual?._out).toBe(6)
  await editMode('4');await page.getByLabel('Manual value').fill('hello');await page.getByLabel('Manual value').press('Tab');await expect.poll(()=>latest?.visualizations?.Manual?.value).toBe('hello')
  await editMode('0')
  const current=await load();current.graph.edges.push({id:'bang',source:'Manual',source_port:'out',target:'Count',target_port:'trigger'});expect((await page.request.put(`/api/projects/${p.id}`,{headers,data:current})).ok()).toBe(true)
  await expect(page.getByLabel('Trigger Manual')).toBeEnabled();const before=latest?.values?.Count?._out||0
  await page.getByLabel('Trigger Manual').click();await expect.poll(()=>latest?.values?.Count?._out).toBe(before+1)
  const revision=(await load()).revision
  expect((await page.request.put(`/api/projects/${p.id}/control`,{headers,data:{node:'Result',revision,value:1}})).status()).toBe(400)
  await page.getByRole('button', { name: 'Disable audio engine', exact: true }).click()
})
