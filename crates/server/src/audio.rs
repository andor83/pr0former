use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use pr0_core::{MAX_CHANNELS, Project};
use pr0_dsp::{BLOCK, Engine};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU64, Ordering},
        mpsc::{SyncSender, sync_channel},
    },
    time::{Duration, Instant},
};
use tokio::sync::{broadcast, oneshot};

static START: OnceLock<Instant> = OnceLock::new();
pub fn monotonic_ms() -> f64 {
    START.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.
}
pub enum Command {
    Load(Project, Box<Engine>),
    Replace {
        project: Project,
        engine: Box<Engine>,
    },
    Unload,
    Parameter {
        node: String,
        key: String,
        value: f64,
        revision: u64,
    },
    Transport(String),
    Tempo(f64),
    Devices(oneshot::Sender<Value>),
    Hardware(bool),
    Capture(bool),
    Clip {
        part: String,
        playing: bool,
    },
    BrowserInput {
        node: String,
        pcm: Vec<[f32; 2]>,
    },
}
pub fn start(
    events: broadcast::Sender<Value>,
    media: broadcast::Sender<crate::media::AudioBlock>,
) -> SyncSender<Command> {
    let (tx, rx) = sync_channel::<Command>(256);
    std::thread::Builder::new().name("pr0-orchestrator".into()).spawn(move||{
    let io=crate::performance::external_worker();let mut sequencer:Option<crate::performance::Sequencer>=None;let mut engine:Option<Engine>=None;let mut project:Option<Project>=None;let mut epoch=String::new();let mut stream=None;let mut output_queue=None;let mut input_stream=None;let mut input_queue:Option<rtrb::Consumer<[f32;MAX_CHANNELS]>>=None;let mut hardware=false;let underruns=Arc::new(AtomicU64::new(0));let mut last=Instant::now();let mut deadline=Instant::now();let mut seq=0_u64;let mut pending_graph:Option<(f64,Project,Box<Engine>)>=None;let mut next_tempo:Option<(f64,f64)>=None;let mut device_error=String::new();let mut browser:std::collections::BTreeMap<String,std::collections::VecDeque<[f32;2]>>=std::collections::BTreeMap::new();
    loop{
        while let Ok(command)=rx.try_recv(){match command{
            Command::Capture(enabled)=>{if !enabled{input_stream=None;input_queue=None;}else if input_stream.is_none(){match open_input(){Ok((stream,queue))=>{input_stream=Some(stream);input_queue=Some(queue);device_error.clear();},Err(e)=>device_error=e}}},
            Command::Clip{part,playing}=>{if let(Some(seq),Some(e))=(&mut sequencer,&mut engine){seq.launch(&part,playing,e,&io);}},
            Command::BrowserInput{node,pcm}=>{if project.as_ref().is_some_and(|p|p.graph.nodes.iter().any(|n|n.id==node&&n.kind=="browser_input")){let q=browser.entry(node).or_default();if q.len()+pcm.len()>4800 {q.clear();}q.extend(pcm);}},
            Command::Replace{project:p,engine:prepared}=>{let boundary=engine.as_ref().map(|e|if e.clock.running{((e.clock.beat/p.quarter_beats_per_bar()).floor()+1.)*p.quarter_beats_per_bar()}else{e.clock.beat}).unwrap_or(0.);pending_graph=Some((boundary,p,prepared));},
            Command::Load(p,mut e)=>{sequencer=Some(crate::performance::Sequencer::new(&p));e.clock.bpm=p.bpm;engine=Some(*e);project=Some(p);epoch=uuid::Uuid::new_v4().to_string();next_tempo=None;},
            Command::Unload=>{pending_graph=None;let _=io.try_send(crate::performance::External::Panic);sequencer=None;engine=None;project=None;browser.clear();hardware=false;stream=None;output_queue=None;input_stream=None;input_queue=None;},
            Command::Parameter{node,key,value,revision}=>{if let Some(e)=engine.as_mut(){if let Err(err)=e.parameter(&node,&key,value){device_error=err;}if let Some(p)=project.as_mut(){p.revision=revision;if let Some(n)=p.graph.nodes.iter_mut().find(|n|n.id==node){n.parameters.insert(key,value);}}}},
            Command::Transport(action)=>if let Some(e)=engine.as_mut(){match action.as_str(){"play"=>e.clock.running=true,"pause"=>{if let Some(seq)=&mut sequencer{seq.pause(e,&io);}e.clock.running=false;},"stop"=>{if let Some(seq)=&mut sequencer{seq.reset(e,&io);}e.clock.stop();next_tempo=None;},_=>{}}},
            Command::Tempo(bpm)=>if let Some(e)=engine.as_mut(){if e.clock.running{next_tempo=Some((e.clock.beat.floor()+1.,bpm));}else{e.clock.set_tempo(bpm);}},
            Command::Devices(reply)=>{let host=cpal::default_host();let devices=host.output_devices().map(|ds|ds.filter_map(|d|d.name().ok()).collect::<Vec<_>>()).unwrap_or_default();let midi=midir::MidiOutput::new("pr0former device list").ok().map(|m|m.ports().iter().filter_map(|p|m.port_name(p).ok()).collect::<Vec<_>>()).unwrap_or_default();let _=reply.send(json!({"outputs":devices,"midi_outputs":midi,"selected":"System default (48 kHz / f32)","hardware_enabled":hardware,"input_enabled":input_stream.is_some(),"underruns":underruns.load(Ordering::Relaxed),"error":device_error}));},
            Command::Hardware(enabled)=>{if !enabled{stream=None;output_queue=None;hardware=false;}else if stream.is_none(){let host=cpal::default_host();let result=(||->Result<_,String>{let device=host.default_output_device().ok_or("No output device")?;let supported=device.supported_output_configs().map_err(|e|e.to_string())?.find(|c|c.sample_format()==cpal::SampleFormat::F32&&c.min_sample_rate().0<=48000&&c.max_sample_rate().0>=48000).ok_or("Output requires a 48 kHz f32 configuration")?;let mut config=supported.with_sample_rate(cpal::SampleRate(48000)).config();config.buffer_size=cpal::BufferSize::Default;let channels=config.channels as usize;let(producer,mut consumer)=rtrb::RingBuffer::<[f32;MAX_CHANNELS]>::new(4096);let u=underruns.clone();let s=device.build_output_stream(&config,move|data:&mut[f32],_|{for frame in data.chunks_mut(channels){let samples=consumer.pop().unwrap_or_else(|_|{u.fetch_add(1,Ordering::Relaxed);[0.;MAX_CHANNELS]});for(ch,v)in frame.iter_mut().enumerate(){*v=samples.get(ch).copied().unwrap_or(0.);}}},move|_|{},None).map_err(|e|e.to_string())?;s.play().map_err(|e|e.to_string())?;Ok((s,producer))})();match result{Ok((s,p))=>{stream=Some(s);output_queue=Some(p);hardware=true;device_error.clear();},Err(e)=>device_error=e}}}
        }}
        if pending_graph.as_ref().is_some_and(|(boundary,_,_)|engine.as_ref().is_some_and(|e|e.clock.beat>=*boundary||!e.clock.running)){
            let(_,p,mut prepared)=pending_graph.take().unwrap();if let Some(previous)=&mut engine{prepared.clock=previous.clock;sequencer=Some(match sequencer.take(){Some(seq)=>seq.replace(&p,previous,&mut prepared,&io),None=>crate::performance::Sequencer::new(&p)});prepared.carry_node_state(previous);}project=Some(p);engine=Some(*prepared);
        }
        if output_queue.as_ref().is_some_and(|q|q.slots()<4096-256){std::thread::sleep(Duration::from_micros(500));continue;}
        if let Some(e)=engine.as_mut(){
            let mut output=[[0.;MAX_CHANNELS];BLOCK];
            let mut monitors:std::collections::BTreeMap<String,Vec<f32>>=if media.receiver_count()>0 {project.as_ref().map(|p|p.graph.nodes.iter().filter(|n|n.kind=="monitor_output").map(|n|(n.id.clone(),Vec::with_capacity(BLOCK*2))).collect()).unwrap_or_default()}else{Default::default()};
            // Split rendering at the exact tempo boundary; no browser timer drives DSP.
            for frame in &mut output{for(node,q)in &mut browser{let sample=q.pop_front().unwrap_or([0.;2]);let mut audio=[0.;MAX_CHANNELS];audio[..2].copy_from_slice(&sample);e.external(node,audio);}if let Some((beat,bpm))=next_tempo{if e.clock.beat>=beat{e.clock.set_tempo(bpm);next_tempo=None;}}if let Some(seq)=&mut sequencer{seq.tick(e,&io);}let input=input_queue.as_mut().and_then(|q|q.pop().ok()).unwrap_or([0.;MAX_CHANNELS]);e.render(std::slice::from_ref(&input),std::slice::from_mut(frame));for(id,pcm)in &mut monitors{pcm.extend_from_slice(&if e.clock.running{e.monitor_frame(id)}else{[0.;2]});}}
            if media.receiver_count()>0{if let Some(p)=&project {let pcm=output.iter().flat_map(|f|if e.clock.running{[f[0],f[1]]}else{[0.;2]}).collect();let _=media.send(crate::media::AudioBlock{project:p.id.clone(),pcm:Arc::new(pcm),monitors:monitors.into_iter().map(|(id,pcm)|(id,Arc::new(pcm))).collect()});}}
            if let Some(queue)=output_queue.as_mut(){for frame in output{let _=queue.push(if e.clock.running{frame}else{[0.;MAX_CHANNELS]});}}
            if last.elapsed()>=Duration::from_millis(50){last=Instant::now();seq+=1;if let Some(p)=&project{let _=events.send(json!({"type":"telemetry","project_id":p.id,"revision":p.revision,"epoch":epoch,"sequence":seq,"server_time":monotonic_ms(),"sample":e.clock.sample,"beat":e.clock.beat,"bpm":e.clock.bpm,"running":e.clock.running,"hardware_enabled":hardware,"input_enabled":input_stream.is_some(),"underruns":underruns.load(Ordering::Relaxed),"error":device_error,"parts":sequencer.as_ref().map(|s|s.playback(e.clock.beat)).unwrap_or_default(),"values":e.telemetry()}));}}
        }
        if hardware {deadline=Instant::now();continue;}
        deadline+=Duration::from_secs_f64(BLOCK as f64/48000.);let now=Instant::now();if deadline>now{std::thread::sleep(deadline-now);}else if now.duration_since(deadline)>Duration::from_millis(100){deadline=now;}
    }
}).expect("Start audio worker");
    tx
}

fn open_input() -> Result<(cpal::Stream, rtrb::Consumer<[f32; MAX_CHANNELS]>), String> {
    let host = cpal::default_host();
    let device = host.default_input_device().ok_or("No input device")?;
    let supported = device
        .supported_input_configs()
        .map_err(|e| e.to_string())?
        .find(|c| {
            c.sample_format() == cpal::SampleFormat::F32
                && c.min_sample_rate().0 <= 48000
                && c.max_sample_rate().0 >= 48000
        })
        .ok_or("Input requires a 48 kHz f32 configuration")?;
    let config = supported.with_sample_rate(cpal::SampleRate(48000)).config();
    let channels = config.channels as usize;
    let (mut producer, consumer) = rtrb::RingBuffer::new(4096);
    let stream = device
        .build_input_stream(
            &config,
            move |data: &[f32], _| {
                for frame in data.chunks(channels) {
                    let mut samples = [0.; MAX_CHANNELS];
                    for (ch, sample) in frame.iter().take(MAX_CHANNELS).enumerate() {
                        samples[ch] = *sample;
                    }
                    let _ = producer.push(samples);
                }
            },
            move |_| {},
            None,
        )
        .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok((stream, consumer))
}
