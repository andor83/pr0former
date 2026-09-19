import type { GraphNode } from './types'
export type StateOptions = Pick<GraphNode,'control_positions'|'id'|'kind'|'label'|'channels'|'parameters'|'control_value'|'sample_choices'|'script'|'io'>
export interface StateSlot {slot:number;name:string;nodes:StateOptions[]}
export interface StateBank {next_slot:number;slots:StateSlot[]}
/** Only visible nodes animate; navigation and reconnection never replay feedback. */
export class RecallFeedback {
  private seen = new Set<string>()
  accept(event:string, restored:string[], visible:string[]):string[] {
    if(this.seen.has(event))return []
    this.seen.add(event)
    if(this.seen.size>128)this.seen.delete(this.seen.values().next().value!)
    const ids=new Set(visible)
    return restored.filter(id=>ids.has(id))
  }
}
export function effectiveNode(node:GraphNode, options:Record<string,StateOptions>):GraphNode {
  const live=options[node.id]
  if(live?.kind!==node.kind)return node
  const same=Object.entries(live).every(([key,value])=>{const fallback=['sample_choices','control_positions'].includes(key)?[]:null;return JSON.stringify(value??fallback)===JSON.stringify(node[key as keyof GraphNode]??fallback)})
  return same?node:{...node,...live}
}
