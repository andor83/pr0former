export async function api<T>(path: string, method = 'GET', body?: unknown): Promise<T> {
  const response = await fetch(`/api${path}`, { method, credentials: 'same-origin', headers: { 'Content-Type': 'application/json', 'X-Pr0former': '1' }, body: body === undefined ? undefined : JSON.stringify(body) })
  const result = await response.json()
  if (!response.ok) throw new Error(result.error || `Request failed (${response.status})`)
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

