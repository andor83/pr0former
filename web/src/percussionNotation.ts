export const noteheads = { normal: 'Normal', cross: 'Cross ×', 'circle-cross': 'Circled cross ⊗', diamond: 'Diamond ◆', triangle: 'Triangle ▲', 'triangle-open': 'Open triangle △', ghost: 'Ghost (note)' }
export const drumMarks = { none: 'None', 'roll-1': 'One roll slash', 'roll-2': 'Two roll slashes / doubles', 'roll-3': 'Three roll slashes', buzz: 'Buzz roll Z' }
export function headSuffix(head?: string | null) {
  return ({ cross: '/X', 'circle-cross': '/X3', diamond: '/D2', triangle: '/T2', 'triangle-open': '/T1' } as Record<string, string>)[head || ''] || ''
}
