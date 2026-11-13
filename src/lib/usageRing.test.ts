import { describe, expect, it } from 'vitest'
import { usageRing } from './usageRing'

describe('usageRing (#317)', () => {
  it.each([0, 50, 100])('shows the reading from the node, %i%%', (percent) => {
    expect(usageRing({ source: 'gpu', percent })).toEqual({
      ringPct: percent,
      ringLabel: `${percent}%`,
      ringCaption: 'Nodea GPU',
    })
  })

  it('an idle serving node reads 0%, not a made-up figure', () => {
    expect(usageRing({ source: 'gpu', percent: 0 }).ringLabel).toBe('0%')
  })

  it('shows a dash, never a number, before the first answer or without a reading', () => {
    expect(usageRing(null)).toEqual({ ringPct: 0, ringLabel: '—', ringCaption: 'Nodea GPU' })
    expect(usageRing({ source: 'gpu', percent: null }).ringLabel).toBe('—')
  })

  it('names the CPU on a CPU-only device', () => {
    expect(usageRing({ source: 'cpu', percent: 33 }).ringCaption).toBe('Nodea CPU')
  })

  it('rounds and clamps out-of-range readings', () => {
    expect(usageRing({ source: 'gpu', percent: 49.6 }).ringLabel).toBe('50%')
    expect(usageRing({ source: 'gpu', percent: 130 }).ringPct).toBe(100)
    expect(usageRing({ source: 'gpu', percent: -5 }).ringPct).toBe(0)
  })
})
