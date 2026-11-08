import { openUrl } from '@tauri-apps/plugin-opener'
import { isTauriApp } from '@/lib/platform'

/**
 * Where the desktop app sends people when it hands off to the web app.
 *
 * The console lives under /console - the bare origin is the marketing and
 * auth surface, so linking there bounced signed-in users through a redirect
 * instead of landing them on the thing the link promised.
 */
export const WEB_CONSOLE_URL = 'https://app.getnodea.com/console/'

// The console's detail pages (frontend/src/App.tsx in the web app).
export const consoleModelUrl = (ref: string) =>
  `${WEB_CONSOLE_URL}models/${encodeURIComponent(ref)}`

// A device is addressed by its owner too: two members can register the same
// machine, and each gets its own row.
export const consoleDeviceUrl = (ownerId: string, deviceId: string) =>
  `${WEB_CONSOLE_URL}fleet/${encodeURIComponent(ownerId)}/${encodeURIComponent(deviceId)}`

/* Opens a console URL in the system browser. window.open does nothing in the
 * Tauri webview; the opener plugin does it, within its scope
 * (src-tauri/capabilities/default.json allows app.getnodea.com). */
export function openInBrowser(url: string) {
  if (isTauriApp()) openUrl(url).catch(console.error)
  else window.open(url, '_blank')
}
