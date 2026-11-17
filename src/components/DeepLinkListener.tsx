import { useEffect } from 'react'
import { useNavigate } from 'react-router-dom'
import { listen } from '@tauri-apps/api/event'
import { isTauriApp } from '@/lib/platform'

/**
 * Bridges nodea:// deep links (opened from emails) into the React Router.
 * The Rust side (src-tauri/src/main.rs) registers the OS-level scheme
 * handler and forwards incoming links here via the
 * "scheme-request-received" event.
 *
 * The backend builds the password reset links this app asks for from
 * DESKTOP_FRONTEND_BASE_URL, "nodea://" in production, so
 * `${DESKTOP_FRONTEND_BASE_URL}/reset-password/${token}` becomes
 * "nodea:///reset-password/<token>" - an empty host with the app route as
 * the URL's pathname, which is what we navigate to below. Its other links
 * (FRONTEND_BASE_URL) open the web console.
 */
export function DeepLinkListener() {
  const navigate = useNavigate()

  useEffect(() => {
    // A plain browser has no Tauri event bus to listen on.
    if (!isTauriApp()) return

    const unlistenPromise = listen<string>('scheme-request-received', (event) => {
      try {
        const path = new URL(event.payload).pathname
        navigate(path)
      } catch (err) {
        console.error('Failed to parse deep link URL:', event.payload, err)
      }
    })

    return () => {
      unlistenPromise.then((unlisten) => unlisten())
    }
  }, [navigate])

  return null
}
