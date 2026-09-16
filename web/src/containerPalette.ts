/** Sixteen frame colours for Container nodes; the index is stored in the node's `color` parameter. */
export const CONTAINER_PALETTE: { name: string; hex: string }[] = [
  { name: 'Slate', hex: '#5b6b70' }, { name: 'Cyan', hex: '#2f8f8a' }, { name: 'Teal', hex: '#2e7d6a' }, { name: 'Green', hex: '#4a8a3f' },
  { name: 'Lime', hex: '#7f9a2f' }, { name: 'Amber', hex: '#b58a2e' }, { name: 'Orange', hex: '#b8642a' }, { name: 'Red', hex: '#a8433c' },
  { name: 'Rose', hex: '#a84a72' }, { name: 'Magenta', hex: '#8f3f8f' }, { name: 'Violet', hex: '#6d4fa3' }, { name: 'Indigo', hex: '#4a56a8' },
  { name: 'Blue', hex: '#3a6fb0' }, { name: 'Sky', hex: '#3d86a8' }, { name: 'Brown', hex: '#7d5a3c' }, { name: 'Graphite', hex: '#3f464a' },
]
export function containerColor(index: number | undefined): { name: string; hex: string } {
  return CONTAINER_PALETTE[Math.max(0, Math.min(CONTAINER_PALETTE.length - 1, Math.round(index ?? 0)))]!
}
