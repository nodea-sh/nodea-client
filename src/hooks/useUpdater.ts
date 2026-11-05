import { useCallback, useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { openUrl } from '@tauri-apps/plugin-opener'
import { toast } from 'sonner'
import { isDemoMode, mockInvoke, mockListen } from '@/lib/mockTauri'
import { isTauriApp } from '@/lib/platform'
import { version as appVersion } from '../../package.json'

/* Mirrors UpdateState and CheckOutcome in src-tauri/src/updater.rs. Rust owns
 * the update logic (check, download, install on restart or quit); this hook
 * only shows its state and sends the user's actions. */
export type UpdateState =
  | { state: 'idle' }
  | { state: 'checking' }
  | { state: 'downloading'; version: string; progress: number | null }
  | { state: 'ready'; version: string }
  | { state: 'installing'; version: string; finishing_requests: boolean }

export type CheckOutcome =
  | { outcome: 'up_to_date' }
  | { outcome: 'ready'; version: string }
  | { outcome: 'busy' }
  | { outcome: 'failed'; error: string }

const LAST_VERSION_KEY = 'nodea.lastVersion'

const releaseUrl = (version: string) =>
  `https://github.com/nodea-sh/nodea-client/releases/tag/v${version}`

function openReleaseNotes(version: string) {
  if (isTauriApp()) openUrl(releaseUrl(version)).catch(console.error)
  else window.open(releaseUrl(version), '_blank')
}

/** The answer to a manual check, from the sidebar or the tray. */
export function showCheckOutcome(outcome: CheckOutcome) {
  switch (outcome.outcome) {
    case 'up_to_date':
      toast("You're up to date", { description: `Nodea ${appVersion}` })
      break
    case 'ready':
      toast('Update ready', { description: `Nodea ${outcome.version} installs when you restart.` })
      break
    case 'busy':
      toast('Already checking for updates')
      break
    case 'failed':
      toast.error("Couldn't check for updates", { description: outcome.error })
      break
  }
}

let announced = false

/** Once per version: the first launch after an update says so. Guarded per
 * page load because StrictMode runs effects twice, and the second run would
 * find the new version already stored. */
function announceUpdate() {
  if (announced) return
  announced = true
  let previous: string | null = null
  try {
    previous = localStorage.getItem(LAST_VERSION_KEY)
    localStorage.setItem(LAST_VERSION_KEY, appVersion)
  } catch {
    return
  }
  if (previous && previous !== appVersion) {
    // Next tick: this runs in the sidebar's effect, before the app-level
    // <Toaster> has subscribed, and sonner drops toasts nobody listens for.
    setTimeout(() =>
      toast(`Updated to ${appVersion}`, {
        action: { label: "What's new", onClick: () => openReleaseNotes(appVersion) },
      }),
    )
  }
}

export interface Updater {
  update: UpdateState
  /** The user hid the Ready item. It comes back on the next launch. */
  dismissed: boolean
  dismiss: () => void
  check: () => void
  restart: () => void
}

export function useUpdater(): Updater {
  const demo = isDemoMode()
  const [update, setUpdate] = useState<UpdateState>({ state: 'idle' })
  const [dismissed, setDismissed] = useState(false)

  useEffect(() => {
    announceUpdate()
  }, [])

  useEffect(() => {
    const inv = demo ? mockInvoke : invoke
    const lis = demo ? mockListen : listen
    let cancelled = false
    const unlisteners: Array<() => void> = []
    const keep = (unlisten: () => void) => (cancelled ? unlisten() : unlisteners.push(unlisten))

    inv<UpdateState>('get_update_state')
      .then((state) => !cancelled && setUpdate(state))
      .catch(console.error)
    lis<UpdateState>('update://state', (event) => setUpdate(event.payload)).then(keep)
    lis<CheckOutcome>('update://check-result', (event) => showCheckOutcome(event.payload)).then(keep)

    return () => {
      cancelled = true
      unlisteners.forEach((unlisten) => unlisten())
    }
  }, [demo])

  const check = useCallback(() => {
    const inv = demo ? mockInvoke : invoke
    inv<CheckOutcome>('check_for_updates')
      .then(showCheckOutcome)
      .catch((error) => showCheckOutcome({ outcome: 'failed', error: String(error) }))
  }, [demo])

  const restart = useCallback(() => {
    const inv = demo ? mockInvoke : invoke
    // Relaunches on success, so only a failure comes back.
    inv('restart_to_update').catch((error) =>
      toast.error("Couldn't install the update", { description: String(error) }),
    )
  }, [demo])

  return { update, dismissed, dismiss: () => setDismissed(true), check, restart }
}
