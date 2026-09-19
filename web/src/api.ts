export class ApiError extends Error { constructor(message:string,readonly status:number){super(message);this.name="ApiError"} }
export async function api<T>(path: string, method = 'GET', body?: unknown, extraHeaders:Record<string,string>={}): Promise<T> {
  const response = await fetch(`/api${path}`, { method, credentials: 'same-origin', headers: { 'Content-Type': 'application/json', 'X-Pr0former': '1', ...extraHeaders }, body: body === undefined ? undefined : JSON.stringify(body) })
  const result = await response.json()
  if (!response.ok) throw new ApiError(result.error || `Request failed (${response.status})`,response.status)
  return result
}
export function formatValue(value: number | undefined, unit = ''): string {
  if (value === undefined || !Number.isFinite(value)) return '—'
  const digits = Math.abs(value) >= 100 ? 1 : Math.abs(value) >= 10 ? 2 : 3
  return `${Number(value.toFixed(digits))}${unit ? ` ${unit}` : ''}`
}
export function finiteInput(value: string, min: number, max: number): number | null {
  if (!value.trim()) return null
  const number = Number(value)
  return Number.isFinite(number) && number >= min && number <= max ? number : null
}

