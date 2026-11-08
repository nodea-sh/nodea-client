import { useEffect, useState } from 'react'
import { useOutletContext } from 'react-router-dom'
import { invoke } from '@tauri-apps/api/core'
import { useAuth } from '@/contexts/AuthContext'
import { AccountSection } from '@/components/account/AccountSection'
import { apiClient } from '@/lib/api'
import { isDemoMode, mockInvoke } from '@/lib/mockTauri'
import { isTauriApp } from '@/lib/platform'
import type { NodeState } from '@/hooks/useNodePhase'
import { version as appVersion } from '../../package.json'

interface ModelCacheInfo {
  path: string
  size_bytes: number
}

function formatGB(bytes: number): string {
  return `${(bytes / 1024 ** 3).toFixed(1)} GB`
}

export function SettingsPage() {
  const node = useOutletContext<NodeState>()
  const { logout } = useAuth()
  const [cache, setCache] = useState<ModelCacheInfo | null>(null)

  useEffect(() => {
    const demo = isDemoMode()
    ;(demo ? mockInvoke : invoke)<ModelCacheInfo>('get_model_cache_info')
      .then(setCache)
      .catch(console.error)
  }, [])

  const revealCache = () => {
    if (!cache) return
    if (isDemoMode()) {
      mockInvoke('reveal_model_cache').catch(console.error)
    } else if (isTauriApp()) {
      invoke('reveal_model_cache').catch(console.error)
    }
  }

  return (
    <div className="p-[30px]">
      <h1 className="mb-6 text-[26px] font-semibold leading-[1.2] tracking-[-0.02em] text-[var(--n-text)]">
        Settings
      </h1>

      <div className="flex flex-col gap-0.5">
        <NodeNameRow />
        <Row title="Share compute" description="Whenever this device is awake and plugged in."
             control={<Switch on={node.sharing} onChange={node.setSharing} />} />
        <Row last title="Model cache"
             description={cache ? `${formatGB(cache.size_bytes)} in ${cache.path}` : 'Calculating…'}
             control={<GhostButton onClick={revealCache} disabled={!cache}>Reveal</GhostButton>} />
      </div>

      <AccountSection />

      <div className="mt-[26px] flex items-center justify-between border-t border-[var(--n-line)] pt-[18px]">
        <span className="text-[13.5px] leading-[1.5] text-[var(--n-text-4)]">Nodea {appVersion}</span>
        <GhostButton onClick={logout}>Sign out</GhostButton>
      </div>
    </div>
  )
}

const MAX_NODE_NAME_LENGTH = 64

type NodeName =
  | { state: 'loading' }
  // The backend creates a device's record on its first connection, so a
  // device that never connected has no name to change yet.
  | { state: 'unknown' }
  | { state: 'failed' }
  | { state: 'ready'; deviceId: string; name: string }

const NODE_NAME_STATUS = {
  loading: 'Loading…',
  unknown: 'Connect once to name this node.',
  failed: "Couldn't load this node's name.",
} as const

function NodeNameRow() {
  const [node, setNode] = useState<NodeName>({ state: 'loading' })
  // null while not editing.
  const [draft, setDraft] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    const demo = isDemoMode()
    ;(demo ? mockInvoke : invoke)<string>('get_device_id')
      .then(async (deviceId) => {
        const device = await apiClient.getDevice(deviceId)
        setNode({ state: 'ready', deviceId, name: device.name })
      })
      .catch((err) => {
        if ((err as { status?: number }).status === 404) {
          setNode({ state: 'unknown' })
        } else {
          console.error(err)
          setNode({ state: 'failed' })
        }
      })
  }, [])

  const startEditing = () => {
    if (node.state !== 'ready') return
    setDraft(node.name)
    setError(null)
  }

  const cancel = () => {
    setDraft(null)
    setError(null)
  }

  const save = async (e: React.FormEvent) => {
    e.preventDefault()
    if (node.state !== 'ready' || draft === null) return
    const name = draft.trim()
    if (!name) {
      setError('Enter a name.')
      return
    }
    if (name === node.name) {
      cancel()
      return
    }
    setBusy(true)
    setError(null)
    try {
      const device = await apiClient.updateDevice(node.deviceId, { name })
      setNode({ ...node, name: device.name })
      setDraft(null)
    } catch (err) {
      setError(err instanceof Error ? err.message : "Couldn't rename this node.")
    } finally {
      setBusy(false)
    }
  }

  if (node.state !== 'ready') {
    return <Row first title="Node name" description={NODE_NAME_STATUS[node.state]} control={null} />
  }

  if (draft === null) {
    return <Row first title="Node name" description={node.name}
                control={<GhostButton onClick={startEditing}>Rename</GhostButton>} />
  }

  return (
    <form onSubmit={save} onKeyDown={(e) => e.key === 'Escape' && cancel()}>
      <Row first title="Node name"
           description={
             <>
               <input value={draft} autoFocus aria-label="Node name" maxLength={MAX_NODE_NAME_LENGTH}
                      disabled={busy} onChange={(e) => setDraft(e.target.value)}
                      className="w-full min-w-[240px] rounded-[8px] border border-[var(--n-mute-4)] bg-transparent px-2.5 py-1.5 text-[14px] text-[var(--n-text)] outline-none focus:border-[var(--n-mute-2)]" />
               {error && <span role="alert" className="mt-1.5 block text-[13.5px] text-[var(--n-text-2)]">{error}</span>}
             </>
           }
           control={
             <span className="flex flex-none gap-2">
               <GhostButton onClick={cancel} disabled={busy}>Cancel</GhostButton>
               <GhostButton type="submit" disabled={busy}>Save</GhostButton>
             </span>
           } />
    </form>
  )
}

function Row({
  title, description, control, first, last,
}: {
  title: string; description: React.ReactNode; control: React.ReactNode; first?: boolean; last?: boolean
}) {
  const radius = first ? '10px 10px 2px 2px' : last ? '2px 2px 10px 10px' : '2px'
  return (
    <div className="flex items-center justify-between gap-5 border border-[var(--n-line)] bg-[var(--n-sidebar)] px-[18px] py-4"
         style={{ borderRadius: radius }}>
      <span>
        <span className="block text-[15px] font-semibold leading-[1.3] text-[var(--n-text)]">{title}</span>
        <span className="mt-1 block text-[14px] leading-[1.5] text-[var(--n-text-3)]">{description}</span>
      </span>
      {control}
    </div>
  )
}

function Switch({ on, onChange }: { on: boolean; onChange: (v: boolean) => void }) {
  return (
    <button role="switch" aria-checked={on} onClick={() => onChange(!on)}
            className="relative inline-block h-5 w-9 flex-none rounded-[10px] transition-colors duration-[180ms]"
            style={{ background: on ? 'var(--n-accent-700)' : 'var(--n-border)' }}>
      <span className="absolute top-0.5 h-4 w-4 rounded-full transition-[left,background] duration-[180ms]"
            style={{ left: on ? 18 : 2, background: on ? 'var(--n-accent-200)' : 'var(--n-text-4)' }} />
    </button>
  )
}

function GhostButton({ children, onClick, disabled, type = 'button' }: {
  children: React.ReactNode; onClick?: () => void; disabled?: boolean; type?: 'button' | 'submit'
}) {
  return (
    <button type={type} onClick={onClick} disabled={disabled}
      className="flex-none rounded-[8px] border border-[var(--n-mute-4)] px-[13px] py-[7px] text-[14px] font-semibold leading-none text-[var(--n-text-4)] transition-colors hover:border-[var(--n-mute-2)] hover:text-[var(--n-text-2)]">
      {children}
    </button>
  )
}
