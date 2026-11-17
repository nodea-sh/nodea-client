import { useMemo } from 'react'
import type { TokenStatsRange, TokenStatsResponse } from '@/lib/api'

export type ChartRange = TokenStatsRange

const AXIS_LABELS = 5

/** Bucket width in minutes per range (backend/repositories/events.py TOKENS_TS_RANGES). */
const BUCKET_MINUTES: Record<ChartRange, number> = { '1h': 2, '1d': 60, '1w': 360 }

const compact = new Intl.NumberFormat('en-US', { notation: 'compact', maximumFractionDigits: 1 })
/** Whole tokens unless the rate is low enough that a decimal matters. */
const formatRate = (rate: number) =>
  new Intl.NumberFormat('en-US', { maximumFractionDigits: rate < 10 ? 1 : 0 }).format(rate)

/** value 0-100 → ramp step */
function barColor(pct: number) {
  if (pct > 88) return 'var(--n-bar-6)'
  if (pct > 70) return 'var(--n-bar-5)'
  if (pct > 55) return 'var(--n-bar-4)'
  if (pct > 40) return 'var(--n-bar-3)'
  if (pct > 25) return 'var(--n-bar-2)'
  return 'var(--n-bar-1)'
}

/** Each bucket's tokens averaged over its width in minutes. */
export function perMinute(stats: TokenStatsResponse[], range: ChartRange): number[] {
  return stats.map((s) => s.value / BUCKET_MINUTES[range])
}

/** Smallest 1, 2 or 5 × 10^k at or above `max`, so axis ticks are round numbers. */
export function niceMax(max: number): number {
  if (max <= 0) return 1
  const pow = 10 ** Math.floor(Math.log10(max))
  return ([1, 2, 5, 10].find((m) => m * pow >= max) ?? 10) * pow
}

function bucketLabel(date: string, range: ChartRange): string {
  const d = new Date(date)
  const time = d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false })
  return range === '1w' ? `${d.toLocaleDateString([], { weekday: 'short' })} ${time}` : time
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
  const rates = useMemo(() => perMinute(stats, range), [stats, range])
  const top = niceMax(Math.max(...rates, 0))
  const busiest = Math.max(...rates, 1)
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

      <div className="mt-3.5 flex gap-2">
        <div data-testid="throughput-axis"
             className="flex h-24 w-9 shrink-0 flex-col justify-between text-right text-[11px] leading-none text-[var(--n-text-4)] tabular-nums">
          {[top, top / 2, 0].map((v) => <span key={v} className="-my-[5px]">{compact.format(v)}</span>)}
        </div>

        <div className="relative flex h-24 flex-1 items-end gap-[3px]">
          {[0, 50, 100].map((p) => (
            <div key={p} className="pointer-events-none absolute inset-x-0 border-t border-dashed border-[var(--n-line-strong)]"
                 style={{ bottom: `${p}%` }} />
          ))}
          {rates.map((rate, i) => {
            const align = i < 3 ? 'left-0' : i > rates.length - 4 ? 'right-0' : 'left-1/2 -translate-x-1/2'
            return (
              <div key={i} className="group relative flex h-full flex-1 items-end">
                <div data-testid="throughput-bar" aria-label={`${formatRate(rate)} tokens / min at ${bucketLabel(stats[i].date, range)}`}
                     className="w-full rounded-t-[2px] transition-[height,filter] duration-150 ease-out group-hover:brightness-125"
                     style={{ height: `${Math.max((rate / top) * 100, 2)}%`, background: barColor((rate / busiest) * 100) }} />
                <div role="tooltip"
                     className={`pointer-events-none absolute top-1 z-10 hidden whitespace-nowrap rounded-[var(--n-radius-sm)] bg-[var(--n-raised)] px-2 py-1.5 text-[12px] leading-none shadow-[var(--n-shadow-md)] group-hover:block ${align}`}>
                  <div className="font-semibold text-[var(--n-text)] tabular-nums">{formatRate(rate)} tok/min</div>
                  <div className="mt-1 text-[var(--n-text-4)]">{bucketLabel(stats[i].date, range)}</div>
                </div>
              </div>
            )
          })}
        </div>
      </div>

      <div className="mt-2 flex justify-between pl-11 text-[12px] leading-none text-[var(--n-text-3)]">
        {labels.map((l, i) => <span key={i}>{l}</span>)}
      </div>
    </div>
  )
}
