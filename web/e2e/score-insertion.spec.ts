import {test,expect} from '@playwright/test'
test('staff clicks append and insert notes without gaps, and overflow changes nothing',async({page})=>{
  const headers={'X-Pr0former':'1'},status=await(await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap?'register':'login'}`,{headers,data:{username:'browser-test',password:'test1234'}})
  let p=await(await page.request.post('/api/projects',{headers,data:{name:'Insertion regression',mode:'structured'}})).json()
  p.parts[0].notes=[]
  p=await(await page.request.put(`/api/projects/${p.id}`,{headers,data:p})).json()
  const read=async()=>(await(await page.request.get(`/api/projects/${p.id}`)).json()).project
  await page.goto('/');await page.getByRole('button',{name:'Score & Parts',exact:true}).click()
  await page.getByLabel('Note values',{exact:true}).click();await page.getByRole('button',{name:'Quarter note (5)',exact:true}).click()
  const staff=page.locator('[data-score-part="part-1"] [data-staff-id]').first()
  await staff.click({position:{x:300,y:118}})
  await expect.poll(async()=>(await read()).parts[0].notes.map((n:any)=>n.beat)).toEqual([0])
  const first=(await read()).parts[0].notes[0].id
  const glyph=staff.locator(`[data-note-id="${first}"]`).first()
  const box=(await glyph.boundingBox())!,row=(await staff.boundingBox())!
  // Empty staff just right of the head is temporally inside the preceding note.
  await page.mouse.click(box.x+box.width+5,row.y+145)
  await expect.poll(async()=>(await read()).parts[0].notes.map((n:any)=>n.beat)).toEqual([0,1])
  const originalSecond=(await read()).parts[0].notes[1].id
  const a=(await glyph.boundingBox())!,b=(await staff.locator(`[data-note-id="${originalSecond}"]`).first().boundingBox())!
  await page.mouse.click((a.x+b.x)/2,row.y+140)
  await expect.poll(async()=>(await read()).parts[0].notes.map((n:any)=>n.beat)).toEqual([0,1,2])
  expect((await read()).parts[0].notes.find((n:any)=>n.id===originalSecond).beat).toBe(2)
  await page.mouse.click(a.x-12,row.y+140)
  await expect.poll(async()=>(await read()).parts[0].notes.map((n:any)=>n.beat)).toEqual([0,1,2,3])
  expect((await read()).parts[0].notes.find((n:any)=>n.id===first).beat).toBe(1)
  const before=await read()
  await page.mouse.click(a.x-12,row.y+140)
  await expect(page.getByText('Cannot insert note: this would overflow the bar.',{exact:true})).toBeVisible()
  expect((await read()).parts[0].notes).toEqual(before.parts[0].notes)
  const last=before.parts[0].notes.at(-1).id,lastBox=(await staff.locator(`[data-note-id="${last}"]`).first().boundingBox())!
  await page.mouse.click(lastBox.x+lastBox.width+6,row.y+145)
  await expect(page.getByText('Cannot insert note: this would overflow the bar.',{exact:true})).toBeVisible()
  expect((await read()).parts[0].notes).toEqual(before.parts[0].notes)
})
