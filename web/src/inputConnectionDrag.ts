import {computed,onBeforeUnmount,ref,watch,type Ref} from 'vue'
import type {GraphEdge} from './types'

// Input handles move their incoming bundle. Output handles remain owned by Vue Flow.
export function useInputConnectionDrag(canvas:Ref<HTMLElement|undefined>,options:{
  editable:()=>boolean;context:()=>string;edges:()=>GraphEdge[];
  tap:(node:string,port:string)=>void;
  drop:(ids:string[],target:{node:string;port:string}|null)=>void;
}){
  const drag=ref<{pointer:number;context:string;node:string;port:string;startX:number;startY:number;x:number;y:number;moved:boolean;ids:string[];origins:{x:number;y:number;color:string}[]}|null>(null)
  let suppressUntil=0,dispose=()=>{}
  function cancel(){drag.value=null}
  const hidden=computed(()=>new Set(drag.value?.moved?drag.value.ids:[]))
  const preview=computed(()=>drag.value?.moved?drag.value:null)
  watch(canvas,element=>{
    dispose();if(!element)return
    const input=(target:EventTarget|null)=>target instanceof Element?target.closest<HTMLElement>('.vue-flow__handle.target'):null
    const down=(event:PointerEvent)=>{
      if(!drag.value)suppressUntil=0
      const handle=input(event.target),node=handle?.closest('.vue-flow__node')?.getAttribute('data-id'),port=handle?.getAttribute('data-handleid')
      if(event.button!==0||!handle||!node||!port||!options.editable()||drag.value)return
      event.preventDefault();event.stopPropagation()
      const edges=options.edges().filter(e=>e.target===node&&e.target_port===port)
      const origins=edges.flatMap(edge=>{
        const source=Array.from(element.querySelectorAll<HTMLElement>('.vue-flow__handle.source')).find(h=>h.getAttribute('data-nodeid')===edge.source&&h.getAttribute('data-handleid')===edge.source_port)
        if(!source)return []
        const box=source.getBoundingClientRect()
        return [{x:box.x+box.width/2,y:box.y+box.height/2,color:source.classList.contains('audio')?'var(--cyan)':source.classList.contains('spectral')?'var(--violet)':'var(--amber)'}]
      })
      drag.value={pointer:event.pointerId,context:options.context(),node,port,startX:event.clientX,startY:event.clientY,x:event.clientX,y:event.clientY,moved:false,ids:edges.map(e=>e.id),origins}
      element.setPointerCapture(event.pointerId)
    }
    const move=(event:PointerEvent)=>{
      const d=drag.value;if(!d||event.pointerId!==d.pointer)return
      event.preventDefault();event.stopPropagation()
      d.x=event.clientX;d.y=event.clientY;d.moved ||= Math.hypot(d.x-d.startX,d.y-d.startY)>4
    }
    const up=(event:PointerEvent)=>{
      const d=drag.value;if(!d||event.pointerId!==d.pointer)return
      event.preventDefault();event.stopPropagation();drag.value=null;suppressUntil=Date.now()+400
      if(element.hasPointerCapture(event.pointerId))element.releasePointerCapture(event.pointerId)
      if(event.type==='pointercancel'||d.context!==options.context()||!options.editable())return
      if(!d.moved){options.tap(d.node,d.port);return}
      const handle=input(document.elementFromPoint(event.clientX,event.clientY))
      const node=handle?.closest('.vue-flow__node')?.getAttribute('data-id'),port=handle?.getAttribute('data-handleid')
      if(d.ids.length)options.drop(d.ids,node&&port&&handle&&element.contains(handle)?{node,port}:null)
    }
    const compatibility=(event:Event)=>{if(drag.value||(event.type==='click'&&Date.now()<suppressUntil&&(event.target===element||(event.target instanceof Element&&!!event.target.closest('.vue-flow__handle'))))){event.preventDefault();event.stopPropagation()}}
    const escape=(event:KeyboardEvent)=>{if(event.key==='Escape'&&drag.value){event.preventDefault();cancel();suppressUntil=Date.now()+400}}
    element.addEventListener('pointerdown',down,true)
    element.addEventListener('mousedown',compatibility,true)
    element.addEventListener('touchstart',compatibility,{capture:true,passive:false})
    element.addEventListener('click',compatibility,true)
    window.addEventListener('pointermove',move,{capture:true,passive:false})
    window.addEventListener('pointerup',up,true)
    window.addEventListener('pointercancel',up,true)
    window.addEventListener('keydown',escape,true)
    dispose=()=>{
      cancel();element.removeEventListener('pointerdown',down,true);element.removeEventListener('mousedown',compatibility,true);element.removeEventListener('touchstart',compatibility,true);element.removeEventListener('click',compatibility,true)
      window.removeEventListener('pointermove',move,true);window.removeEventListener('pointerup',up,true);window.removeEventListener('pointercancel',up,true);window.removeEventListener('keydown',escape,true)
    }
  },{flush:'post'})
  onBeforeUnmount(()=>dispose())
  return {hidden,preview}
}
