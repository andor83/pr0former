// Mirrors the versioned Rust model. Contract checks live in api.test.ts.
export type Mode = 'structured' | 'conducted' | 'freeform'
export type Signal = 'audio' | 'control' | 'spectral'
export interface Parameter { id: string; label: string; unit: string; min: number; max: number; default: number; logarithmic: boolean; structural: boolean }
export interface Port { id: string; label: string; signal: Signal; fixed_channels?: number | null }
export interface Descriptor { default_channels?: number; kind: string; label: string; symbol: string; category: string; description: string; aliases: string[]; inputs: Port[]; outputs: Port[]; parameters: Parameter[] }
export interface IoConfig { port: string; address: string; destination: string }
export interface GraphNode { io?: IoConfig | null; part_id?: string | null; library?:{id:string;version:number}|null; parent?: string | null; id: string; kind: string; label: string; x: number; y: number; channels: number; parameters: Record<string, number>; control_value?: number | string | null }
export interface GraphEdge { id: string; source: string; source_port: string; target: string; target_port: string }
export interface Note { id: string; pitch: number; beat: number; duration: number; velocity: number; rest: boolean; tied: boolean }
export interface Part { id: string; name: string; performer: string | null; view: string; clef: string; key_signature?: string | null; show_time_signature?: boolean; notes: Note[]; loop_beats: number; instrument_node: string | null; midi_port?: string | null; midi_channel?: number; osc_destination?: string | null; osc_address: string }
export interface Project { schema_version: number; id: string; name: string; mode: Mode; revision: number; bpm: number; beats_per_bar: number; beat_unit?: number; graph: { nodes: GraphNode[]; edges: GraphEdge[] }; parts: Part[] }
export interface PartPlayback { id: string; playing: boolean; start: number; position: number; pending: [number, boolean] | null }
export interface Visualization {kind:'control'|'audio'|'spectral';value?:number|string;sequence?:number;generation?:number;size?:number;ready?:boolean;polar?:boolean;channels?:{magnitude:number[];phase:number[]}[];history?:string[];columns?:number}
export interface Telemetry { midi_input_error?: string | null; node_io?: {error: string | null; dropped: number}; count_in_remaining?: number | null; sample_rate?:number;block_size?:number;visualizations?:Record<string,Visualization>;  parts: PartPlayback[]; type: 'telemetry'; project_id: string; revision: number; epoch: string; sequence: number; server_time: number; sample: number; beat: number; bpm: number; running: boolean; hardware_enabled: boolean; underruns: number; error: string; values: Record<string, Record<string, number>> }
export interface Summary { id: string; name: string; mode: Mode; role: string; revision: number }
export interface Member { id: string; username: string; role: string }


export interface HardwareDeviceLevels {id:number;name:string;channels:number;levels:{channel:number;peak?:number}[]}
export interface HardwareLevels {project_id:string|null;inputs:HardwareDeviceLevels[];outputs:HardwareDeviceLevels[]}
