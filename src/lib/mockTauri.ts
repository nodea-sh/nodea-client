// Mock Tauri API for demo mode
// This simulates the status indicator behavior without a real backend

interface ConnectionStatus {
  type: 'Disconnected' | 'Connecting' | 'Connected' | 'Error' | 'Refused'
  message?: string
}

interface ModelStatus {
  type: 'NoModel' | 'Downloading' | 'Loading' | 'Ready' | 'Error'
  model_name?: string
  progress?: number
  message?: string
  downloaded_bytes?: number
  total_bytes?: number
  bytes_per_sec?: number
  started_at_ms?: number
  load_ms?: number | null
}

export interface StatusSnapshot {
  connection: ConnectionStatus
  model: ModelStatus
}

// Event listener management
type EventCallback = (event: { payload: any }) => void
const eventListeners = new Map<string, Set<EventCallback>>()

// Current mock state
let currentConnectionStatus: ConnectionStatus = { type: 'Disconnected' }
let currentModelStatus: ModelStatus = { type: 'NoModel' }

// Helper to emit events
const emitEvent = (eventName: string, payload: any) => {
  const listeners = eventListeners.get(eventName)
  if (listeners) {
    listeners.forEach(callback => {
      callback({ payload })
    })
  }
}

// Simulate connection lifecycle
const simulateConnectionLifecycle = () => {
  // Start disconnected
  currentConnectionStatus = { type: 'Disconnected' }
  emitEvent('connection-status-changed', currentConnectionStatus)

  // After 2 seconds, start connecting
  setTimeout(() => {
    currentConnectionStatus = { type: 'Connecting' }
    emitEvent('connection-status-changed', currentConnectionStatus)

    // After 3 seconds, connect successfully
    setTimeout(() => {
      currentConnectionStatus = { type: 'Connected' }
      emitEvent('connection-status-changed', currentConnectionStatus)

      // Start model lifecycle AFTER connection is established
      // This simulates the real behavior where models only load when connected
      setTimeout(() => {
        simulateModelLifecycle()
      }, 2000)

      // Optionally simulate an error after 30 seconds
      setTimeout(() => {
        currentConnectionStatus = {
          type: 'Error',
          message: 'Demo: Simulated connection error'
        }
        emitEvent('connection-status-changed', currentConnectionStatus)

        // Reconnect after 5 seconds
        setTimeout(() => {
          currentConnectionStatus = { type: 'Connecting' }
          emitEvent('connection-status-changed', currentConnectionStatus)

          setTimeout(() => {
            currentConnectionStatus = { type: 'Connected' }
            emitEvent('connection-status-changed', currentConnectionStatus)
          }, 2000)
        }, 5000)
      }, 30000)
    }, 3000)
  }, 2000)
}

// Simulate model download and load
const simulateModelLifecycle = () => {
  // Start with no model
  currentModelStatus = { type: 'NoModel' }
  emitEvent('model-status-changed', currentModelStatus)

  // After 5 seconds, start downloading
  setTimeout(() => {
    const modelName = 'llama-3.2-1b'
    const totalBytes = Math.round(0.8 * 1024 ** 3)
    const downloading = (progress: number) => ({
      type: 'Downloading' as const,
      model_name: modelName,
      progress,
      downloaded_bytes: Math.round((totalBytes * progress) / 100),
      total_bytes: totalBytes,
      bytes_per_sec: progress > 0 ? Math.round(totalBytes * 0.12) : 0, // ~12%/s, the simulated pace
    })
    currentModelStatus = downloading(0)
    emitEvent('model-status-changed', currentModelStatus)

    // Simulate incremental download progress
    let progress = 0
    const progressInterval = setInterval(() => {
      progress += Math.floor(Math.random() * 15) + 5 // 5-20% increment
      if (progress >= 100) {
        progress = 100
        clearInterval(progressInterval)

        // Download complete, emit final 100%
        currentModelStatus = downloading(100)
        emitEvent('model-status-changed', currentModelStatus)

        // Start loading after download
        setTimeout(() => {
          currentModelStatus = {
            type: 'Loading',
            model_name: modelName,
            started_at_ms: Date.now()
          }
          emitEvent('model-status-changed', currentModelStatus)

          // Model ready after loading
          setTimeout(() => {
            currentModelStatus = {
              type: 'Ready',
              model_name: modelName,
              load_ms: 3000
            }
            emitEvent('model-status-changed', currentModelStatus)
          }, 3000)
        }, 500)
      } else {
        currentModelStatus = downloading(progress)
        emitEvent('model-status-changed', currentModelStatus)
      }
    }, 1000) // Update every second
  }, 5000)
}

// Mock Tauri invoke
export const mockInvoke = async <T>(command: string, args?: any): Promise<T> => {
  console.log(`[Mock Tauri] Invoking command: ${command}`, args)

  switch (command) {
    case 'get_all_status':
      return {
        connection: currentConnectionStatus,
        model: currentModelStatus
      } as T

    case 'connect_grpc':
      // Don't restart lifecycle if already started, or if a phase is pinned via ?phase=
      if (currentConnectionStatus.type === 'Disconnected' && !getForcedPhase()) {
        simulateConnectionLifecycle()
      }
      return Promise.resolve() as T

    case 'disconnect_grpc':
      return Promise.resolve() as T

    case 'load_model_auto':
      // Don't restart lifecycle if already started
      if (currentModelStatus.type === 'NoModel') {
        simulateModelLifecycle()
      }
      return Promise.resolve('Model loading started' as any) as T

    case 'get_inference_backend':
      return 'Metal' as unknown as T

    case 'get_device_id':
      return 'device-001' as unknown as T

    case 'reveal_model_cache':
      console.log('[Mock Tauri] reveal_model_cache')
      return undefined as unknown as T

    case 'get_update_state':
      return getForcedUpdate() as unknown as T

    case 'check_for_updates': {
      const update = getForcedUpdate()
      return (update.state === 'ready'
        ? { outcome: 'ready', version: update.version }
        : { outcome: 'up_to_date' }) as unknown as T
    }

    case 'restart_to_update':
      console.log('[Mock Tauri] restart_to_update')
      return undefined as unknown as T

    case 'get_model_cache_info':
      return {
        path: '~/Library/Application Support/Nodea/models',
        size_bytes: Math.round(4.9 * 1024 ** 3),
      } as unknown as T

    default:
      console.warn(`[Mock Tauri] Unknown command: ${command}`)
      return Promise.reject(`Unknown command: ${command}`)
  }
}

// Mock Tauri listen
export const mockListen = async <T>(
  eventName: string,
  callback: (event: { payload: T }) => void
): Promise<() => void> => {
  console.log(`[Mock Tauri] Listening to event: ${eventName}`)

  if (!eventListeners.has(eventName)) {
    eventListeners.set(eventName, new Set())
  }

  const listeners = eventListeners.get(eventName)!
  listeners.add(callback as EventCallback)

  // Return unlisten function
  return () => {
    listeners.delete(callback as EventCallback)
    console.log(`[Mock Tauri] Stopped listening to event: ${eventName}`)
  }
}

/**
 * Force a specific node phase via ?phase=<name> (with ?demo=true), so each
 * visual state (connecting/downloading/loading/serving/paused/stalled) can be
 * reached directly instead of waiting through the timed simulation. Used by
 * tests/node.spec.ts.
 */
const FORCED_MODEL_NAME = 'Llama 3.1 8B Instruct'
// Figures the forced phases report; tests/node.spec.ts asserts their rendering.
const FORCED_TOTAL_BYTES = 4 * 1024 ** 3
const FORCED_BYTES_PER_SEC = 32 * 1024 ** 2
const FORCED_LOAD_MS = 12_400

const getForcedPhase = (): string | null => {
  return new URLSearchParams(window.location.search).get('phase')
}

/**
 * The updater's state via ?update=<downloading|ready|installing> (with
 * ?demo=true); idle otherwise. Used by tests/updates.spec.ts.
 */
const MOCK_UPDATE_VERSION = '26.11.3'

const getForcedUpdate = () => {
  switch (new URLSearchParams(window.location.search).get('update')) {
    case 'downloading':
      return { state: 'downloading', version: MOCK_UPDATE_VERSION, progress: 40 }
    case 'ready':
      return { state: 'ready', version: MOCK_UPDATE_VERSION }
    case 'installing':
      return { state: 'installing', version: MOCK_UPDATE_VERSION, finishing_requests: true }
    default:
      return { state: 'idle' }
  }
}

const applyForcedPhase = (phase: string) => {
  switch (phase) {
    case 'connecting':
      currentConnectionStatus = { type: 'Connecting' }
      currentModelStatus = { type: 'NoModel' }
      break
    case 'downloading':
      currentConnectionStatus = { type: 'Connected' }
      currentModelStatus = {
        type: 'Downloading',
        model_name: FORCED_MODEL_NAME,
        progress: 62,
        downloaded_bytes: Math.round(FORCED_TOTAL_BYTES * 0.62),
        total_bytes: FORCED_TOTAL_BYTES,
        bytes_per_sec: FORCED_BYTES_PER_SEC,
      }
      break
    case 'loading':
      currentConnectionStatus = { type: 'Connected' }
      currentModelStatus = { type: 'Loading', model_name: FORCED_MODEL_NAME, started_at_ms: Date.now() - 7_000 }
      break
    case 'serving':
      currentConnectionStatus = { type: 'Connected' }
      currentModelStatus = { type: 'Ready', model_name: FORCED_MODEL_NAME, load_ms: FORCED_LOAD_MS }
      localStorage.setItem('nodea.sharing', 'true')
      break
    case 'paused':
      currentConnectionStatus = { type: 'Connected' }
      currentModelStatus = { type: 'Ready', model_name: FORCED_MODEL_NAME, load_ms: FORCED_LOAD_MS }
      localStorage.setItem('nodea.sharing', 'false')
      break
    case 'stalled':
      currentConnectionStatus = { type: 'Connected' }
      currentModelStatus = { type: 'Error', model_name: FORCED_MODEL_NAME, message: 'Demo: the download failed' }
      break
    case 'offline':
      // The model is fine; only the coordinator is out of reach.
      currentConnectionStatus = { type: 'Error', message: 'Demo: no connection' }
      currentModelStatus = { type: 'Ready', model_name: FORCED_MODEL_NAME, load_ms: FORCED_LOAD_MS }
      break
    case 'refused':
      currentConnectionStatus = { type: 'Refused', message: 'Account is inactive' }
      currentModelStatus = { type: 'Ready', model_name: FORCED_MODEL_NAME, load_ms: FORCED_LOAD_MS }
      break
    default:
      console.warn(`[Mock Tauri] Unknown forced phase: ${phase}`)
      return
  }
  emitEvent('connection-status-changed', currentConnectionStatus)
  emitEvent('model-status-changed', currentModelStatus)
}

// Auto-start simulation when demo mode is enabled
export const initMockTauri = () => {
  const forced = getForcedPhase()
  if (forced) {
    console.log(`[Mock Tauri] Pinning phase: ${forced}`)
    applyForcedPhase(forced)
    return
  }

  console.log('[Mock Tauri] Initializing demo mode simulations')

  // Start connection lifecycle which will trigger model lifecycle when connected
  setTimeout(() => {
    simulateConnectionLifecycle()
  }, 1000)
}

// Check if demo mode is enabled
export const isDemoMode = () => {
  const urlParams = new URLSearchParams(window.location.search)
  const demoParam = urlParams.get('demo')
  const storedDemo = localStorage.getItem('demoMode')

  if (demoParam !== null) {
    const enabled = demoParam === 'true' || demoParam === '1'
    localStorage.setItem('demoMode', enabled.toString())
    return enabled
  }

  return storedDemo === 'true'
}
