import {describe,it,expect} from 'vitest'
import {roundSlider,formatSlider} from './sliderNumbers'
describe('slider precision',()=>{
 it('commits hundredths without binary tails, including negative values',()=>{
  expect(roundSlider(0.123456)).toBe(0.12)
  expect(roundSlider(-12.346)).toBe(-12.35)
  expect(roundSlider(440.999)).toBe(441)
 })
 it('formats fractions with two places and whole values without decimals',()=>{
  expect(formatSlider(0.1)).toBe('0.10')
  expect(formatSlider(12.3456)).toBe('12.35')
  expect(formatSlider(440)).toBe('440')
  expect(formatSlider(-0.001)).toBe('0')
  expect(formatSlider(63.9,true)).toBe('64')
 })
})
