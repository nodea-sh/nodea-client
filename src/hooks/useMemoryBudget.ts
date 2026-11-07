import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { isDemoMode, mockInvoke } from '@/lib/mockTauri'

interface SystemMemory {
  total_memory_mb: number
  available_memory_mb: number
  accelerator_memory_mb: number | null
}

// When the memory can't be read (plain web mode), assume a 16 GB Mac.
const FALLBACK_BUDGET_MB = 10 * 1024

/* The memory a model on this machine can use, in MB: the GPU's working set
 * when there is one (Metal on Apple Silicon), else 60% of RAM, leaving the
 * rest for the OS and other apps. Only drives which models are *suggested* -
 * the backend still decides what each node loads. */
export function useMemoryBudget(): number | null {
  const [budget, setBudget] = useState<number | null>(null)

  useEffect(() => {
    const inv = isDemoMode() ? mockInvoke : invoke
    inv<SystemMemory>('get_system_memory')
      .then((m) => setBudget(m.accelerator_memory_mb ?? Math.round(m.total_memory_mb * 0.6)))
      .catch(() => setBudget(FALLBACK_BUDGET_MB))
  }, [])

  return budget
}
