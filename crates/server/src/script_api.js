// Bundled locally. This closure's host interface is retained privately by Rust.
(function () {
  "use strict";
  const stringify = JSON.stringify, parse = JSON.parse;
  const inputs = [], outputs = [], bindings = [], pending = [], logs = [];
  let sealed = false, started = false, now = {sample: 0, ms:0, beat: 0, bpm: 120, sample_rate: 48000, generation: 0, running: false};
  let seed = 1, nextTimer = 1, previousGeneration, previousRunning, previousBpm, lastTick;
  const timers = new Map();
  const finite = v => { if (typeof v !== "number" || !Number.isFinite(v)) throw new TypeError("Expected a finite number"); return v; };
  const integer = (v, min, max) => { finite(v); if (!Number.isInteger(v) || v < min || v > max) throw new RangeError(`Expected an integer from ${min} to ${max}`); return v; };
  const name = v => { if (typeof v !== "string" || !/^[A-Za-z_][A-Za-z0-9_-]{0,47}$/.test(v) || v === "midi") throw new TypeError("Port names must be identifiers of 1–48 bytes, other than midi"); return v; };
  const handler = fn => { if (typeof fn !== "function") throw new TypeError("Expected a handler function"); return fn; };
  let subscriptions = 0;
  function emitter(allowed) {
    const listeners = new Map();
    const api = {
      on(event, fn) {
        if (!allowed.includes(event)) throw new Error(`Unknown event '${event}'; use ${allowed.join(", ")}`);
        handler(fn); if (subscriptions >= 256) throw new Error("At most 256 event subscriptions");
        const list = listeners.get(event) || []; if (!list.includes(fn)) { list.push(fn); subscriptions++; } listeners.set(event, list); return api;
      },
      off(event, fn) { const list = listeners.get(event) || [], i = list.indexOf(fn); if (i >= 0) { list.splice(i, 1); subscriptions--; } return api; },
      once(event, fn) { handler(fn); const once = e => { api.off(event, once); return fn(e); }; return api.on(event, once); }
    };
    return {api, fire(event, details) { for (const fn of [...(listeners.get(event) || [])]) {
      const result = fn(Object.freeze({...details}));
      if (result && typeof result.then === "function") throw new Error("Handlers must be synchronous; use after_beats or after_ms");
    } }};
  }
  function command(action, timing = {}) {
    if (pending.length >= 256) throw new Error("Script command budget exceeded (256 per event)");
    if (timing.sample !== undefined && finite(timing.sample) < 0) throw new RangeError("Sample cannot be negative");
    if (timing.beat !== undefined) finite(timing.beat);
    pending.push({...action, sample: timing.sample ?? now.sample, beat: timing.beat ?? null, generation: now.generation, initial: !started, immediate: timing.sample === undefined});
  }
  function define_input(id, initial = 0, fn) {
    if (typeof initial === "function") { fn = initial; initial = 0; }
    if (sealed) throw new Error("Define ports only at script setup");
    name(id); finite(initial);
    if (inputs.length >= 8 || inputs.some(p => p.name === id)) throw new Error("Inputs must be unique; at most eight");
    const event = emitter(["change", "rise", "fall"]);
    let value = initial;
    const api = Object.freeze({...event.api, get value() { return value; }, get: () => value, read: () => value});
    const port = {name: id, initial, dispatch(e) { const previous = value; value = e.value; const details = {...e, name:id, previous}; event.fire("change", details); if (previous <= 0 && value > 0) event.fire("rise", details); if (previous > 0 && value <= 0) event.fire("fall", details); }};
    inputs.push(port); if (fn) event.api.on("change", fn); return api;
  }
  function define_output(id, initial = 0) {
    if (sealed) throw new Error("Define ports only at script setup");
    name(id); finite(initial);
    if (outputs.length >= 8 || outputs.some(p => p.name === id)) throw new Error("Outputs must be unique; at most eight");
    const port = outputs.length, event = emitter(["change"]); let value = initial;
    const set = (next, timing = {}) => { finite(next); command({type:"output", port, value:next}, timing); const previous=value; value=next; event.fire("change", {...now,name:id,value,previous}); return api; };
    const api = Object.freeze({...event.api, get value() {return value;}, get:()=>value, read:()=>value, set, write:set, emit:set,
      at_sample:(sample,value)=>set(value,{sample}), at_beat:(beat,value)=>set(value,{beat}),
      pulse(value=1, duration_ms=10) { finite(duration_ms); if(duration_ms<=0)throw new RangeError("Pulse duration must be positive"); const start=now.sample + Math.ceil(now.sample_rate * 0.01); set(value,{sample:start}); set(0,{sample:start+Math.max(1,Math.round(duration_ms*now.sample_rate/1000))}); return api; }
    });
    outputs.push({name:id,initial}); return api;
  }
  const engineEvent = emitter(["ready","tick","reset"]), transportEvent = emitter(["play","pause","tempo","reset"]), metroEvent = emitter(["click"]);
  const midiEvent = emitter(["message","note_on","note_off","cc","pitch_bend","program","channel_pressure","poly_pressure"]);
  function decode_midi(bytes) {
    if (!Array.isArray(bytes) || ![2,3].includes(bytes.length)) throw new Error("MIDI requires two or three channel-message bytes");
    const status=integer(bytes[0],128,239), type=status>>4, size=[12,13].includes(type)?2:3;
    if(bytes.length!==size)throw new Error("Wrong MIDI message length");
    const data1=integer(bytes[1],0,127), data2=size===3?integer(bytes[2],0,127):0;
    const event = type===9&&data2>0?"note_on":type===8||type===9?"note_off":({10:"poly_pressure",11:"cc",12:"program",13:"channel_pressure",14:"pitch_bend"})[type];
    return {type:event, bytes:[...bytes], status, channel:(status&15)+1, data1, data2,
      ...(["note_on","note_off","poly_pressure"].includes(event)?{note:data1,velocity:data2}:{}),
      ...(event==="cc"?{controller:data1,value:data2,normalized:data2/127}:{}),
      ...(event==="pitch_bend"?{value:(data2<<7)|data1,normalized:(((data2<<7)|data1)-8192)/8192}:{}),
      ...(event==="program"?{program:data1}:{}),
      ...(event.includes("pressure")?{pressure:type===13?data1:data2}:{})};
  }
  const status=(base,channel)=>base+integer(channel,1,16)-1;
  const midi = Object.freeze({...midiEvent.api, decode:decode_midi,
    send(bytes,timing={}) {const e=decode_midi(bytes); command({type:"midi",status:e.status,data1:e.data1,data2:e.data2},timing);},
    note_on(note,velocity=100,channel=1,timing={}) {midi.send([status(144,channel),integer(note,0,127),integer(velocity,0,127)],timing);},
    note_off(note,channel=1,timing={}) {midi.send([status(128,channel),integer(note,0,127),0],timing);},
    cc(controller,value,channel=1,timing={}) {midi.send([status(176,channel),integer(controller,0,127),integer(value,0,127)],timing);},
    pitch_bend(value,channel=1,timing={}) {integer(value,0,16383);midi.send([status(224,channel),value&127,value>>7],timing);},
    program(value,channel=1,timing={}) {midi.send([status(192,channel),integer(value,0,127)],timing);},
    all_notes_off(channel=1) {midi.cc(123,0,channel);},
    note(note,velocity=100,duration_beats=0.25,channel=1,beat=now.beat+0.05) {finite(duration_beats);if(duration_beats<=0)throw new Error("Note duration must be positive");midi.note_on(note,velocity,channel,{beat});midi.note_off(note,channel,{beat:beat+duration_beats});}
  });
  const oscEvent=emitter(["message"]), oscBindings=[];
  const osc=Object.freeze({...oscEvent.api, bind(address) {
    if(sealed)throw new Error("Bind OSC addresses at script setup");
    if(typeof address!=="string"||!address.startsWith("/")||address.length>256||oscBindings.length>=32)throw new Error("Use an OSC address up to 256 characters; at most 32 bindings");
    const e=emitter(["message"]);oscBindings.push({address,...e});return Object.freeze(e.api);
  }});
  function bind(kind,target) {
    if(sealed)throw new Error("Bind named routes at script setup");
    if(typeof target!=="string"||!target.length||target.length>256||bindings.length>=16)throw new Error("Use a named route up to 256 characters; at most sixteen bindings");
    const binding=bindings.length, e=emitter(["change"]);let value;
    const api=Object.freeze({...e.api,get value(){return value;},get:()=>value,read:()=>value,set(next,timing={}) {
      finite(next);
      command({type:"named",binding,value:next},timing);return api;
    },write(next,timing={}){return api.set(next,timing);},emit(next,timing={}){return api.set(next,timing);}});
    bindings.push({kind,name:target,dispatch(details){const previous=value;value=details.value;e.fire("change",{...details,name:target,previous});}});return api;
  }
  function timer(delay,fn,unit,repeat) {
    finite(delay);handler(fn);if(delay<=0||timers.size>=128)throw new Error("Timers need positive delays; at most 128 timers");
    const id=nextTimer++;timers.set(id,{due:now[unit]+delay,delay,initial:!started,fn,unit,repeat:repeat?delay:0});return id;
  }
  const cancel=id=>timers.delete(id);
  function runTimers() {for(const [id,t]of [...timers])if(timers.has(id)&&now[t.unit]>=t.due){if(t.repeat)t.due+=t.repeat*(1+Math.floor((now[t.unit]-t.due)/t.repeat));else timers.delete(id);const result=t.fn(Object.freeze({...now,timer:id}));if(result&&typeof result.then==="function")throw new Error("Timer handlers must be synchronous");}}
  const clamp=(v,min=0,max=1)=>Math.min(max,Math.max(min,finite(v)));
  const lerp=(a,b,t)=>finite(a)+(finite(b)-a)*finite(t);
  const wrap=(v,min=0,max=1)=>{finite(v);finite(min);finite(max);if(max<=min)throw new RangeError("wrap needs max > min");return ((v-min)%(max-min)+(max-min))%(max-min)+min;};
  const random=()=>{seed^=seed<<13;seed^=seed>>>17;seed^=seed<<5;return(seed>>>0)/4294967296;};
  function log(level,args){if(logs.length>=64)throw new Error("Console budget exceeded (64 messages per event)");logs.push({level,sample:now.sample,initial:!started,message:args.map(x=>typeof x==="string"?x:stringify(x)??String(x)).join(" ").slice(0,2048)});}
  const globals = {
    Atomics: undefined, SharedArrayBuffer: undefined,
    define_input, define_output, midi, osc,
    engine:Object.freeze({...engineEvent.api,now:()=>Object.freeze({...now})}),
    transport:Object.freeze({...transportEvent.api}), metronome:Object.freeze({...metroEvent.api}),
    on_tick:fn=>engineEvent.api.on("tick",fn),
    bind_send:name=>bind("send",name), bind_receive:name=>bind("receive",name),
    control:Object.freeze({send:name=>bind("publish",name),receive:name=>bind("send",name)}),
    after_beats:(beats,fn)=>timer(beats,fn,"beat",false), every_beats:(beats,fn)=>timer(beats,fn,"beat",true),
    after_ms:(ms,fn)=>timer(ms,fn,"ms",false), every_ms:(ms,fn)=>timer(ms,fn,"ms",true),cancel,
    clamp,lerp,wrap,map_range:(v,a,b,c,d)=>{if(a===b)throw new RangeError("Input range cannot be zero");return lerp(c,d,(finite(v)-a)/(b-a));},
    quantize:(v,step=1)=>{if(finite(step)<=0)throw new Error("Step must be positive");return Math.round(finite(v)/step)*step;},
    midi_to_hz:note=>440*2**((finite(note)-69)/12),hz_to_midi:hz=>{if(finite(hz)<=0)throw new Error("Frequency must be positive");return 69+12*Math.log2(hz/440);},
    db_to_gain:db=>10**(finite(db)/20),gain_to_db:gain=>20*Math.log10(Math.max(finite(gain),1e-12)),
    seed_random:value=>{seed=integer(value,0,4294967295)||1;},random,random_int:(min,max)=>{integer(min,-2147483648,2147483647);integer(max,min,2147483647);return min+Math.floor(random()*(max-min+1));},
    choose:values=>{if(!Array.isArray(values)||!values.length)throw new Error("choose needs a nonempty array");return values[Math.floor(random()*values.length)];},chance:probability=>random()<clamp(probability),
    euclidean:(step,hits,length)=>{integer(length,1,1024);integer(hits,0,length);integer(step,-2147483648,2147483647);return wrap(step,0,length)*hits%length<hits;},
    console:Object.freeze({log:(...args)=>log("info",args),info:(...args)=>log("info",args),warn:(...args)=>log("warn",args),error:(...args)=>log("error",args),debug:(...args)=>log("debug",args)}),
  };
  for(const [key,value]of Object.entries(globals))Object.defineProperty(globalThis,key,{value,writable:false,configurable:false});
  return {
    seal() {sealed=true;return stringify({inputs:inputs.map(({name,initial})=>({name,initial})),outputs:outputs.map(({name,initial})=>({name,initial})),bindings:bindings.map(({kind,name})=>({kind,name}))});},
    dispatch(json) {
      const e=parse(json);now={sample:e.sample,ms:e.sample*1000/e.sample_rate,beat:e.beat,bpm:e.bpm,sample_rate:e.sample_rate,generation:e.generation,running:e.running??now.running};
      if(!started){started=true;for(const t of timers.values())if(t.initial){t.due=now[t.unit]+t.delay;t.initial=false;}engineEvent.fire("ready",now);}
      if(e.type==="clock") {
        if(previousGeneration!==undefined&&previousGeneration!==e.generation){timers.clear();engineEvent.fire("reset",now);transportEvent.fire("reset",now);}
        if(previousRunning!==e.running)transportEvent.fire(e.running?"play":"pause",now);
        if(previousBpm!==e.bpm)transportEvent.fire("tempo",{...now,previous:previousBpm});
        previousGeneration=e.generation;previousRunning=e.running;previousBpm=e.bpm;
        if(e.tick){const frames=lastTick===undefined?0:e.sample-lastTick;lastTick=e.sample;engineEvent.fire("tick",{...now,frames,dt:frames/e.sample_rate});runTimers();}
        if(e.click)metroEvent.fire("click",{...now,beat_unit:e.unit,position:e.position});
      } else if(e.type==="input")inputs[e.port]?.dispatch(e);
      else if(e.type==="midi"){const details={...e,...decode_midi(e.bytes)};midiEvent.fire("message",details);midiEvent.fire(details.type,details);}
      else if(e.type==="named")bindings[e.binding]?.dispatch(e);
      else if(e.type==="osc"){oscEvent.fire("message",e);for(const b of oscBindings)if(b.address===e.address)b.fire("message",e);}
      const result=stringify({commands:pending.map(c=>c.initial?{...c,sample:c.immediate?e.sample:c.sample,generation:e.generation}:c),logs:logs.map(l=>l.initial?{...l,sample:e.sample}:l)});pending.length=0;logs.length=0;return result;
    }
  };
})()
