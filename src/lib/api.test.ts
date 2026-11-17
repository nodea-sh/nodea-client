import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
vi.mock('./demoMode', () => ({ isDemoMode: () => false }))

import { apiClient, ApiError } from './api'

const ok = (body: unknown) => new Response(JSON.stringify(body), { status: 200 })
const unauthorized = (body: unknown = {}) => new Response(JSON.stringify(body), { status: 401 })

const sentToken = (fetchMock: ReturnType<typeof vi.fn>, call: number) =>
  (fetchMock.mock.calls[call][1] as RequestInit & { headers: Record<string, string> }).headers.Authorization

describe('ApiClient token refresh', () => {
  let fetchMock: ReturnType<typeof vi.fn>

  beforeEach(() => {
    invoke.mockResolvedValue(undefined)
    fetchMock = vi.fn()
    vi.stubGlobal('fetch', fetchMock)
    apiClient.setToken('expired')
  })

  afterEach(() => {
    apiClient.setTokenRefresher(null)
    vi.unstubAllGlobals()
  })

  it('refreshes an expired token and retries the request with the new one', async () => {
    const refresher = vi.fn().mockResolvedValue('fresh')
    apiClient.setTokenRefresher(refresher)
    fetchMock.mockResolvedValueOnce(unauthorized()).mockResolvedValueOnce(ok({ uuid: 'u1' }))

    await expect(apiClient.getMe()).resolves.toEqual({ uuid: 'u1' })

    expect(refresher).toHaveBeenCalledTimes(1)
    expect(sentToken(fetchMock, 0)).toBe('Bearer expired')
    expect(sentToken(fetchMock, 1)).toBe('Bearer fresh')
    expect(apiClient.getToken()).toBe('fresh')
  })

  it('shares one refresh between requests refused together', async () => {
    let resolve!: (token: string) => void
    const refresher = vi.fn(() => new Promise<string>((r) => (resolve = r)))
    apiClient.setTokenRefresher(refresher)
    fetchMock.mockImplementation((_url: string, init: RequestInit & { headers: Record<string, string> }) =>
      Promise.resolve(init.headers.Authorization === 'Bearer fresh' ? ok({}) : unauthorized())
    )

    const requests = Promise.all([apiClient.getMe(), apiClient.getOrganization()])
    await vi.waitFor(() => expect(refresher).toHaveBeenCalled())
    resolve('fresh')
    await requests

    expect(refresher).toHaveBeenCalledTimes(1)
  })

  it('retries without refreshing again when the token changed while the request was in flight', async () => {
    const refresher = vi.fn().mockResolvedValue('unused')
    apiClient.setTokenRefresher(refresher)
    fetchMock
      .mockImplementationOnce(async () => {
        apiClient.setToken('fresh')
        return unauthorized()
      })
      .mockResolvedValueOnce(ok({}))

    await apiClient.getMe()

    expect(refresher).not.toHaveBeenCalled()
    expect(sentToken(fetchMock, 1)).toBe('Bearer fresh')
  })

  it('gives up with the 401 when the refresh fails, and retries only once', async () => {
    apiClient.setTokenRefresher(vi.fn().mockResolvedValue(null))
    fetchMock.mockResolvedValue(unauthorized())
    await expect(apiClient.getMe()).rejects.toMatchObject({ status: 401 })
    expect(fetchMock).toHaveBeenCalledTimes(1)

    apiClient.setTokenRefresher(vi.fn().mockResolvedValue('still-refused'))
    fetchMock.mockClear()
    await expect(apiClient.getMe()).rejects.toBeInstanceOf(ApiError)
    expect(fetchMock).toHaveBeenCalledTimes(2)
  })

  it('does not refresh for a deleted account', async () => {
    const refresher = vi.fn().mockResolvedValue('fresh')
    apiClient.setTokenRefresher(refresher)
    fetchMock.mockResolvedValue(unauthorized({ code: 'account_deleted' }))

    await expect(apiClient.getMe()).rejects.toMatchObject({ code: 'account_deleted' })
    expect(refresher).not.toHaveBeenCalled()
  })

  it('tells listeners when the token changes', () => {
    const listener = vi.fn()
    const unsubscribe = apiClient.onTokenChange(listener)

    apiClient.setToken('expired')
    expect(listener).not.toHaveBeenCalled()
    apiClient.setToken('fresh')
    expect(listener).toHaveBeenCalledTimes(1)

    unsubscribe()
    apiClient.setToken('other')
    expect(listener).toHaveBeenCalledTimes(1)
  })
})
