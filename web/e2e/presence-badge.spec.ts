import {test,expect} from '@playwright/test'
const headers={'X-Pr0former':'1'}
test('the header counts connected users and hides the badge when alone',async({page,browser})=>{
 test.setTimeout(120000)
 const status=await(await page.request.get('/api/status')).json()
 await page.request.post(`/api/${status.bootstrap?'register':'login'}`,{headers,data:{username:'browser-test',password:'test1234'}})
 const p=await(await page.request.post('/api/projects',{headers,data:{name:'Presence badge',mode:'freeform'}})).json()
 await page.goto('/')
 const indicator=page.locator('.server-indicator'),badge=indicator.locator('.connection-count')
 await expect(indicator).toHaveText('Server connected')
 await expect(badge).toHaveCount(0)
 // A second tab of the same user is not another connected user.
 const secondTab=await page.context().newPage();await secondTab.goto('/')
 await expect(secondTab.locator('.server-indicator')).toHaveText('Server connected')
 await expect(badge).toHaveCount(0)
 await secondTab.close()
 const guestName=`player-${Date.now()}`
 const created=await page.request.post('/api/admin/users',{headers,data:{username:guestName,password:'player123',is_admin:false,enabled:true,fields:{}}});expect(created.ok(),await created.text()).toBeTruthy()
 const guest=await browser.newContext({baseURL:'http://127.0.0.1:3101'})
 try{
  await guest.request.post('/api/login',{headers,data:{username:guestName,password:'player123'}})
  const me=await(await guest.request.get('/api/me')).json()
  const joined=await page.request.post(`/api/projects/${p.id}/members`,{headers,data:{user_id:me.id,role:'performer'}});expect(joined.ok(),await joined.text()).toBeTruthy()
  const remote=await guest.newPage();await remote.goto('/')
  await expect(remote.locator('.server-indicator')).toContainText('Server connected')
  await expect(badge).toHaveText('2')
  await expect(badge).toHaveAttribute('title','Connected users: 2')
  await expect(remote.locator('.connection-count')).toHaveText('2')
  await page.screenshot({path:'test-results/presence-badge.png'})
  await remote.close()
  await expect(badge).toHaveCount(0)
 }finally{await guest.close()}
})
