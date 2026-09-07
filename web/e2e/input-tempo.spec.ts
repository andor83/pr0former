import { test, expect } from '@playwright/test'

test('input device availability, local browser selection, and connected global tempo', async ({ page }) => {
  const headers = { 'X-Pr0former': '1' }
  const status = await (await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap ? 'register' : 'login'}`, { headers, data: { username: 'browser-test', password: 'test1234' } })
  let project = await (await page.request.post('/api/projects', { headers, data: { name: 'Inputs and tempo', mode: 'freeform' } })).json()
  const node = (id:string,kind:string,x:number,parameters={}) => ({id,kind,label:id,x,y:0,channels:2,parameters})
  project.parts=[]
  project.graph={nodes:[node('Native','input',0),node('Browser','browser_input',300),node('Tempo source','add',600,{a:75,b:0}),node('Clock','clock',900)],edges:[{id:'tempo',source:'Tempo source',source_port:'out',target:'Clock',target_port:'tempo'}]}
  expect((await page.request.put(`/api/projects/${project.id}`, {headers,data:project})).ok()).toBe(true)
  let latest:any
  page.on('websocket', socket=>socket.on('framereceived',({payload})=>{const m=JSON.parse(String(payload));if(m.type==='telemetry')latest=m}))
  await page.addInitScript(() => {
    ;(window as any).__devices=[]
    navigator.mediaDevices.enumerateDevices=async()=>(window as any).__devices
    navigator.permissions.query=async()=>({state:'prompt'} as PermissionStatus)
    navigator.mediaDevices.getUserMedia=async(constraints)=>{(window as any).__constraints=constraints;throw new DOMException('blocked','NotAllowedError')}
  })
  await page.route('**/api/devices', async route => {
    const response=await route.fetch(), data=await response.json()
    await route.fulfill({json:{...data,input_interfaces:[],active_inputs:[],input_enabled:false}})
  })
  await page.goto('/')
  await page.getByRole('button',{name:'Edit Native',exact:true}).click()
  await expect(page.getByText('No native audio inputs are available on the server.')).toBeVisible()
  await page.unroute('**/api/devices')
  await page.route('**/api/devices',async route=>{const response=await route.fetch();await route.fulfill({json:{...await response.json(),input_interfaces:[{id:123,name:'Test native input'}],active_inputs:[]}})})
  await page.getByRole('button',{name:'Refresh native inputs'}).click()
  await expect(page.getByText('No native inputs are enabled. Enable inputs in System settings and save.')).toBeVisible()
  await expect(page.getByLabel('Input interface').locator('option[value="123"]')).toBeDisabled()
  await page.getByRole('button',{name:'Close parameters'}).click()
  await page.getByRole('button',{name:'Edit Browser',exact:true}).click()
  await expect(page.getByText('No browser audio inputs are available or exposed.',{exact:false})).toBeVisible()
  await page.getByRole('button',{name:'Grant microphone access'}).click()
  await expect(page.getByRole('alert')).toContainText('Microphone permission is denied')
  await page.evaluate(()=>{(window as any).__devices=[{kind:'audioinput',deviceId:'mic-a',label:'USB mic'},{kind:'audioinput',deviceId:'mic-b',label:'Built-in mic'},{kind:'audiooutput',deviceId:'out',label:'Speakers'}]})
  await page.getByRole('button',{name:'Refresh browser inputs'}).click()
  await expect(page.getByLabel('Browser microphone device').locator('option')).toHaveCount(3)
  await page.getByLabel('Browser microphone device').selectOption('mic-b')
  await page.getByRole('button',{name:'Close parameters'}).click()
  await page.getByRole('button',{name:'Play',exact:true}).click()
  await expect.poll(()=>latest?.bpm).toBe(75)
  await expect(page.locator('input#tempo')).toBeDisabled()
  expect((await page.request.post(`/api/projects/${project.id}/transport`,{headers,data:{action:'tempo',bpm:90}})).ok()).toBe(false)
  await page.getByRole('button',{name:'Edit Clock',exact:true}).click()
  await expect(page.getByRole('dialog').getByText('CONNECTED · 75 BPM')).toBeVisible()
  await page.getByRole('button',{name:'Close parameters'}).click()
  await page.getByRole('button',{name:'Monitor',exact:true}).click()
  await page.getByLabel('Send microphone to the graph',{exact:true}).check()
  await page.getByLabel('Browser input node',{exact:true}).selectOption('Browser')
  await expect(page.getByLabel('Browser microphone device')).toHaveValue('mic-b')
  await page.getByRole('button',{name:'Connect monitor',exact:true}).click()
  await expect(page.getByRole('alert')).toContainText('Microphone permission is denied')
  expect(await page.evaluate(()=>(window as any).__constraints.audio.deviceId)).toEqual({exact:'mic-b'})
  // Device disappearance is explicit; never silently substitute another mic.
  await page.evaluate(()=>{(window as any).__devices=[]})
  await page.getByRole('button',{name:'Refresh browser inputs'}).click()
  await expect(page.getByLabel('Browser microphone device').locator('option:checked')).toHaveText('Selected input unavailable — refresh and choose another')
  await page.getByRole('button',{name:'Deactivate show',exact:true}).click()
})
