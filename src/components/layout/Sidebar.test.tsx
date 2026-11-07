import { act, cleanup, fireEvent, render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { NodeState } from '@/hooks/useNodePhase'

const { invoke, apiClient } = vi.hoisted(() => ({
  invoke: vi.fn(),
  apiClient: {
    setToken: vi.fn(),
    setAccountDeletedHandler: vi.fn(),
    getMe: vi.fn(),
    getOrganization: vi.fn(),
    getOrganizationMembers: vi.fn(),
  },
}))

// Signed in through the desktop app: auto_login hands back a Supabase token.
vi.mock('@tauri-apps/api/core', () => ({ invoke, isTauri: () => true }))
vi.mock('@supabase/supabase-js', () => ({ createClient: () => ({ auth: {} }) }))
vi.mock('@/lib/api', () => ({ apiClient }))
vi.mock('@/hooks/useUpdater', () => ({
  useUpdater: () => ({ update: { state: 'idle' }, dismissed: false, dismiss: vi.fn(), restart: vi.fn(), check: vi.fn() }),
}))
vi.mock('@/components/NodeStatusWidget', () => ({ NodeStatusWidget: () => null }))

import { AuthProvider } from '@/contexts/AuthContext'
import { Sidebar } from './Sidebar'

const EMAIL = 'ada@example.com'
const ORG = { id: 1, name: 'Analytical Engines', slug: 'ae', member_count: 1, created_at: '2026-01-01T00:00:00Z' }

const token = (userMetadata: Record<string, unknown> = {}) =>
  ['h', btoa(JSON.stringify({ sub: 'user-uuid', email: EMAIL, user_metadata: userMetadata })), 's'].join('.')

const me = (fullName?: string | null) => ({
  uuid: 'user-uuid',
  email: EMAIL,
  ...(fullName === undefined ? {} : { full_name: fullName }),
  role: 'admin',
  is_active: true,
  organization: { uuid: 'org-uuid', name: ORG.name, member_count: 1 },
  can_leave: true,
  leave_blocked_reason: null,
  leave_deletes_organization: false,
  leave_disables_api_keys: 0,
})

const signIn = async (accessToken: string) => {
  invoke.mockImplementation(async (cmd: string) => (cmd === 'auto_login' ? accessToken : undefined))
  render(
    <MemoryRouter>
      <AuthProvider>
        <Sidebar node={{} as NodeState} />
      </AuthProvider>
    </MemoryRouter>,
  )
  // checkAuth, then the /v1/me check it triggers.
  await act(async () => {})
  await act(async () => {})
}

const accountButton = () => screen.getByText(EMAIL).closest('button') as HTMLElement

beforeEach(() => {
  apiClient.getOrganization.mockResolvedValue(ORG)
  apiClient.getOrganizationMembers.mockResolvedValue({
    users: [{ id: '1', uuid: 'user-uuid', email: EMAIL, name: null, role: 'admin', is_active: true, created_at: '' }],
    total_count: 1,
  })
})

afterEach(() => {
  cleanup()
  localStorage.clear()
  window.history.replaceState(null, '', '/')
})

describe('Sidebar account name', () => {
  it('shows full_name from /v1/me, with its initials', async () => {
    apiClient.getMe.mockResolvedValue(me('Ada Lovelace'))
    await signIn(token())

    expect(accountButton().textContent).toContain('Ada Lovelace')
    expect(accountButton().textContent).toContain('AL')
    expect(screen.queryByText('User')).toBeNull()
  })

  it('falls back to the Google name when full_name is null', async () => {
    apiClient.getMe.mockResolvedValue(me(null))
    await signIn(token({ full_name: 'Ada King' }))

    expect(accountButton().textContent).toContain('Ada King')
    expect(screen.queryByText('User')).toBeNull()
  })

  it('falls back to the email local part when full_name is null and there is no provider name', async () => {
    apiClient.getMe.mockResolvedValue(me(null))
    await signIn(token())

    expect(accountButton().textContent).toContain('ada')
    expect(screen.queryByText('User')).toBeNull()
  })

  it('falls back to the email local part when full_name is missing', async () => {
    apiClient.getMe.mockResolvedValue(me(undefined))
    await signIn(token())

    expect(accountButton().textContent).toContain('ada')
    expect(screen.queryByText('User')).toBeNull()

    fireEvent.click(accountButton())
    expect(screen.queryByText('User')).toBeNull()
  })

  it('picks up a name changed in the console when the window regains focus', async () => {
    apiClient.getMe.mockResolvedValue(me(null))
    await signIn(token())
    expect(accountButton().textContent).not.toContain('Ada Lovelace')

    apiClient.getMe.mockResolvedValue(me('Ada Lovelace'))
    await act(async () => {
      window.dispatchEvent(new Event('focus'))
    })

    expect(accountButton().textContent).toContain('Ada Lovelace')
  })
})

describe('demo mode outside development builds', () => {
  it('ignores ?demo= and the stored flag and signs in for real', async () => {
    vi.stubEnv('DEV', false)
    localStorage.setItem('demoMode', 'true')
    window.history.replaceState(null, '', '/?demo=true')
    apiClient.getMe.mockResolvedValue(me('Ada Lovelace'))

    await signIn(token())

    expect(invoke).toHaveBeenCalledWith('auto_login')
    expect(screen.queryByText('Demo User')).toBeNull()
    expect(screen.queryByText('demo@getnodea.com')).toBeNull()
    expect(accountButton().textContent).toContain('Ada Lovelace')
  })
})
