import { test, expect } from '@playwright/test'

test('live edits coalesce into saved revisions, keyboard saves flush pending changes, and server autosaves', async ({page}) => {
  test.setTimeout(100000)
  const headers = {'X-Pr0former':'1'}
  const status = await (await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap?'register':'login'}`, {headers,data:{username:'browser-test',password:'test1234'}})
  let p = await (await page.request.post('/api/projects',{headers,data:{name:'Revision saving',mode:'freeform'}})).json()
  const path = `/api/projects/${p.id}`
  const saved = async () => (await page.request.get(`${path}/save`)).json()
  const load = async () => (await (await page.request.get(path)).json()).project
  for (let i=0;i<30;i++) {
    const response = await page.request.put(`${path}/parameter`, {headers,data:{node:'out',parameter:'gain',value:-i,revision:p.revision}})
    expect(response.ok()).toBeTruthy();p=await response.json()
  }
  expect(await saved()).toMatchObject({revision:0,change_revision:0,dirty:true})
  // Optimistic concurrency still rejects stale edits despite the stable saved revision.
  expect((await page.request.put(`${path}/parameter`,{headers,data:{node:'out',parameter:'gain',value:0,revision:0}})).status()).toBe(409)
  await page.goto('/')
  await page.getByRole('button',{name:'Choose project'}).click()
  await page.getByRole('button',{name:'Revision saving',exact:true}).click()
  await expect(page.locator('.revision')).toHaveText('Revision 0 · Unsaved changes')
  await page.getByRole('button',{name:'Edit Audio output',exact:true}).click()
  const gain = page.getByRole('dialog').getByRole('spinbutton',{name:'Output gain',exact:true})
  // Shortcut while a field is focused commits its change and flushes the 60 ms live-edit queue.
  await gain.fill('-8')
  await gain.press('Control+s')
  await expect.poll(async () => (await saved()).revision).toBe(1)
  expect((await load()).graph.nodes.find((n:any)=>n.id==='out').parameters.gain).toBe(-8)
  await expect(page.locator('.revision')).toHaveText('Revision 1 · Saved')
  await gain.fill('-6')
  await gain.press('Meta+s')
  await expect.poll(async () => (await saved()).revision).toBe(2)
  expect((await load()).graph.nodes.find((n:any)=>n.id==='out').parameters.gain).toBe(-6)
  await page.keyboard.press('Meta+s')
  await expect(page.getByRole('button',{name:'Save revision',exact:true})).toBeEnabled()
  expect((await saved()).revision).toBe(2)
  await page.getByRole('button',{name:'Close parameters'}).click()
  p=await load()
  p=await (await page.request.put(`${path}/parameter`,{headers,data:{node:'out',parameter:'gain',value:-12,revision:p.revision}})).json()
  await expect(page.locator('.revision')).toHaveText('Revision 2 · Unsaved changes')
  // The timer belongs to the server: autosave survives a browser being closed.
  await page.goto('about:blank')
  await expect.poll(async () => (await saved()).revision,{timeout:65000,intervals:[1000]}).toBe(3)
  expect(await saved()).toMatchObject({revision:3,change_revision:p.revision,dirty:false})
  await page.goto('/')
  await expect(page.locator('.revision')).toHaveText('Revision 3 · Saved')
})
