const clean = (value: string | null | undefined): string | undefined => {
  const trimmed = value?.trim()
  return trimmed ? trimmed : undefined
}

/**
 * The name to show for the signed-in account: the name set in the console
 * (GET /v1/me full_name), then the one the identity provider gave (Google's
 * user_metadata.full_name), then the local part of the email.
 */
export const resolveDisplayName = (
  fullName: string | null | undefined,
  providerName: string | null | undefined,
  email: string | null | undefined,
): string => clean(fullName) ?? clean(providerName) ?? clean(email?.split('@')[0]) ?? ''

/** "Ada Lovelace" -> "AL", "igayoso" -> "IG". */
export const initialsOf = (name: string): string => {
  const words = name.trim().split(/\s+/).filter(Boolean)
  if (words.length === 0) return ''
  if (words.length === 1) return words[0].slice(0, 2).toUpperCase()
  return (words[0][0] + words[1][0]).toUpperCase()
}

/** Supabase access tokens carry the identity provider's profile here. */
export const providerNameFromToken = (payload: { user_metadata?: Record<string, unknown> }): string | undefined => {
  const meta = payload.user_metadata
  const name = meta?.full_name ?? meta?.name
  return typeof name === 'string' ? clean(name) : undefined
}
