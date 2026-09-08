import {test,expect} from '@playwright/test'
test.use({hasTouch:true,isMobile:true,viewport:{width:390,height:844}})
test('phone layout leaves room for the graph and fits controls in both orientations',async({page})=>{
  const headers={'X-Pr0former':'1'}
  const status=await(await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap?'register':'login'}`,{headers,data:{username:'browser-test',password:'test1234'}})
  const p=await(await page.request.post('/api/projects',{headers,data:{name:'Phone workspace',mode:'freeform'}})).json()
  p.parts=[];p.graph={nodes:[{id:'tone',kind:'oscillator',label:'Phone tone',x:0,y:0,channels:2,parameters:{frequency:440,amplitude:.2}}],edges:[]}
  expect((await page.request.put(`/api/projects/${p.id}`,{headers,data:p})).ok()).toBe(true)
  await page.goto('/')
  await expect(page.getByRole('button',{name:'Edit Phone tone',exact:true})).toBeVisible()
  await expect(page.locator('.topbar')).toHaveCSS('height','32px')
  await expect(page.locator('.node-library')).toHaveCSS('width','112px')
  const canvas=(await page.locator('.graph-canvas').boundingBox())!
  expect(canvas.width).toBeGreaterThan(250);expect(canvas.height).toBeGreaterThan(550)
  await expect.poll(()=>page.locator('.vue-flow__transformationpane').evaluate(el=>new DOMMatrix(getComputedStyle(el).transform).a)).toBeLessThanOrEqual(.5)
  await page.screenshot({path:'test-results/phone-portrait.png'})
  await page.getByRole('button',{name:'Edit Phone tone',exact:true}).click()
  const dialog=page.getByRole('dialog')
  await expect(dialog).toBeVisible()
  const box=(await dialog.boundingBox())!
  expect(box.x).toBeGreaterThanOrEqual(0);expect(box.width).toBeLessThanOrEqual(390)
  await page.getByRole('button',{name:'Close parameters'}).click()
  await page.setViewportSize({width:844,height:390})
  await expect(page.locator('.topbar')).toHaveCSS('height','32px')
  expect((await page.locator('.graph-canvas').boundingBox())!.height).toBeGreaterThan(200)
  expect(await page.evaluate(()=>document.documentElement.scrollWidth)).toBe(844)
  await expect(page.getByRole('button',{name:'Edit Phone tone',exact:true})).toBeInViewport()
  await expect(page.locator('.node-library-content .library-node').first()).toBeInViewport()
  await page.screenshot({path:'test-results/phone-landscape.png'})
  await page.setViewportSize({width:1024,height:768})
  await expect(page.locator('.topbar')).toHaveCSS('height','64px')
})
