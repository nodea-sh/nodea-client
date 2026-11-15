import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { isDemoMode, mockInvoke } from '@/lib/mockTauri'

/* Mirrors nodea_usage::NodeaUsage (src-tauri/crates/nodea-usage): how much of
 * this device's GPU - CPU on a CPU-only build - the Nodea process itself is
 * using. Other apps don't count. `percent` is null when the OS gave no reading. */
export interface NodeaUsage {
  source: 'gpu' | 'cpu'
  percent: number | null
}

const POLL_MS = 2000

/** Polls Nodea's usage while `enabled`; null until the first answer. */
export function useNodeaUsage(enabled: boolean): NodeaUsage | null {
  const [usage, setUsage] = useState<NodeaUsage | null>(null)

  useEffect(() => {
    if (!enabled) {
      setUsage(null)
      return
    }
    const inv = isDemoMode() ? mockInvoke : invoke
    let cancelled = false
    const refresh = async () => {
      try {
        const next = await inv<NodeaUsage>('nodea_usage')
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
