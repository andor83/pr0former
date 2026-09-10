import { test, expect, type Page } from '@playwright/test'
import { readFile } from 'node:fs/promises'
async function setup(page: Page) {
  const headers = { 'X-Pr0former':'1' }
  const status = await (await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap?'register':'login'}`,{headers,data:{username:'browser-test',password:'test1234'}})
  const p = await (await page.request.post('/api/projects',{headers,data:{name:'Score save barrier',mode:'structured'}})).json()
  p.parts[0].notes=[]
  await page.request.put(`/api/projects/${p.id}`,{headers,data:p})
  await page.goto('/')
  await page.getByRole('button',{name:'Score & Parts',exact:true}).click()
  await page.getByLabel('Text and tempo',{exact:true}).click()
  await page.getByRole('button',{name:'Text tool',exact:true}).click()
  return `/api/projects/${p.id}`
}
async function text(page: Page, value: string, x: number) {
  await page.locator('[data-staff-id]').first().click({position:{x,y:118}})
  await page.getByLabel('Score text',{exact:true}).fill(value)
  await page.getByLabel('Score text',{exact:true}).press('Enter')
}
for (const fail of [false,true]) test(`pending score saves ${fail?'retain the newest failed draft across tabs':'block export until newer edits are saved'}`,async({page})=>{
  const url=await setup(page)
  let release!:()=>void
  const gate=new Promise<void>(resolve=>{release=resolve})
  let intercepted=false
  await page.route(`**${url}`,async route=>{
    if(route.request().method()!=='PUT'||intercepted) return route.continue()
    intercepted=true
    await gate
    if(fail) await route.fulfill({status:400,contentType:'application/json',body:JSON.stringify({error:'Test validation failure'})})
    else await route.continue()
  })
  await text(page,'First phrase',350)
  await expect.poll(()=>intercepted).toBe(true)
  await text(page,'Newest phrase',650)
  if(!fail){
    let downloaded=false
    page.on('download',()=>{downloaded=true})
    const download=page.waitForEvent('download')
    await page.getByRole('button',{name:'MusicXML',exact:true}).click()
    expect(downloaded).toBe(false)
    release()
    const file=await download
    const xml=await readFile((await file.path())!,'utf8')
    expect(xml).toContain('First phrase');expect(xml).toContain('Newest phrase')
  }else{
    release()
    await expect(page.getByText('Your draft is retained.',{exact:false})).toBeVisible()
    await page.getByRole('button',{name:'Signal Graph',exact:true}).click()
    await expect(page.locator('.save-status')).toHaveText('Score draft needs attention')
    await page.getByRole('button',{name:'Score & Parts',exact:true}).click()
    await page.getByRole('button',{name:'Reapply draft',exact:true}).click()
    await expect.poll(async()=>{
      const p=(await (await page.request.get(url)).json()).project
      return p.parts[0].staves?.[0]?.marks?.map((m:any)=>m.text)
    }).toEqual(['First phrase','Newest phrase'])
  }
})
test('Save revision includes the score debounce queue',async({page})=>{
  const url=await setup(page)
  await text(page,'Save this revision',350)
  await page.keyboard.press('Control+s')
  await expect.poll(async()=> (await (await page.request.get(`${url}/save`)).json()).revision).toBe(1)
  const p=(await (await page.request.get(url)).json()).project
  const saved=await (await page.request.get(`${url}/save`)).json()
  expect(saved.change_revision).toBe(p.revision)
  expect(p.parts[0].staves[0].marks[0].text).toBe('Save this revision')
})
test('ramp points can be selected, navigated and edited entirely by keyboard',async({page})=>{
  const url=await setup(page)
  const p=(await (await page.request.get(url)).json()).project
  p.parts[0].staves=[{id:'keyboard-staff',name:'Upper',clef:'treble',transpose:0,dynamics:{mode:'velocity',controller:11,events:[
    {id:'a',beat:0,duration:0,start:64,end:64,curve:'step'},
    {id:'b',beat:2,duration:0,start:96,end:96,curve:'step'},
  ]}}]
  await page.request.put(url,{headers:{'X-Pr0former':'1'},data:p})
  await page.getByRole('button',{name:/Dynamics & ramps/}).click()
  const first=page.locator('.ramp-node[data-node-beat="0"]')
  await first.focus()
  await first.press('ArrowRight')
  const second=page.locator('.ramp-node[data-node-beat="2"]')
  await expect(second).toBeFocused()
  await second.press('ArrowUp')
  await expect.poll(async()=> (await (await page.request.get(url)).json()).project.parts[0].staves[0].dynamics.events[1].start).toBe(97)
  await second.press('Home')
  await expect(first).toBeFocused()
})
