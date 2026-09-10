// UI precision only; incoming engine values retain their full numeric precision.
export function roundSlider(value:number,integer=false):number {
  return integer?Math.round(value):Number(value.toFixed(2))
}
export function formatSlider(value:number,integer=false):string {
  if(!Number.isFinite(value))return '—'
  const rounded=roundSlider(value,integer)
  return Number.isInteger(rounded)?String(rounded):rounded.toFixed(2)
}
