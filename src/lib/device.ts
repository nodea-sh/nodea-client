/**
 * The word the UI uses for the machine Nodea runs on (Mac, PC or
 * computer), so copy reads right on every platform.
 *
 * The OS comes from the webview's user agent: each Tauri webview (WKWebView,
 * WebView2, WebKitGTK) reports its host OS there, synchronously, without an
 * extra plugin. In a plain browser (demo mode, tests) `?os=<macos|windows|linux>`
 * overrides it for the session.
 */

export type DeviceOs = 'macos' | 'windows' | 'linux' | 'unknown'

const OS_OVERRIDE_KEY = 'nodea.os'

const isDeviceOs = (value: string | null): value is DeviceOs =>
  value === 'macos' || value === 'windows' || value === 'linux' || value === 'unknown'

export const osFromUserAgent = (userAgent: string): DeviceOs => {
  // iPhone/iPad user agents also say "Mac OS X"; Nodea does not run there.
  if (/Macintosh|Mac OS X/i.test(userAgent) && !/iPhone|iPad/i.test(userAgent)) return 'macos'
  if (/Windows/i.test(userAgent)) return 'windows'
  if (/Linux|X11/i.test(userAgent) && !/Android/i.test(userAgent)) return 'linux'
  return 'unknown'
}

const osOverride = (): DeviceOs | null => {
  try {
    const fromQuery = new URLSearchParams(window.location.search).get('os')
    if (isDeviceOs(fromQuery)) {
      sessionStorage.setItem(OS_OVERRIDE_KEY, fromQuery)
      return fromQuery
    }
    const stored = sessionStorage.getItem(OS_OVERRIDE_KEY)
    return isDeviceOs(stored) ? stored : null
  } catch {
    return null
  }
}

export const currentOs = (): DeviceOs => {
  if (typeof window === 'undefined') return 'unknown'
  return osOverride() ?? osFromUserAgent(window.navigator.userAgent)
}

const NOUNS: Record<DeviceOs, string> = {
  macos: 'Mac',
  windows: 'PC',
  linux: 'computer',
  unknown: 'computer',
}

export const deviceNoun = (os: DeviceOs = currentOs()): string => NOUNS[os]

/** "this <noun>", for use mid-sentence. */
export const thisDevice = (os?: DeviceOs): string => `this ${deviceNoun(os)}`

/** "This <noun>", for the start of a sentence. */
export const thisDeviceCapitalized = (os?: DeviceOs): string => `This ${deviceNoun(os)}`
