import { useEffect, useState } from 'react'
import { useOutletContext } from 'react-router-dom'
import { CircleNotch, DownloadSimple, CheckCircle, CircleDashed, Broadcast, WifiSlash, Prohibit, Cube } from '@phosphor-icons/react'
import { useAuth } from '@/contexts/AuthContext'
import { apiClient, type TokenStatsResponse } from '@/lib/api'
import { ThroughputChart, type ChartRange } from '@/components/ThroughputChart'
import { announceModelsChanged, type NodeState } from '@/hooks/useNodePhase'
import { AddModelsDialog } from '@/components/AddModelsDialog'
import { SetupChecklist } from '@/components/SetupChecklist'
import { PrimaryButton } from '@/components/nocturne'
import { downloadSummary, formatDuration } from '@/lib/utils'

export function NodePage() {
  const node = useOutletContext<NodeState>()
  const { organization, accountInactive, isAdmin } = useAuth()
  const [addingModel, setAddingModel] = useState(false)
  // While setting up, the download/load progress lives in the sidebar
  // widget only; the checklist takes this space.
  const [settingUp, setSettingUp] = useState(false)
  const [range, setRange] = useState<ChartRange>('1d')
  const [chartStats, setChartStats] = useState<TokenStatsResponse[]>([])
  const [dayStats, setDayStats] = useState<TokenStatsResponse[]>([])

  useEffect(() => {
    let cancelled = false
    // The stat strip always sums the last day; the chart shows the selected
    // range. One request covers both while the chart is on 1d.
    const refresh = async () => {
      try {
        const day = await apiClient.getConsumedTokens('1d')
        const chart = range === '1d' ? day : await apiClient.getConsumedTokens(range)
        if (cancelled) return
        setDayStats(day)
        setChartStats(chart)
      } catch (err) {
        console.error(err)
      }
    }
    refresh()
    // The stat strip and chart reflect real backend usage (Playground
    // requests, or actual served traffic) - poll instead of fetching once,
    // or the chart just freezes at whatever existed when the page mounted.
    const interval = setInterval(refresh, 15000)
    return () => {
      cancelled = true
      clearInterval(interval)
    }
  }, [range])

  const gettingReady =
    node.phase === 'downloading' || node.phase === 'loading' || node.phase === 'connecting' || node.phase === 'stalled'

  const hero = {
    downloading: {
      kicker: 'Stage 1 of 2 · Downloading',
      KickerIcon: DownloadSimple,
      spin: false,
      title: 'Fetching the model for this device.',
      body: `${node.modelName ?? 'The model'}, chosen for this hardware. It downloads once and stays cached.`,
      ringPct: node.downloadProgress,
      ringLabel: `${node.downloadProgress}%`,
      ringCaption: 'downloaded',
      ringColor: 'var(--n-accent)',
    },
    loading: {
      kicker: 'Stage 2 of 2 · Loading',
      KickerIcon: CircleNotch,
      spin: true,
      title: 'Loading the model into memory.',
      body: 'Weights are on disk. Nodea is loading them into memory, once per launch.',
      ringPct: 100,
      ringLabel: node.loadElapsedS === undefined ? '—' : `${node.loadElapsedS}s`,
      ringCaption: 'elapsed',
      ringColor: 'var(--n-accent-700)',
    },
    connecting: {
      kicker: 'Connecting',
      KickerIcon: CircleNotch,
      spin: true,
      title: 'Reaching the coordinator.',
      body: 'Nodea is opening a connection for this device.',
      ringPct: 0,
      ringLabel: '—',
      ringCaption: 'connecting',
      ringColor: 'var(--n-mute-2)',
    },
    stalled: {
      kicker: 'Interrupted',
      KickerIcon: DownloadSimple,
      spin: false,
      title: 'The download stopped.',
      body: node.message ?? 'Retry to start the download again.',
      ringPct: node.downloadProgress,
      ringLabel: `${node.downloadProgress}%`,
      ringCaption: 'downloaded',
      ringColor: 'var(--n-mute-2)',
    },
    offline: {
      kicker: 'Offline',
      KickerIcon: WifiSlash,
      spin: false,
      title: "Can't reach the coordinator.",
      body: 'Nodea keeps retrying on its own. The model stays on this device, so sharing resumes as soon as the connection does.',
      ringPct: 0,
      ringLabel: '—',
      ringCaption: 'offline',
      ringColor: 'var(--n-mute-2)',
    },
    refused: accountInactive
      ? {
          kicker: 'Account deactivated',
          KickerIcon: Prohibit,
          spin: false,
          title: `An admin deactivated your account${organization?.name ? ` in ${organization.name}` : ''}.`,
          body: 'This device stopped sharing compute. Ask an admin to reactivate you, or leave the organization in Settings.',
          ringPct: 0,
          ringLabel: '—',
          ringCaption: 'not sharing',
          ringColor: 'var(--n-mute-2)',
        }
      : {
          kicker: 'Not sharing',
          KickerIcon: Prohibit,
          spin: false,
          title: 'The server refused this account.',
          body: node.message ? `${node.message}. This device stopped sharing compute.` : 'This device stopped sharing compute.',
          ringPct: 0,
          ringLabel: '—',
          ringCaption: 'not sharing',
          ringColor: 'var(--n-mute-2)',
        },
    serving: {
      kicker: null,
      KickerIcon: Broadcast,
      spin: false,
      title: '',
      body: node.modelName
        ? node.loadMs !== undefined
          ? `${node.modelName}, loaded in ${formatDuration(node.loadMs)}.`
          : `${node.modelName} is loaded.`
        : '',
      // No capacity metric exists yet, so the ring shows state, not a number.
      ringPct: 100,
      ringLabel: 'Live',
      ringCaption: 'serving',
      ringColor: 'var(--n-accent)',
    },
    'no-models': {
      kicker: 'Waiting for a model',
      KickerIcon: Cube,
      spin: false,
      title: isAdmin ? 'Add a model to start serving.' : 'Waiting for an admin to add a model.',
      body: isAdmin
        ? `${organization?.name ?? 'Your organization'} has no models yet, so this device has nothing to serve. Pick one and it starts downloading on its own.`
        : 'Your organization has no models yet. This device starts serving on its own once an admin adds one.',
      ringPct: 0,
      ringLabel: '—',
      ringCaption: 'no model',
      ringColor: 'var(--n-mute-2)',
    },
    paused: {
      kicker: null,
      KickerIcon: Broadcast,
      spin: false,
      title: 'Paused. This device is idle.',
      body: node.modelName
        ? `${node.modelName} is still in memory. Turn sharing back on and it picks up within seconds.`
        : '',
      ringPct: 0,
      ringLabel: '—',
      ringCaption: 'paused',
      ringColor: 'var(--n-mute-2)',
    },
  }[node.phase]

  return (
    <div className="px-[30px] pt-[30px]">
      <div className="flex items-start justify-between gap-6">
        <div>
          {hero.kicker && (
            <div className="flex items-center gap-2 text-[13px] font-semibold uppercase leading-none tracking-[0.09em] text-[var(--n-accent-400)]">
              <hero.KickerIcon size={14} className={hero.spin ? 'n-spin' : undefined} />
              {node.phase === 'serving' ? `Serving · ${organization?.name}` : hero.kicker}
            </div>
          )}
          {node.phase === 'serving' ? (
            <div className="mt-3.5 flex items-center gap-2">
              <StatusBadge label="Connected" />
              <StatusBadge label="Serving" />
            </div>
          ) : (
            <h1 className="mt-3.5 max-w-[440px] text-[32px] font-semibold leading-[1.15] tracking-[-0.02em] text-[var(--n-text)] [text-wrap:pretty]">
              {hero.title}
            </h1>
          )}
          {hero.body && (
            <p className="mt-2.5 max-w-[400px] text-[15px] leading-[1.6] text-[var(--n-text-4)]">{hero.body}</p>
          )}
          {node.phase === 'no-models' && isAdmin && (
            <div className="mt-5">
              <PrimaryButton onClick={() => setAddingModel(true)}>Add a model</PrimaryButton>
            </div>
          )}
        </div>
        <CapacityRing pct={hero.ringPct} label={hero.ringLabel} caption={hero.ringCaption} color={hero.ringColor} />
      </div>

      <SetupChecklist node={node} onVisibleChange={setSettingUp} />

      {/* Connecting means nothing is assigned yet: no model, so no checklist for one. */}
      {node.phase === 'no-models' ? null : gettingReady ? (settingUp || node.phase === 'connecting' ? null : <ReadyChecklist node={node} />) : (
        <>
          <StatStrip stats={dayStats} />
          <ThroughputChart stats={chartStats} range={range} onRangeChange={setRange} />
        </>
      )}

      <AddModelsDialog open={addingModel} onOpenChange={setAddingModel}
                       onAdded={() => {
                         setAddingModel(false)
                         announceModelsChanged()
                       }} />
    </div>
  )
}

function StatusBadge({ label }: { label: string }) {
  return (
    <span className="inline-flex items-center gap-1.5 rounded-[6px] border border-[var(--n-accent-800)] px-2 py-1 text-[13px] font-medium leading-none text-[var(--n-accent-300)]">
      <span className="n-pulse h-[7px] w-[7px] rounded-full bg-[var(--n-accent-400)]" />
      {label}
    </span>
  )
}

function CapacityRing({ pct, label, caption, color }: { pct: number; label: string; caption: string; color: string }) {
  const CIRC = 440
  return (
    <div className="relative h-[168px] w-[168px] flex-none">
      <svg width="168" height="168" viewBox="0 0 168 168">
        <circle cx="84" cy="84" r="70" fill="none" stroke="var(--n-line-strong)" strokeWidth="10" />
        <circle cx="84" cy="84" r="70" fill="none" stroke={color} strokeWidth="10" strokeLinecap="round"
                strokeDasharray={CIRC} strokeDashoffset={CIRC - (CIRC * pct) / 100}
                transform="rotate(-90 84 84)"
                style={{ transition: 'stroke-dashoffset .4s ease, stroke .3s ease' }} />
      </svg>
      <div className="absolute inset-0 flex flex-col items-center justify-center gap-[3px]">
        <span className="text-[28px] font-semibold leading-none tracking-[-0.02em] text-[var(--n-text)]">{label}</span>
        <span className="text-[12.5px] font-medium uppercase leading-none tracking-[0.06em] text-[var(--n-text-3)]">{caption}</span>
      </div>
    </div>
  )
}

/**
 * Only "Tokens · 24h" is real (the 1d apiClient.getConsumedTokens() series). The design's
 * Requests / Median latency / Uptime cells have no backend field yet, so
 * they're dropped rather than shown as invented numbers.
 */
function StatStrip({ stats }: { stats: TokenStatsResponse[] }) {
  const lastDay = stats.reduce((s, x) => s + x.value, 0)
  return (
    <div className="mt-[26px] w-fit overflow-hidden rounded-[12px] border border-[var(--n-line)] bg-[var(--n-sidebar)]">
      <div className="px-4 py-3.5">
        <div className="text-[12.5px] font-medium uppercase leading-none tracking-[0.06em] text-[var(--n-text-3)]">Tokens · 24h</div>
        <div className="mt-[7px] text-[23px] font-semibold leading-none text-[var(--n-text)]">
          {new Intl.NumberFormat('en-US').format(lastDay)}
        </div>
      </div>
    </div>
  )
}

function ReadyChecklist({ node }: { node: NodeState }) {
  const dl = node.phase === 'downloading' || node.phase === 'stalled'
  const ld = node.phase === 'loading'
  const onBackend = node.backend ? `On ${node.backend}` : 'Into memory'

  const rows = [
    {
      title: 'Fetching weights',
      sub: dl
        ? downloadSummary(node.downloadedBytes, node.totalBytes, node.bytesPerSec, ' at ') ?? 'Starting download…'
        : 'On disk',
      value: dl ? 'In progress' : 'Done',
      state: dl ? 'busy' : 'done',
    },
    {
      title: 'Loading into memory',
      sub: dl ? 'Waiting for the download' : onBackend,
      value: ld && node.loadElapsedS !== undefined ? `${node.loadElapsedS}s` : '—',
      state: ld ? 'busy' : dl ? 'idle' : 'done',
    },
    {
      title: 'Accepting requests',
      sub: dl || ld ? 'Starts on its own when the model is ready' : 'First request answered moments ago',
      value: dl || ld ? '—' : 'Live',
      state: dl || ld ? 'idle' : 'live',
    },
  ] as const

  return (
    <div className="mt-[30px] overflow-hidden rounded-[12px] border border-[var(--n-line)]">
      {rows.map((r, i) => (
        <div key={r.title}
             className={`flex items-center gap-3 bg-[var(--n-sidebar)] px-[18px] py-3.5 ${i ? 'border-t border-[var(--n-line)]' : ''}`}>
          {r.state === 'done' && <CheckCircle size={17} color="var(--n-accent-400)" />}
          {r.state === 'busy' && <CircleNotch size={17} color="var(--n-accent-400)" className="n-spin" />}
          {r.state === 'idle' && <CircleDashed size={17} color="var(--n-mute)" />}
          {r.state === 'live' && <Broadcast size={17} color="var(--n-accent-400)" />}
          <span className="min-w-0 flex-1">
            <span className="block text-[15px] font-semibold leading-[1.3]"
                  style={{ color: r.state === 'idle' ? 'var(--n-text-4)' : r.state === 'done' ? 'var(--n-text-2)' : 'var(--n-text)' }}>
              {r.title}
            </span>
            <span className="mt-[3px] block text-[13.5px] leading-[1.3] text-[var(--n-text-3)]">{r.sub}</span>
          </span>
          <span className="font-mono text-[13.5px] font-medium leading-none text-[var(--n-text-2)]">{r.value}</span>
        </div>
      ))}
    </div>
  )
}
