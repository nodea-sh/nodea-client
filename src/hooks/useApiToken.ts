import { useSyncExternalStore } from 'react'
import { apiClient } from '@/lib/api'

// The API client's current access token. A stream puts the token in its URL,
// so an effect that opens one lists this in its dependencies to reopen the
// stream after the token is refreshed.
export function useApiToken(): string | null {
  return useSyncExternalStore(apiClient.onTokenChange, () => apiClient.getToken())
}
