<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import type { Visualization } from '../types'
const props=defineProps<{data?:Visualization;kind:string;sampleRate:number;blockSize:number;stale:boolean;compact?:boolean}>()
const canvases=ref<HTMLCanvasElement[]>([])
const historyCanvases:HTMLCanvasElement[]=[]
const spectral=computed(()=>props.kind==='spectral_visualizer')
const historyMs=computed(()=>((props.data?.columns||0)*props.blockSize/props.sampleRate*1000).toFixed(1))
function setCanvas(element:unknown,index:number){if(element instanceof HTMLCanvasElement)canvases.value[index]=element}
function draw(){
  const value=props.data
  if(!value?.channels)return
  value.channels.forEach((channel,index)=>{
    const canvas=canvases.value[index],ctx=canvas?.getContext('2d');if(!canvas||!ctx)return
    const width=props.compact?280:640,height=spectral.value?(props.compact?156:260):(props.compact?94:180)
    canvas.width=width;canvas.height=height;ctx.fillStyle='#111b20';ctx.fillRect(0,0,width,height)
    const waterfallHeight=spectral.value?height*0.42:height-18,columns=value.columns||0,history=value.history?.[index]||''
    if(columns){const tile=historyCanvases[index]||(historyCanvases[index]=document.createElement('canvas'));tile.width=columns;tile.height=32;const tileCtx=tile.getContext('2d')!;const image=tileCtx.createImageData(columns,32);
      for(let col=0;col<columns;col++)for(let band=0;band<32;band++){const intensity=parseInt(history.slice((col*32+band)*2,(col*32+band)*2+2),16)||0,t=intensity/255,offset=((31-band)*columns+col)*4;image.data[offset]=Math.round(15+210*t*t);image.data[offset+1]=Math.round(25+195*t);image.data[offset+2]=Math.round(37+150*t);image.data[offset+3]=255;}
      tileCtx.putImageData(image,0,0);ctx.imageSmoothingEnabled=false;ctx.drawImage(tile,0,0,width,waterfallHeight);
    }
    ctx.fillStyle='#9bb4bc';ctx.font=`${props.compact?11:12}px sans-serif`;ctx.fillText(`${(props.sampleRate/2000).toFixed(1)} kHz`,5,13)
    ctx.fillText(`0 Hz  ·  ${historyMs.value} ms history`,5,waterfallHeight+13)
    if(spectral.value){
      const top=waterfallHeight+26,plotHeight=(height-top-10)/2
      const plot=(values:number[],offset:number,color:string,phase:boolean)=>{ctx.strokeStyle='#35464d';ctx.beginPath();ctx.moveTo(0,offset+plotHeight);ctx.lineTo(width,offset+plotHeight);ctx.stroke();ctx.strokeStyle=color;ctx.beginPath();values.forEach((v,i)=>{const y=phase?(Math.max(-Math.PI,Math.min(Math.PI,v))+Math.PI)/(2*Math.PI):Math.max(0,Math.min(1,(20*Math.log10(Math.max(v,1e-6))+100)/180));const x=i*width/Math.max(1,values.length-1),yy=offset+(1-y)*plotHeight;if(i===0)ctx.moveTo(x,yy);else ctx.lineTo(x,yy)});ctx.stroke()}
      plot(channel.magnitude,top,'#edbf73',false);plot(channel.phase,top+plotHeight+7,'#bda3e5',true)
    }
  })
}
watch(()=>[props.data,props.compact],draw,{flush:'post'})
onMounted(draw)
</script>
<template>
<div class="data-visualizer" :class="{compact,stale}">
  <template v-if="kind==='control_visualizer'"><output class="control-visualizer-value" :title="data ? String(data.value) : undefined">{{data ? data.value : '—'}}</output><small>{{data ? typeof data.value === 'string' ? 'STRING' : 'NUMBER' : 'Waiting for engine'}}{{stale && data ? ' · STALE' : ''}}</small></template>
  <template v-else><p v-if="!data?.channels" class="visualizer-placeholder">Enable the audio engine to visualize this signal.</p><div v-else class="visualizer-channels"><section v-for="(_,index) in data.channels" :key="index"><header>CH {{index+1}} <span v-if="stale">STALE</span><span v-else-if="!data.ready">FILLING FFT</span></header><canvas :ref="el=>setCanvas(el,index)" role="img" :aria-label="`Channel ${index+1} ${spectral?'spectrogram, FFT bins and phase':'spectrogram'}`"/><div v-if="spectral" class="spectrum-key"><span>Magnitude (dB) · bins 0–{{data.size! / 2}}</span><span>Phase · −π…π rad</span></div></section></div><small>20 Hz display<span v-if="spectral && data"> · {{data.polar?'Polar':'Cartesian'}} frames</span></small></template>
</div>
</template>
