import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { isDemoMode, mockInvoke } from '@/lib/mockTauri'

/* Mirrors src-tauri/src/machine_usage.rs. Whole-machine utilization: GPU when
 * the OS exposes it, CPU otherwise. Not Nodea's share alone. */
export interface MachineUsage {
  source: 'gpu' | 'cpu'
  percent: number
}

const POLL_MS = 2000

/** Polls the node's machine usage while `enabled`; null until the first reading. */
export function useMachineUsage(enabled: boolean): MachineUsage | null {
  const [usage, setUsage] = useState<MachineUsage | null>(null)

  useEffect(() => {
    if (!enabled) {
      setUsage(null)
      return
    }
    const inv = isDemoMode() ? mockInvoke : invoke
    let cancelled = false
    const refresh = async () => {
      try {
        const next = await inv<MachineUsage>('machine_usage')
        if (!cancelled) setUsage(next)
      } catch (err) {
        console.error(err)
      }
    }
    refresh()
    const interval = setInterval(refresh, POLL_MS)
    return () => {
      cancelled = true
      clearInterval(interval)
    }
  }, [enabled])

  return usage
}
