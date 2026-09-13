import {test,expect} from '@playwright/test'

test('device toggles persist immediately, preserve drafts and restore failed changes',async({page})=>{
  const headers={'X-Pr0former':'1'},status=await(await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap?'register':'login'}`,{headers,data:{username:'browser-test',password:'test1234'}})
  const p=await(await page.request.post('/api/projects',{headers,data:{name:'Device preferences',mode:'freeform'}})).json()
  const saved={sample_rate:48000,block_size:128,interfaces:[{id:201,name:'Studio output',enabled:true,correct_latency:false,latency_ms:0}],input_interfaces:[{id:101,name:'Studio input',enabled:true}]}
  let fail=false,toggles=0
  // Simulated devices: no test may open the host's physical audio interfaces.
  await page.route('**/api/system/audio',route=>route.fulfill({json:saved}))
  await page.route('**/api/devices',route=>route.fulfill({json:{interfaces:saved.interfaces,input_interfaces:saved.input_interfaces,midi_inputs:[],midi_outputs:[]}}))
  await page.route('**/system/audio/device',route=>{
    const body=route.request().postDataJSON();toggles++
    expect(Object.keys(body).sort()).toEqual(['direction','enabled','id'])
    if(fail)return route.fulfill({status:400,json:{error:'Device unavailable'}})
    const devices=body.direction==='input'?saved.input_interfaces:saved.interfaces
    devices.find(d=>d.id===body.id)!.enabled=body.enabled
    return route.fulfill({json:saved})
  })
  await page.goto('/')
  await page.getByRole('button',{name:'Choose project'}).click()
  await page.getByRole('button',{name:p.name,exact:true}).click()
  const open=async()=>{
    await page.getByRole('button',{name:'Settings menu',exact:true}).click()
    await page.getByRole('menuitem',{name:'System settings',exact:true}).click()
  }
  await open()
  const dialog=page.getByRole('dialog',{name:'System settings',exact:true})
  await expect(dialog.getByRole('checkbox',{name:'Studio input'})).toBeChecked()
  await expect(dialog.getByRole('checkbox',{name:'Studio output'})).toBeChecked()
  await dialog.getByRole('combobox',{name:'Global sample rate (Hz)',exact:true}).selectOption('96000')
  await dialog.getByRole('checkbox',{name:'Studio input'}).uncheck()
  await expect.poll(()=>saved.input_interfaces[0]!.enabled).toBe(false)
  await expect(dialog.getByRole('combobox',{name:'Global sample rate (Hz)',exact:true})).toHaveValue('96000')
  expect(saved.sample_rate).toBe(48000)
  await dialog.getByRole('checkbox',{name:'Studio output'}).uncheck()
  await expect.poll(()=>saved.interfaces[0]!.enabled).toBe(false)
  await dialog.getByRole('button',{name:'Close system settings'}).click()
  await open()
  await expect(dialog.getByRole('checkbox',{name:'Studio input'})).not.toBeChecked()
  await expect(dialog.getByRole('checkbox',{name:'Studio output'})).not.toBeChecked()
  fail=true
  await dialog.getByRole('checkbox',{name:'Studio input'}).check()
  await expect(dialog.getByRole('alert')).toContainText('Device unavailable')
  await expect(dialog.getByRole('checkbox',{name:'Studio input'})).not.toBeChecked()
  expect(toggles).toBe(3)
})

test('device preference API validates requests and retains choices across discovery',async({request})=>{
  const headers={'X-Pr0former':'1'},status=await(await request.get('/api/status')).json()
  await request.post(`/api/${status.bootstrap?'register':'login'}`,{headers,data:{username:'browser-test',password:'test1234'}})
  const p=await(await request.post('/api/projects',{headers,data:{name:'Device API',mode:'freeform'}})).json()
  const url=`/api/projects/${p.id}/system/audio`
  const saved={sample_rate:48000,block_size:128,interfaces:[{id:201,name:'Disconnected output',enabled:false,correct_latency:true,latency_ms:12}],input_interfaces:[{id:101,name:'Disconnected input',enabled:false}]}
  expect((await request.put(url,{headers,data:saved})).ok()).toBeTruthy()
  const change={direction:'input',id:101,enabled:false}
  expect((await request.put(`${url}/device`,{data:change})).ok()).toBeFalsy()
  expect((await request.put(`${url}/device`,{headers,data:{...change,direction:'wrong'}})).status()).toBe(400)
  expect((await request.put(`${url}/device`,{headers,data:{...change,id:999}})).status()).toBe(400)
  expect((await request.put(`${url}/device`,{headers,data:{...change,enabled:true}})).status()).toBe(400)
  expect((await request.put(`${url}/device`,{headers,data:change})).ok()).toBeTruthy()
  await request.get('/api/devices')
  expect(await(await request.get('/api/system/audio')).json()).toEqual(saved)
  expect((await request.put(url,{headers,data:{...saved,interfaces:[],input_interfaces:[]}})).ok()).toBeTruthy()
})
