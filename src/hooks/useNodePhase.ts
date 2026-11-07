import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { isDemoMode, mockInvoke, mockListen, initMockTauri } from '@/lib/mockTauri'
import { useAuth } from '@/contexts/AuthContext'
import { apiClient, type OrgModel } from '@/lib/api'

/* Existing backend shapes — see src-tauri/src/status.rs */
export interface ConnectionStatus {
  type: 'Disconnected' | 'Connecting' | 'Connected' | 'Error' | 'DeviceLimitReached' | 'Refused'
  message?: string
}
export interface ModelStatus {
  type: 'NoModel' | 'Downloading' | 'Loading' | 'Ready' | 'Error'
  model_name?: string
  progress?: number
  message?: string
  /** Downloading: 0 until the response gives a size */
  downloaded_bytes?: number
  total_bytes?: number
  bytes_per_sec?: number
  /** Loading: Unix ms */
  started_at_ms?: number
  /** Ready: null when nothing was loaded (already resident) */
  load_ms?: number | null
}
interface StatusSnapshot {
  connection: ConnectionStatus
  model: ModelStatus
}

/* The phases the UI renders. Derived, never stored.
 *
 * `stalled` is the model failing (download or load). Connection problems are
 * their own phases: `offline` (can't reach the coordinator; retries on its
 * own) and `refused` (the server refused this account - deleted or
 * deactivated). Mapping every connection Error to `stalled` is how a leave in
 * the web console read as "The download stopped" (#241). */
export type NodePhase =
  | 'connecting'
  | 'downloading'
  | 'loading'
  | 'serving'
  | 'paused'
  | 'stalled'
  | 'offline'
  | 'refused'
  | 'no-models'

export interface NodeState {
  phase: NodePhase
  /** The model the backend assigned; undefined until a LoadModel names one */
  modelName?: string
  /** 0-100, only meaningful while downloading */
  downloadProgress: number
  /** Downloading only; 0 until known */
  downloadedBytes: number
  totalBytes: number
  bytesPerSec: number
  /** Whole seconds since loading started; undefined unless loading */
  loadElapsedS?: number
  /** How long the last load took; undefined when not timed */
  loadMs?: number
  /** What llama.cpp runs on: 'Metal' | 'Vulkan' | 'CPU'; undefined until known */
  backend?: string
  message?: string
  sharing: boolean
  setSharing: (on: boolean) => void
  retry: () => void
  /** Null while unknown; see derivePhase. */
  orgHasModels: boolean | null
  /** An enabled model of the organization, for the API examples. */
  orgModelRef: string | null
}

const SHARING_KEY = 'nodea.sharing'

/* The organization the current connection was opened for. The backend binds
 * a node's session - and its device registration - to the organization at
 * connect time, and leaving an organization deletes that registration. So
 * joining or creating another one needs a fresh connection, even when the
 * old one still looks connected from here. Module-level so it survives the
 * layout unmounting during onboarding. */
let sessionOrganization: string | null = null

/* `orgHasModels` is false only once the organization's model list has been
 * read and has nothing enabled: the backend has nothing to send this node, so
 * "Connecting" would spin forever. Null while unknown. */
export function derivePhase(
  connection: ConnectionStatus | undefined,
  model: ModelStatus | undefined,
  sharing: boolean,
  accountInactive: boolean,
  orgHasModels: boolean | null = null
): NodePhase {
  if (accountInactive || connection?.type === 'Refused') return 'refused'
  if (model?.type === 'Error') return 'stalled'
  if (connection?.type === 'Error') return 'offline'
  if (model?.type === 'Downloading') return 'downloading'
  if (model?.type === 'Loading') return 'loading'
  if (model?.type === 'Ready' && connection?.type === 'Connected') return sharing ? 'serving' : 'paused'
  if (!sharing) return 'paused'
  if (orgHasModels === false) return 'no-models'
  return 'connecting'
}

export function useNodePhase(): NodeState {
  const demo = isDemoMode()
  const { accountInactive, organization } = useAuth()
  const organizationKey = organization ? (organization.uuid ?? String(organization.id)) : null
  const orgModels = useOrgModels(organization !== null)
  const enabledModel = orgModels?.find((m) => m.enabled) ?? null
  const orgHasModels = orgModels === null ? null : enabledModel !== null
  const [snapshot, setSnapshot] = useState<StatusSnapshot | null>(null)
  const [backend, setBackend] = useState<string>()
  const [now, setNow] = useState(() => Date.now())
  const [sharing, setSharingState] = useState(() => {
    if (demo) {
      // ?phase=paused|serving (see mockTauri.ts) pins sharing before the
      // mock lifecycle even starts - reading localStorage here would be too
      // late, since the mock only updates it from an effect after mount.
      const forcedPhase = new URLSearchParams(window.location.search).get('phase')
      if (forcedPhase === 'paused') return false
      if (forcedPhase === 'serving') return true
    }
    return localStorage.getItem(SHARING_KEY) !== 'false'
  })

  useEffect(() => {
    if (demo) initMockTauri()
    const inv = demo ? mockInvoke : invoke
    const lis = demo ? mockListen : listen
    let cancelled = false

    inv<StatusSnapshot>('get_all_status')
      .then((s) => {
        if (cancelled) return
        setSnapshot(s)
      })
      .catch(console.error)

    inv<string>('get_inference_backend')
      .then((b) => {
        if (!cancelled) setBackend(b)
      })
      .catch(console.error)

    const unlisten = [
      lis<ConnectionStatus>('connection-status-changed', (e) =>
        setSnapshot((p) => (p ? { ...p, connection: e.payload } : p))
      ),
      lis<ModelStatus>('model-status-changed', (e) =>
        setSnapshot((p) => (p ? { ...p, model: e.payload } : p))
      ),
      // The tray's "Pause/Resume sharing" item (src-tauri/src/main.rs) calls
      // connect_grpc/disconnect_grpc directly and emits this so the app's own
      // Sharing toggle - a local preference this hook doesn't otherwise
      // re-derive from connection state - doesn't go stale.
      lis<boolean>('sharing-changed', (e) => {
        setSharingState(e.payload)
        localStorage.setItem(SHARING_KEY, String(e.payload))
      }),
    ]

    return () => {
      cancelled = true
      unlisten.forEach((p) => p.then((off) => off()))
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [demo])

  // Connect once the status is known, and reconnect when the organization
  // changes under an open connection (see sessionOrganization).
  const statusKnown = snapshot !== null
  useEffect(() => {
    if (!statusKnown || !organizationKey || !sharing || accountInactive) return
    const inv = demo ? mockInvoke : invoke
    const disconnected = snapshot?.connection.type === 'Disconnected'
    if (!disconnected && sessionOrganization === organizationKey) return

    const previous = sessionOrganization
    sessionOrganization = organizationKey
    const reconnect = disconnected
      ? inv('connect_grpc')
      : inv('disconnect_grpc').then(() => inv('connect_grpc'))
    reconnect.catch((err) => {
      sessionOrganization = previous
      console.error(err)
    })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [statusKnown, organizationKey, accountInactive])

  const setSharing = (on: boolean) => {
    setSharingState(on)
    if (on && organizationKey) sessionOrganization = organizationKey
    localStorage.setItem(SHARING_KEY, String(on))
    const inv = demo ? mockInvoke : invoke
    inv(on ? 'connect_grpc' : 'disconnect_grpc').catch(console.error)
  }

  const retry = () => {
    if (organizationKey) sessionOrganization = organizationKey
    const inv = demo ? mockInvoke : invoke
    inv('connect_grpc').catch(console.error)
  }

  const connection = snapshot?.connection
  const model = snapshot?.model

  const phase = derivePhase(connection, model, sharing, accountInactive, orgHasModels)

  // Tick once a second while loading, for the elapsed counter.
  const loadStartedAt = model?.type === 'Loading' ? model.started_at_ms : undefined
  useEffect(() => {
    if (loadStartedAt === undefined) return
    setNow(Date.now())
    const id = setInterval(() => setNow(Date.now()), 1000)
    return () => clearInterval(id)
  }, [loadStartedAt])

  return {
    phase,
    modelName: model?.model_name,
    downloadProgress: Math.round(model?.progress ?? 0),
    downloadedBytes: model?.downloaded_bytes ?? 0,
    totalBytes: model?.total_bytes ?? 0,
    bytesPerSec: model?.bytes_per_sec ?? 0,
    loadElapsedS:
      loadStartedAt === undefined ? undefined : Math.max(0, Math.floor((now - loadStartedAt) / 1000)),
    loadMs: model?.load_ms ?? undefined,
    backend,
    // The message that explains the phase: the model's for a model failure,
    // the connection's for an offline or refused node.
    message: phase === 'stalled' ? model?.message : connection?.message ?? model?.message,
    sharing,
    setSharing,
    retry,
    orgHasModels,
    orgModelRef: enabledModel?.ref ?? null,
  }
}

const ORG_MODELS_POLL_MS = 15000

/* The organization's models, re-read every 15s, when the
 * window regains focus, and when the app itself adds one - so a member
 * waiting on an admin moves on by itself. */
function useOrgModels(inOrganization: boolean): OrgModel[] | null {
  const [models, setModels] = useState<OrgModel[] | null>(null)

  useEffect(() => {
    if (!inOrganization) {
      setModels(null)
      return
    }
    let stopped = false
    const check = () => {
      apiClient.getOrgModels()
        .then((list) => {
          if (!stopped) setModels(list)
        })
        .catch(() => {
          // Unknown rather than "none": a failed read must not claim the
          // organization is empty.
          if (!stopped) setModels(null)
        })
    }
    check()
    const interval = setInterval(check, ORG_MODELS_POLL_MS)
    window.addEventListener('focus', check)
    window.addEventListener(MODELS_CHANGED, check)
    return () => {
      stopped = true
      clearInterval(interval)
      window.removeEventListener('focus', check)
      window.removeEventListener(MODELS_CHANGED, check)
    }
  }, [inOrganization])

  return models
}

const MODELS_CHANGED = 'nodea:models-changed'

/* Call after adding, enabling, disabling or removing a model, so the node
 * state doesn't wait for the next poll. */
export function announceModelsChanged() {
  window.dispatchEvent(new Event(MODELS_CHANGED))
}
