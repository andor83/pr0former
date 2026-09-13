import {test,expect} from '@playwright/test'
const headers={'X-Pr0former':'1'}
test('owners and editors assign local inputs; only assignees configure and send; disconnected microphones are gray',async({page,browser})=>{
 test.setTimeout(45000)
 await page.addInitScript(()=>{navigator.mediaDevices.getUserMedia=async()=>{throw new DOMException('Fixture permission denied','NotAllowedError')}})
 const status=await(await page.request.get('/api/status')).json();await page.request.post(`/api/${status.bootstrap?'register':'login'}`,{headers,data:{username:'browser-test',password:'test1234'}})
 const owner=await(await page.request.get('/api/me')).json()
 let p=await(await page.request.post('/api/projects',{headers,data:{name:'Input assignment permissions',mode:'freeform'}})).json()
 p.parts=[];p.graph={nodes:[{id:'mic',kind:'browser_input',label:'Local audio input',x:0,y:0,channels:2,parameters:{}}],edges:[]}
 p=await(await page.request.put(`/api/projects/${p.id}`,{headers,data:p})).json()
 const clients=[] as Awaited<ReturnType<typeof browser.newContext>>[]
 const actors=[] as {id:string;username:string;role:string;context:Awaited<ReturnType<typeof browser.newContext>>}[]
 const load=async()=>(await(await page.request.get(`/api/projects/${p.id}`)).json()).project
 try{
  for(const role of ['editor','performer','conductor']){
   const username=`input-${role}-${Date.now()}`;await page.request.post('/api/admin/users',{headers,data:{username,password:'password123',is_admin:false,enabled:true,fields:{}}})
   const context=await browser.newContext({baseURL:'http://127.0.0.1:3101'});clients.push(context);await context.addInitScript(()=>{navigator.mediaDevices.getUserMedia=async()=>{throw new DOMException('Fixture permission denied','NotAllowedError')}});await context.request.post('/api/login',{headers,data:{username,password:'password123'}})
   const me=await(await context.request.get('/api/me')).json();expect((await page.request.post(`/api/projects/${p.id}/members`,{headers,data:{user_id:me.id,role}})).ok()).toBeTruthy();actors.push({id:me.id,username,role,context})
  }
  await page.goto('/');const mic=page.getByRole('button',{name:'Mute local audio input',exact:true});await expect(mic).toHaveClass(/offline/);await expect(mic).toBeDisabled()
  await page.getByRole('button',{name:'Edit Local audio input',exact:true}).click();const select=page.getByLabel('Assigned ensemble user');await expect(select).toBeEnabled();await expect(page.locator('.parameter-modal').getByLabel('Local audio input device',{exact:true})).toHaveCount(0)
  await select.selectOption(actors[1]!.id);await expect.poll(async()=>(await load()).local_audio_assignments.mic).toBe(actors[1]!.id)
  const performer=await actors[1]!.context.newPage();await performer.goto('/');await performer.getByRole('button',{name:'Edit Local audio input',exact:true}).click();await expect(performer.getByLabel('Assigned ensemble user')).toBeDisabled();await expect(performer.locator('.parameter-modal').getByLabel('Local audio input device',{exact:true})).toBeEnabled()
  await page.request.post(`/api/projects/${p.id}/engine`,{headers,data:{enabled:true}})
  expect((await page.request.post(`/api/projects/${p.id}/media`,{headers,data:{sdp:'invalid',input_node:'mic'}})).status()).toBe(403)
  for(const actor of actors){const next=await load();next.local_audio_assignments={mic:actor.id};const response=await actor.context.request.put(`/api/projects/${p.id}`,{headers,data:next});expect(response.status()).toBe(actor.role==='editor'?200:403)}
  await expect(performer.locator('.parameter-modal').getByLabel('Local audio input device',{exact:true})).toHaveCount(0)
  const next=await load();next.local_audio_assignments={mic:'not-a-member'};expect((await page.request.put(`/api/projects/${p.id}`,{headers,data:next})).status()).toBe(400)
  const editor=await actors[0]!.context.newPage();await editor.goto('/');await editor.getByRole('button',{name:'Edit Local audio input',exact:true}).click();await expect(editor.getByLabel('Assigned ensemble user')).toBeEnabled();await expect(editor.locator('.parameter-modal').getByLabel('Local audio input device',{exact:true})).toBeEnabled()
  await editor.getByLabel('Assigned ensemble user').selectOption(owner.id);await expect.poll(async()=>(await load()).local_audio_assignments.mic).toBe(owner.id)
  await expect(page.locator('.parameter-modal').getByLabel('Local audio input device',{exact:true})).toBeEnabled()
  await page.screenshot({path:'test-results/input-assignment.png'})
 }finally{for(const c of clients)await c.close().catch(()=>{});await page.request.post(`/api/projects/${p.id}/engine`,{headers,data:{enabled:false},timeout:5000}).catch(()=>{})}
})
