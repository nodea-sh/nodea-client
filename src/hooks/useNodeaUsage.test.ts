import { act, renderHook } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
vi.mock('@/lib/mockTauri', () => ({ isDemoMode: () => false, mockInvoke: vi.fn() }))

import { useNodeaUsage } from './useNodeaUsage'

describe('useNodeaUsage (#317)', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    invoke.mockReset()
  })
  afterEach(() => vi.useRealTimers())

  it('polls the nodea_usage command every 2 seconds while enabled', async () => {
    invoke.mockResolvedValueOnce({ source: 'gpu', percent: 0 }).mockResolvedValueOnce({ source: 'gpu', percent: 84 })
    const { result } = renderHook(() => useNodeaUsage(true))

    await act(async () => {})
    expect(invoke).toHaveBeenCalledWith('nodea_usage')
    expect(result.current).toEqual({ source: 'gpu', percent: 0 })

    await act(async () => {
      await vi.advanceTimersByTimeAsync(2000)
    })
    expect(result.current).toEqual({ source: 'gpu', percent: 84 })
  })

  it('does not poll while disabled, and forgets the last reading', async () => {
    invoke.mockResolvedValue({ source: 'gpu', percent: 50 })
    const { result, rerender } = renderHook(({ on }) => useNodeaUsage(on), { initialProps: { on: true } })
    await act(async () => {})
    expect(result.current?.percent).toBe(50)

    rerender({ on: false })
    const calls = invoke.mock.calls.length
    await act(async () => {
      await vi.advanceTimersByTimeAsync(10000)
    })
    expect(invoke.mock.calls.length).toBe(calls)
    expect(result.current).toBeNull()
  })
})
