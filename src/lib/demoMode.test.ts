import { afterEach, describe, expect, it, vi } from 'vitest'
import { isDemoMode } from './demoMode'

const visit = (search: string) => window.history.replaceState(null, '', `/${search}`)

afterEach(() => {
  localStorage.clear()
  visit('')
})

describe('isDemoMode', () => {
  it('is off in a production build, whatever the URL or storage say', () => {
    vi.stubEnv('DEV', false)

    visit('?demo=true')
    expect(isDemoMode()).toBe(false)

    visit('')
    localStorage.setItem('demoMode', 'true')
    expect(isDemoMode()).toBe(false)
  })

  it('does not persist a ?demo= flag in a production build', () => {
    vi.stubEnv('DEV', false)
    visit('?demo=1')
    isDemoMode()
    expect(localStorage.getItem('demoMode')).toBeNull()
  })

  it('follows ?demo= and the stored flag in a development build', () => {
    vi.stubEnv('DEV', true)

    visit('?demo=1')
    expect(isDemoMode()).toBe(true)

    visit('')
    expect(isDemoMode()).toBe(true)

    visit('?demo=false')
    expect(isDemoMode()).toBe(false)
  })
})
