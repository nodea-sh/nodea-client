import { test, expect, Page } from '@playwright/test'

/**
 * The desktop Google sign-in screen while it waits for the browser.
 *
 * These run in a plain browser, so the Tauri host is faked: `isTauri` makes the
 * app take its desktop path and `__TAURI_INTERNALS__.invoke` answers the three
 * OAuth commands. `finish_oauth` never resolves on its own - exactly what a
 * machine with no default browser looks like (#314) - and `cancel_oauth`
 * rejects it the way the Rust side does. Every other command answers null.
 */

const SIGN_IN_URL = 'https://ref.supabase.co/auth/v1/authorize?provider=google&code_challenge=abc'

async function fakeDesktopHost(page: Page) {
  await page.addInitScript((signInUrl) => {
    type Win = Window & Record<string, unknown>
    const w = window as unknown as Win
    const calls: string[] = []
    let rejectFinish: ((reason: string) => void) | null = null
    let nextCallback = 1

    w.__invokeCalls = calls
    w.isTauri = true
    w.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} }
    w.__TAURI_INTERNALS__ = {
      metadata: {
        currentWindow: { label: 'main' },
        currentWebview: { windowLabel: 'main', label: 'main' },
      },
      plugins: {},
      transformCallback: () => nextCallback++,
      unregisterCallback: () => {},
      convertFileSrc: (path: string) => path,
      invoke: async (cmd: string) => {
        calls.push(cmd)
        switch (cmd) {
          case 'start_oauth':
            return signInUrl
          case 'finish_oauth':
            return new Promise((_, reject) => {
              rejectFinish = reject
            })
          case 'cancel_oauth':
            rejectFinish?.('Sign-in was cancelled')
            rejectFinish = null
            return null
          default:
            return null
        }
      },
    }
  }, SIGN_IN_URL)
}

const invokeCalls = (page: Page) =>
  page.evaluate(() => (window as unknown as { __invokeCalls: string[] }).__invokeCalls)

test.describe('Desktop Google sign-in', () => {
  test.beforeEach(async ({ page }) => {
    await page.clock.install()
    await fakeDesktopHost(page)
    await page.goto('/login')
    await page.getByRole('button', { name: /continue with Google/i }).click()
  })

  test('offers the sign-in link and Cancel while waiting for the browser', async ({ page }) => {
    const status = page.getByRole('status', { name: 'Google sign-in' })

    await expect(status).toContainText('Finish signing in with Google in your browser.')
    await expect(status).toContainText("Browser didn't open? Copy this link")
    await expect(page.getByLabel('Sign-in link')).toHaveValue(SIGN_IN_URL)
    await expect(page.getByRole('button', { name: 'Copy link' })).toBeVisible()
    await expect(page.getByRole('button', { name: 'Cancel' })).toBeVisible()
    await expect(status).not.toContainText('Still waiting')
  })

  test('says it is still waiting once the browser has been silent a while', async ({ page }) => {
    const status = page.getByRole('status', { name: 'Google sign-in' })
    await expect(status).toBeVisible()

    await page.clock.fastForward(16_000)

    await expect(status).toContainText('Still waiting for the browser…')
    await expect(status).toContainText('email and password')
    await expect(page.getByLabel('Sign-in link')).toHaveValue(SIGN_IN_URL)
    await expect(page.getByRole('button', { name: 'Cancel' })).toBeVisible()
  })

  test('Cancel abandons the attempt and resets the screen', async ({ page }) => {
    await expect(page.getByLabel('Sign-in link')).toBeVisible()

    await page.getByRole('button', { name: 'Cancel' }).click()

    await expect(page.getByRole('button', { name: /continue with Google/i })).toBeEnabled()
    await expect(page.getByRole('status', { name: 'Google sign-in' })).toHaveCount(0)
    await expect(page.locator('button[type="submit"]')).toHaveText('Sign in')
    await expect(page.locator('button[type="submit"]')).toBeEnabled()
    // A cancel is the user's choice, not a failure to report.
    await expect(page.getByRole('alert')).toHaveCount(0)
    expect(await invokeCalls(page)).toEqual(
      expect.arrayContaining(['start_oauth', 'finish_oauth', 'cancel_oauth']),
    )
  })
})
