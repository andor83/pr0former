// Mirrors the versioned Rust model. Contract checks live in api.test.ts.
export type Mode = 'structured' | 'conducted' | 'freeform'
export type Signal = 'audio' | 'control' | 'spectral' | 'midi'
export interface Parameter { id: string; label: string; unit: string; min: number; max: number; default: number; logarithmic: boolean; structural: boolean }
export interface Port { id: string; label: string; signal: Signal; fixed_channels?: number | null }
export interface Descriptor { default_channels?: number; kind: string; label: string; symbol: string; category: string; description: string; aliases: string[]; inputs: Port[]; outputs: Port[]; parameters: Parameter[] }
export interface IoConfig { port: string; address: string; destination: string }
export interface GraphNode { io?: IoConfig | null; part_id?: string | null; library?:{id:string;version:number}|null; parent?: string | null; id: string; kind: string; label: string; x: number; y: number; channels: number; parameters: Record<string, number>; control_value?: number | string | null }
export interface GraphEdge { id: string; source: string; source_port: string; target: string; target_port: string }
export type MarkKind = 'text'|'rehearsal'|'cue'|'expression'|'tempo'|'lyric'
export interface StaffMark {id:string;beat:number;kind:MarkKind;text:string}
export type CurveKind = 'slur'|'bracket'|'crescendo'|'decrescendo'
export interface StaffCurve {id:string;kind:CurveKind;start_note?:string|null;start_beat:number;end_note?:string|null;end_beat:number;height:number;lift:number;end_lift?:number;start_dynamic?:Pick<AutomationEvent,'id'|'start'|'end'|'curve'>|null}
export interface Staff {curves?:StaffCurve[]; dynamics?:Dynamics|null; marks?:StaffMark[]; hidden_rests?:{beat:number;duration:number;voice:number}[]; key_mode?:'major'|'minor'|null; clef_changes?:{beat:number;clef:string}[]; instrument_node?:string|null;midi_channel?:number|null;midi_port?:string|null; id:string; name:string; clef:string; key_signature?:string|null; transpose:number }
export interface RationalTime {numerator:number;denominator:number}
export interface NoteNotation {onset?:RationalTime|null;written_duration?:RationalTime|null; tie_to?:string|null;slur_to?:string|null;grace_to?:string|null;articulation?:string|null;octave?:number; staff:string; step:number; alter:number; voice:number; base:number; dots:number; tuplet_actual:number; tuplet_normal:number }
export interface Note { notation?: NoteNotation; id: string; pitch: number; beat: number; duration: number; velocity: number; rest: boolean; tied: boolean }
export type MidiCurve = 'step'|'linear'|'ease_in'|'ease_out'|'s_curve'
export interface AutomationEvent {id:string;beat:number;duration:number;start:number;end:number;curve:MidiCurve}
export interface AutomationLane {initial?:number|null;id:string;name:string;channel:number;message:'note'|'cc'|'bend'|'program'|'pressure'|'poly_pressure';number:number;events:AutomationEvent[]}
export interface Dynamics { mode:'velocity'|'cc'|'both';controller:number;events:AutomationEvent[] }
export interface Part { muted?:boolean; solo?:boolean; dynamics?:Dynamics|null; automation?:AutomationLane[]; staves?: Staff[]; id: string; name: string; performer: string | null; view: string; clef: string; key_signature?: string | null; show_time_signature?: boolean; notes: Note[]; loop_beats: number; instrument_node: string | null; midi_port?: string | null; midi_channel?: number; osc_destination?: string | null; osc_address: string }
export interface ScoreTimeline { barlines?:{beat:number;style:string}[]; version:1; length:number; loop_score:boolean; meters:{beat:number;beats:number;unit:number}[]; keys:{beat:number;key:string;mode?:'major'|'minor'|null}[]; repeats:{start:number;end:number;times:number;first_ending?:number|null}[]; navigation?:{at:number;target:number;fine?:number|null;coda?:[number,number]|null}|null; tempos?:{beat:number;bpm:number}[] }
export interface Project { score?: ScoreTimeline|null; schema_version: number; id: string; name: string; mode: Mode; revision: number; bpm: number; beats_per_bar: number; beat_unit?: number; graph: { nodes: GraphNode[]; edges: GraphEdge[] }; parts: Part[] }
export interface PartPlayback { position_end?:number|null; id: string; playing: boolean; start: number; position: number; pending: [number, boolean] | null }
export interface Visualization {kind:'control'|'audio'|'spectral';value?:number|string;sequence?:number;generation?:number;size?:number;ready?:boolean;polar?:boolean;channels?:{magnitude:number[];phase:number[]}[];history?:string[];columns?:number}
export interface Telemetry { worker_max_work_us?:number; worker_max_block_gap_us?:number; route_targets?:Record<string,string>; midi_input_error?: string | null; node_io?: {error: string | null; dropped: number}; count_in_remaining?: number | null; metronome?: boolean; sample_rate?:number;block_size?:number;visualizations?:Record<string,Visualization>;  parts: PartPlayback[]; type: 'telemetry'; project_id: string; revision: number; epoch: string; sequence: number; server_time: number; sample: number; beat: number; bpm: number; running: boolean; hardware_enabled: boolean; underruns: number; error: string; values: Record<string, Record<string, number>> }
export interface Summary { id: string; name: string; mode: Mode; role: string; revision: number; bpm?:number; beats_per_bar?:number; beat_unit?:number; schema_version?:number; parts?:number; nodes?:number; owner?:string; opened?:number|null }
export interface Member { id: string; username: string; role: string }


export interface HardwareDeviceLevels {id:number;name:string;channels:number;levels:{channel:number;peak?:number}[]}
export interface HardwareLevels {project_id:string|null;inputs:HardwareDeviceLevels[];outputs:HardwareDeviceLevels[]}
