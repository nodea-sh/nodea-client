import type { NodeaUsage } from '@/hooks/useNodeaUsage'

export const USAGE_TOOLTIP =
  "How much of this device's GPU Nodea is using right now (the CPU, on a device without a usable GPU). " +
  "Other apps don't count. Updates every 2 seconds."

/** The serving/paused ring: Nodea's own usage, or a dash without a reading (#317). */
export function usageRing(usage: NodeaUsage | null) {
  const caption = usage?.source === 'cpu' ? 'Nodea CPU' : 'Nodea GPU'
  if (usage?.percent == null) return { ringPct: 0, ringLabel: '—', ringCaption: caption }
  const pct = Math.max(0, Math.min(100, Math.round(usage.percent)))
  return { ringPct: pct, ringLabel: `${pct}%`, ringCaption: caption }
}
