import { describe, expect, it } from 'vitest'
import { initialsOf, providerNameFromToken, resolveDisplayName } from './displayName'

describe('resolveDisplayName', () => {
  it('prefers the account name from /v1/me', () => {
    expect(resolveDisplayName('Ada Lovelace', 'Ada G', 'ada@example.com')).toBe('Ada Lovelace')
  })

  it('falls back to the identity provider name when full_name is null, blank or missing', () => {
    expect(resolveDisplayName(null, 'Ada G', 'ada@example.com')).toBe('Ada G')
    expect(resolveDisplayName('   ', 'Ada G', 'ada@example.com')).toBe('Ada G')
    expect(resolveDisplayName(undefined, 'Ada G', 'ada@example.com')).toBe('Ada G')
  })

  it('falls back to the email local part last', () => {
    expect(resolveDisplayName(null, undefined, 'ada.l@example.com')).toBe('ada.l')
  })

  it('never invents a placeholder name', () => {
    expect(resolveDisplayName(null, null, null)).toBe('')
  })
})

describe('initialsOf', () => {
  it('uses the first letters of the first two words', () => {
    expect(initialsOf('Ada King Lovelace')).toBe('AK')
  })

  it('uses the first two letters of a single word', () => {
    expect(initialsOf('igayoso')).toBe('IG')
  })

  it('is empty for an empty name', () => {
    expect(initialsOf('')).toBe('')
  })
})

describe('providerNameFromToken', () => {
  it('reads full_name, then name, from user_metadata', () => {
    expect(providerNameFromToken({ user_metadata: { full_name: 'Ada', name: 'A' } })).toBe('Ada')
    expect(providerNameFromToken({ user_metadata: { name: 'A' } })).toBe('A')
  })

  it('ignores missing or non-string values', () => {
    expect(providerNameFromToken({})).toBeUndefined()
    expect(providerNameFromToken({ user_metadata: { full_name: 42 } })).toBeUndefined()
  })
})
