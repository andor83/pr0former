import {test,expect} from '@playwright/test'
const headers={'X-Pr0former':'1'}
test('graphical control step snaps sliders, arrows and steppers, and ` toggles the engine',async({page})=>{
 test.setTimeout(120000)
 const status=await(await page.request.get('/api/status')).json()
 await page.request.post(`/api/${status.bootstrap?'register':'login'}`,{headers,data:{username:'browser-test',password:'test1234'}})
 const p=await(await page.request.post('/api/projects',{headers,data:{name:'Control step',mode:'freeform'}})).json()
 const node=(id:string,x:number,parameters:Record<string,number>)=>({id,kind:'control_input',label:id,x,y:0,channels:1,parameters,control_value:0})
 p.parts=[];p.graph={nodes:[node('Fader',0,{mode:3,min:0,max:10,step:.5}),node('Count',400,{mode:1,min:0,max:100,step:5}),node('Gain',800,{mode:2,min:0,max:1,step:.25})],edges:[]}
 const saved=await page.request.put(`/api/projects/${p.id}`,{headers,data:p});expect(saved.ok(),await saved.text()).toBe(true)
 const load=async()=>(await(await page.request.get(`/api/projects/${p.id}`)).json()).project
 const value=async(id:string)=>(await load()).graph.nodes.find((n:any)=>n.id===id)
 await page.goto('/')
 await expect(page.getByLabel('Fader value',{exact:true})).toHaveAttribute('step','0.5')
 // Click the fader as a player would, then nudge with the keyboard: one notch per press.
 await page.getByLabel('Fader slider',{exact:true}).click();await page.keyboard.press('ArrowRight')
 await expect.poll(async()=>(await value('Fader')).control_value).toBe(.5)
 await page.getByRole('button',{name:'Increase Count by 5',exact:true}).click()
 await expect.poll(async()=>(await value('Count')).control_value).toBe(5)
 const gain=page.getByLabel('Gain value',{exact:true});await gain.focus();await gain.press('ArrowUp')
 await expect.poll(async()=>(await value('Gain')).control_value).toBe(.25)
 await gain.press('ArrowUp');await expect.poll(async()=>(await value('Gain')).control_value).toBe(.5)
 // The step lives in Options as a plain number field and is hidden for Bang and Text controls.
 await page.getByRole('button',{name:'Edit Fader',exact:true}).click()
 const modal=page.locator('dialog.node-parameter-modal'),stepField=modal.locator('#param-step')
 await expect(stepField).toHaveValue('0.50')
 await stepField.fill('2');await stepField.press('Tab')
 await expect.poll(async()=>(await value('Fader')).parameters.step).toBe(2)
 await page.screenshot({path:'test-results/control-step-options.png'})
 await page.getByRole('button',{name:'Close parameters'}).click()
 await page.getByLabel('Fader slider',{exact:true}).click();await page.keyboard.press('ArrowRight')
 await expect.poll(async()=>(await value('Fader')).control_value).toBe(2)
 await page.locator('.graph-canvas').click({position:{x:20,y:400}})
 await page.keyboard.press('Backquote')
 await expect(page.getByRole('button',{name:'Disable audio engine',exact:true})).toBeVisible()
 await page.keyboard.press('Backquote')
 await expect(page.getByRole('button',{name:'Enable audio engine',exact:true})).toBeVisible()
 // Typing in a field must not toggle the engine.
 await gain.focus();await page.keyboard.press('Backquote')
 await expect(page.getByRole('button',{name:'Enable audio engine',exact:true})).toBeVisible()
})
