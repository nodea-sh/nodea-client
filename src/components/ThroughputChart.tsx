import { useMemo } from 'react'
import type { TokenStatsRange, TokenStatsResponse } from '@/lib/api'

export type ChartRange = TokenStatsRange

const AXIS_LABELS = 5

/** value 0-100 → ramp step */
function barColor(pct: number) {
  if (pct > 88) return 'var(--n-bar-6)'
  if (pct > 70) return 'var(--n-bar-5)'
  if (pct > 55) return 'var(--n-bar-4)'
  if (pct > 40) return 'var(--n-bar-3)'
  if (pct > 25) return 'var(--n-bar-2)'
  return 'var(--n-bar-1)'
}

/** Each bucket's share of the busiest one, 0-100. */
function heights(stats: TokenStatsResponse[]): number[] {
  const max = Math.max(...stats.map((s) => s.value), 1)
  return stats.map((s) => Math.round((s.value / max) * 100))
}

function axisLabel(t: number, last: number, range: ChartRange): string {
  if (t === last) return 'now'
  const date = new Date(t)
  switch (range) {
    case '1h':
      return `−${Math.round((last - t) / 60000)}m`
    case '1d':
      return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false })
    case '1w':
      return date.toLocaleDateString([], { weekday: 'short' })
  }
}

/** Evenly spaced labels between the first and the last (current) bucket. */
function axisLabels(stats: TokenStatsResponse[], range: ChartRange): string[] {
  if (stats.length < 2) return []
  const first = new Date(stats[0].date).getTime()
  const last = new Date(stats[stats.length - 1].date).getTime()
  return Array.from({ length: AXIS_LABELS }, (_, k) =>
    axisLabel(first + ((last - first) * k) / (AXIS_LABELS - 1), last, range))
}

/**
 * Renders the backend's buckets as-is, one bar each (oldest first); the range
 * is owned by the caller because it decides what gets fetched.
 */
export function ThroughputChart({ stats, range, onRangeChange }: {
  stats: TokenStatsResponse[]
  range: ChartRange
  onRangeChange: (range: ChartRange) => void
}) {
  const bars = useMemo(() => heights(stats), [stats])
  const labels = useMemo(() => axisLabels(stats, range), [stats, range])

  return (
    <div className="mt-[26px]">
      <div className="flex items-center justify-between gap-3">
        <span className="text-[14px] font-semibold leading-none text-[var(--n-text-2)]">
          Throughput <span className="font-normal text-[var(--n-text-3)]">· tokens / min</span>
        </span>
        <span className="flex gap-0.5 rounded-[7px] bg-[var(--n-sidebar)] p-0.5">
          {(['1h', '1d', '1w'] as ChartRange[]).map((r) => (
            <button key={r} onClick={() => onRangeChange(r)}
              className="rounded-[5px] px-2.5 py-1 text-[13px] font-semibold leading-none transition-colors"
              style={
                range === r
                  ? { background: 'var(--n-tint-selected)', color: 'var(--n-accent-200)' }
                  : { background: 'transparent', color: 'var(--n-text-4)' }
              }>
              {r}
            </button>
          ))}
        </span>
      </div>

      <div className="mt-3.5 flex h-24 items-end gap-[3px]">
        {bars.map((pct, i) => (
          <div key={i} data-testid="throughput-bar"
               className="flex-1 rounded-t-[2px] transition-[height] duration-150 ease-out"
               style={{ height: `${Math.max(pct, 2)}%`, background: barColor(pct) }} />
        ))}
      </div>

      <div className="mt-2 flex justify-between text-[12px] leading-none text-[var(--n-text-3)]">
        {labels.map((l, i) => <span key={i}>{l}</span>)}
      </div>
    </div>
  )
}
