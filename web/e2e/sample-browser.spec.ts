import {test,expect} from '@playwright/test'
import {spawnSync} from 'node:child_process'
import {mkdtempSync,readFileSync,rmSync} from 'node:fs'
import {tmpdir} from 'node:os'
import {join} from 'node:path'
const headers={'X-Pr0former':'1'}
function tone(){const dir=mkdtempSync(join(tmpdir(),'pr0-browser-fixture-'));try{const file=join(dir,'tone.flac');const r=spawnSync('ffmpeg',['-hide_banner','-loglevel','error','-f','lavfi','-i','sine=frequency=330:sample_rate=44100','-t','0.2','-ac','1','-f','flac',file]);expect(r.status,String(r.stderr)).toBe(0);return readFileSync(file)}finally{rmSync(dir,{recursive:true,force:true})}}
test('sample browser: full-width search, top tags, tag: queries ranked by hits, one-line rows and a pinned pager',async({page})=>{
 test.setTimeout(180000)
 const status=await(await page.request.get('/api/status')).json()
 await page.request.post(`/api/${status.bootstrap?'register':'login'}`,{headers,data:{username:'browser-test',password:'test1234'}})
 const p=await(await page.request.post('/api/projects',{headers,data:{name:'Sample browser',mode:'freeform'}})).json()
 const buffer=tone()
 // 01–04 acoustic only, 05–08 both, 09–12 electric only; every sample also carries a unique tag.
 for(let i=1;i<=12;i++){
  const name=`Sample ${String(i).padStart(2,'0')}`
  const upload=await page.request.post(`/api/projects/${p.id}/samples`,{headers,multipart:{sample:{name:`${name}.flac`,mimeType:'application/octet-stream',buffer}}});expect(upload.ok(),await upload.text()).toBe(true)
  const sample=await upload.json()
  const tags=[...(i<=8?['acoustic']:[]),...(i>=5?['electric']:[]),`unique${i}`].join(', ')
  const saved=await page.request.put(`/api/projects/${p.id}/samples/${sample.id}`,{headers,data:{...sample,name,tags,global:true}});expect(saved.ok(),await saved.text()).toBe(true)
 }
 await page.goto('/')
 await page.getByRole('button',{name:'Samples',exact:true}).click()
 await page.getByRole('button',{name:'Browse all',exact:true}).click()
 const dialog=page.getByRole('dialog',{name:'Full sample browser',exact:true})
 const search=dialog.getByLabel('Search all samples'),rows=dialog.locator('.sample-browser-row'),chips=dialog.locator('.tag-filter .tag-chip')
 await expect(dialog).toBeVisible();await expect(rows.first()).toBeVisible()
 await page.screenshot({path:'test-results/sample-browser.png'})
 // Other specs may have added global samples to this server, so totals come from the API.
 const total=(await(await page.request.get(`/api/projects/${p.id}/sample-library`)).json()).length
 expect(total).toBeGreaterThanOrEqual(18)
 await expect(rows).toHaveCount(total)
 await expect(dialog.getByText('Browse samples')).toHaveCount(0)
 const dialogBox=(await dialog.boundingBox())!,searchBox=(await search.boundingBox())!,chipsBox=(await dialog.locator('.tag-filter').boundingBox())!
 expect(searchBox.width,'search spans the dialog').toBeGreaterThan(dialogBox.width*0.85)
 expect(chipsBox.y,'tags sit below the search bar').toBeGreaterThanOrEqual(searchBox.y+searchBox.height-1)
 await expect(chips).toHaveCount(8)
 await expect(chips.nth(0)).toHaveText(/^acoustic/i);await expect(chips.nth(1)).toHaveText(/^electric/i)
 // One line per sample on a desktop viewport: name, details and tags share a row.
 const first=rows.first(),nameBox=(await first.locator('.sample-name').boundingBox())!,detailBox=(await first.locator('.sb-details').boundingBox())!,tagBox=(await first.locator('.sb-tags').boundingBox())!
 expect(Math.abs(nameBox.y+nameBox.height/2-(detailBox.y+detailBox.height/2))).toBeLessThan(6)
 expect(Math.abs(nameBox.y+nameBox.height/2-(tagBox.y+tagBox.height/2))).toBeLessThan(8)
 expect(nameBox.height).toBeLessThan(40)
 // tag: queries are case-insensitive; a comma list matches either tag and ranks samples with both first.
 await search.fill('tag:ACOUSTIC')
 await expect(rows).toHaveCount(8)
 await expect(rows.first().locator('.sample-name')).toHaveText('Sample 01')
 await search.fill('tag:acoustic,electric')
 await expect(rows).toHaveCount(12)
 await expect(rows.nth(0).locator('.sample-name')).toHaveText('Sample 05')
 await expect(rows.nth(3).locator('.sample-name')).toHaveText('Sample 08')
 await expect(rows.nth(4).locator('.sample-name')).toHaveText('Sample 01')
 await search.fill('09 tag:electric')
 await expect(rows).toHaveCount(1)
 await expect(rows.first().locator('.sample-name')).toHaveText('Sample 09')
 // Cloud chips edit the tag: query instead of a hidden filter.
 await search.fill('')
 await chips.nth(1).click()
 await expect(search).toHaveValue('tag:electric')
 await expect(rows).toHaveCount(8)
 await chips.nth(0).click()
 await expect(search).toHaveValue('tag:electric,acoustic')
 await expect(rows).toHaveCount(12)
 await search.fill('')
 // Pagination with the pager pinned to the bottom and the column header pinned to the top.
 await dialog.getByLabel('Samples per page').selectOption('10')
 await expect(rows).toHaveCount(10)
 await expect(dialog.locator('.sample-browser-pager')).toContainText(`1–10 of ${total}`)
 await expect(dialog.getByRole('button',{name:'Previous page',exact:true})).toBeDisabled()
 const list=dialog.locator('.sample-browser-list'),head=dialog.locator('.sample-browser-head'),pager=dialog.locator('.sample-browser-pager')
 await list.evaluate(el=>{el.scrollTop=el.scrollHeight})
 const listBox=(await list.boundingBox())!,headBox=(await head.boundingBox())!,pagerBox=(await pager.boundingBox())!
 expect(Math.abs(headBox.y-listBox.y)).toBeLessThan(2)
 expect(pagerBox.y+pagerBox.height).toBeLessThanOrEqual(dialogBox.y+dialogBox.height+1)
 await page.screenshot({path:'test-results/sample-browser-paged.png'})
 await dialog.getByRole('button',{name:'Next page',exact:true}).click()
 await expect(rows).toHaveCount(total-10)
 await expect(pager).toContainText(`11–${total} of ${total}`)
 await expect(dialog.getByRole('button',{name:'Next page',exact:true})).toBeDisabled()
 await dialog.getByRole('button',{name:'Previous page',exact:true}).click()
 await expect(rows).toHaveCount(10)
 // Narrow viewports fall back to the stacked three-line row.
 await page.setViewportSize({width:390,height:844})
 const narrowName=(await rows.first().locator('.sample-name').boundingBox())!,narrowDetails=(await rows.first().locator('.sb-details').boundingBox())!
 expect(narrowDetails.y).toBeGreaterThanOrEqual(narrowName.y+narrowName.height-1)
 await page.screenshot({path:'test-results/sample-browser-phone.png'})
 await page.getByRole('button',{name:'Close sample browser',exact:true}).click()
})
