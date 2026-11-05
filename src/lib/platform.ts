/**
 * Platform detection utilities
 * Determines if the app is running in Tauri (desktop) or web mode
 */

import { isTauri } from '@tauri-apps/api/core'

// Tauri v2 only defines window.__TAURI__ with withGlobalTauri, which is off,
// so this asks the API package instead.
export const isTauriApp = (): boolean => isTauri()

export const isWebMode = (): boolean => {
  return !isTauriApp()
}

/**
 * Platform-specific configuration
 */
export const getPlatformInfo = () => {
  const tauri = isTauriApp()
  return {
    isTauri: tauri,
    isWeb: !tauri,
    name: tauri ? 'Tauri Desktop' : 'Web Browser',
  }
}
