import { describe, it, expect } from 'vitest'
import { finiteInput, formatValue } from './api'
describe('parameter display and input boundaries', () => {
  it('does not turn empty or nonfinite input into an edit', () => {
    expect(finiteInput('', -90, 24)).toBeNull()
    expect(finiteInput('Infinity', -90, 24)).toBeNull()
    expect(finiteInput('25', -90, 24)).toBeNull()
    expect(finiteInput('-12.5', -90, 24)).toBe(-12.5)
  })
  it('distinguishes missing telemetry from zero', () => {
    expect(formatValue(undefined)).toBe('—')
    expect(formatValue(0, 'Hz')).toBe('0 Hz')
    expect(formatValue(1.23456)).toBe('1.235')
  })
})
