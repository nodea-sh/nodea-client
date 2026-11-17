import { describe, expect, it } from 'vitest'
import { niceMax, perMinute } from './ThroughputChart'

describe('perMinute', () => {
  it('divides each bucket by its width in minutes', () => {
    const stats = [{ date: '2026-01-01T00:00:00Z', value: 120 }]
    expect(perMinute(stats, '1h')).toEqual([60])
    expect(perMinute(stats, '1d')).toEqual([2])
    expect(perMinute(stats, '1w')).toEqual([120 / 360])
  })
})

describe('niceMax', () => {
  it('rounds up to 1, 2 or 5 times a power of ten', () => {
    expect(niceMax(0)).toBe(1)
    expect(niceMax(0.3)).toBe(0.5)
    expect(niceMax(7)).toBe(10)
    expect(niceMax(130)).toBe(200)
    expect(niceMax(200)).toBe(200)
    expect(niceMax(4100)).toBe(5000)
  })
})
