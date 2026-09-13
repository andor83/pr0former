export const desktopViews = ['graph', 'score', 'conductor', 'ensemble', 'monitor', 'stage'] as const
export function desktopView(value: string | null): string {
  return desktopViews.includes(value as typeof desktopViews[number]) ? value! : 'graph'
}
export function isDesktopWindow(): boolean {
  return (window as Window & { __PR0_DESKTOP__?: boolean }).__PR0_DESKTOP__ === true
}
export function desktopProject(): string | null {
  return isDesktopWindow() ? new URLSearchParams(location.search).get('project') : null
}
export function updateDesktopWindow(id: string | undefined, name: string | undefined, view: string) {
  if (!isDesktopWindow()) return
  const url = new URL(location.href)
  if (id) { url.searchParams.set('project', id); url.searchParams.set('view', desktopView(view)) }
  else { url.searchParams.delete('project'); url.searchParams.delete('view') }
  history.replaceState({}, '', url)
  // Native code observes the URL and title; remote pages receive no IPC permission.
  document.title = id ? `${name || 'Performance'} — ${desktopView(view)} — pr0former` : 'pr0former'
}
