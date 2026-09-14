import {test,expect} from '@playwright/test'
test('six drum trigger inputs accept negative values and release at zero',async({page})=>{
 const headers={'X-Pr0former':'1'},status=await(await page.request.get('/api/status')).json()
 await page.request.post(`/api/${status.bootstrap?'register':'login'}`,{headers,data:{username:'browser-test',password:'test1234'}})
 let p=await(await page.request.post('/api/projects',{headers,data:{name:'Drum trigger inputs',mode:'freeform'}})).json()
 p.parts=[]
 p.graph={nodes:[{id:'pads',kind:'drum_pads',label:'Pads',x:300,y:0,channels:1,parameters:{}},...Array.from({length:6},(_,i)=>({id:`v${i+1}`,kind:'value',label:`Value ${i+1}`,x:0,y:i*180,channels:1,parameters:{value:0}}))],edges:Array.from({length:6},(_,i)=>({id:`e${i}`,source:`v${i+1}`,source_port:'out',target:'pads',target_port:`trigger_${i+1}`}))}
 const saved=await page.request.put(`/api/projects/${p.id}`,{headers,data:p});expect(saved.ok(),await saved.text()).toBe(true);p=await saved.json()
 await page.goto('/')
 await page.getByRole('button',{name:'Enable audio engine',exact:true}).click()
 try{
  const pads=page.locator('.vue-flow__node').filter({has:page.getByRole('button',{name:'Edit Pads',exact:true})})
  for(let i=1;i<=6;i++)await expect(pads.locator(`[data-handleid="trigger_${i}"]`)).toHaveCount(1)
  for(const value of [-0.25,0]){
   for(let i=1;i<=6;i++){
    const response=await page.request.put(`/api/projects/${p.id}/parameter`,{headers,data:{node:`v${i}`,parameter:'value',value,revision:p.revision}})
    expect(response.ok()).toBe(true);p=await response.json()
   }
   await expect(pads.locator('.drum-pad[aria-pressed="true"]')).toHaveCount(value===0?0:6)
  }
 }finally{await page.request.post(`/api/projects/${p.id}/engine`,{headers,data:{enabled:false}})}
})
