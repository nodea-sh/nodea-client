import { test, expect, type Page } from '@playwright/test'

/**
 * Node page + sidebar status widget, driven entirely through demo mode
 * (`?demo=true&phase=<name>`, see src/lib/mockTauri.ts). No backend or login
 * required: each phase is pinned instantly instead of waiting through the
 * timed simulation, and demo mode auto-authenticates (AuthContext).
 */
async function gotoPhase(page: Page, phase: string) {
  await page.goto(`/node?demo=true&phase=${phase}`)
  await page.waitForLoadState('domcontentloaded')
}

test.describe('Node phases (demo mode)', () => {
  test('downloading: widget and hero agree', async ({ page }) => {
    await gotoPhase(page, 'downloading')

    await expect(page.getByText('Downloading', { exact: true })).toBeVisible()
    await expect(page.locator('h1')).toHaveText('Fetching the model for this device.')
    await expect(page.getByText('62%').first()).toBeVisible()
  })

  test('connecting with no model assigned shows no model', async ({ page }) => {
    await gotoPhase(page, 'connecting')

    await expect(page.locator('h1')).toHaveText('Reaching the coordinator.')
    await expect(page.getByText('Llama 3.1 8B Instruct')).toHaveCount(0)
    await expect(page.getByText(/cached/i)).toHaveCount(0)
    await expect(page.getByText('Fetching weights')).toHaveCount(0)
    await expect(page.getByText('1 · Done')).toHaveCount(0)
  })

  test('downloading names the model it was assigned', async ({ page }) => {
    await gotoPhase(page, 'downloading')

    // FORCED_MODEL_NAME in src/lib/mockTauri.ts, sent as the status's model_name.
    await expect(page.getByText('Llama 3.1 8B Instruct').first()).toBeVisible()
  })

  test('loading: widget and hero agree', async ({ page }) => {
    await gotoPhase(page, 'loading')

    await expect(page.getByText('Loading', { exact: true })).toBeVisible()
    await expect(page.locator('h1')).toHaveText('Loading the model into memory.')
  })

  test('stalled: a model failure shows retry, hero explains the interruption', async ({ page }) => {
    await gotoPhase(page, 'stalled')

    await expect(page.getByText('Download stalled', { exact: true })).toBeVisible()
    await expect(page.getByRole('button', { name: 'Retry now' })).toBeVisible()
    await expect(page.locator('h1')).toHaveText('The download stopped.')
  })

  test('offline: a connection error is not a download problem (#241)', async ({ page }) => {
    // The model is ready; only the coordinator is out of reach.
    await gotoPhase(page, 'offline')

    await expect(page.getByText('Offline', { exact: true }).first()).toBeVisible()
    await expect(page.locator('h1')).toHaveText("Can't reach the coordinator.")
    await expect(page.getByText(/download/i)).toHaveCount(0)
    await expect(page.getByRole('button', { name: 'Retry now' })).toBeVisible()
  })

  test('refused: the server refusing the account is explained, with no retry', async ({ page }) => {
    await gotoPhase(page, 'refused')

    await expect(page.getByText('Not sharing', { exact: true }).first()).toBeVisible()
    await expect(page.locator('h1')).toHaveText('The server refused this account.')
    await expect(page.getByText('Account is inactive. This device stopped sharing compute.')).toBeVisible()
    await expect(page.getByRole('button', { name: 'Retry now' })).toHaveCount(0)
  })

  test('a deactivated account explains who did it and what to do', async ({ page }) => {
    await page.goto('/node?demo=true&phase=serving&account=inactive')

    await expect(page.locator('h1')).toHaveText('An admin deactivated your account in Demo Organization.')
    await expect(page.getByText(/leave the organization in Settings/)).toBeVisible()
  })

  test('serving: widget and hero agree', async ({ page }) => {
    await gotoPhase(page, 'serving')

    // "Sharing" appears twice while ready (widget title + toggle row label).
    await expect(page.getByText('Sharing', { exact: true }).first()).toBeVisible()
    await expect(page.locator('h1')).toHaveText('This device is serving the network.')
  })

  test('paused: widget and hero agree', async ({ page }) => {
    await gotoPhase(page, 'paused')

    await expect(page.getByText('Paused', { exact: true })).toBeVisible()
    await expect(page.locator('h1')).toHaveText('Paused. This device is idle.')
  })

  test('sharing toggle flips widget, hero and ring together', async ({ page }) => {
    await gotoPhase(page, 'serving')

    await expect(page.locator('h1')).toHaveText('This device is serving the network.')

    // The whole status widget is the sharing toggle when the node is ready.
    await page.locator('button[aria-pressed]').click()

    await expect(page.getByText('Paused', { exact: true })).toBeVisible()
    await expect(page.locator('h1')).toHaveText('Paused. This device is idle.')

    // Toggling back should restore the serving copy.
    await page.locator('button[aria-pressed]').click()
    await expect(page.locator('h1')).toHaveText('This device is serving the network.')
  })
})

// The figures the mock's forced phases report (src/lib/mockTauri.ts): a
// 4.0 GB download 62% through at 32 MB/s, a load 7s in, a 12.4s load.
const PLACEHOLDERS = [/4\.9 GB/, /24 MB\/s/, /~20s/, /\b18s\b/, /71%/, /unified memory/, /where it left off/, /twenty seconds/]

async function expectNoPlaceholders(page: Page) {
  for (const placeholder of PLACEHOLDERS) {
    await expect(page.getByText(placeholder)).toHaveCount(0)
  }
}

test.describe('Node figures come from the node (demo mode)', () => {
  test('downloading shows the real size and speed', async ({ page }) => {
    await gotoPhase(page, 'downloading')

    await expect(page.getByText('2.5 GB of 4.0 GB · 32 MB/s')).toBeVisible()
    await expect(page.getByText('2.5 GB of 4.0 GB at 32 MB/s')).toBeVisible()
    await expectNoPlaceholders(page)
  })

  test('loading counts elapsed time and names the backend', async ({ page }) => {
    await gotoPhase(page, 'loading')

    await expect(page.getByText('Loading into memory on Metal')).toBeVisible()
    await expect(page.getByText('On Metal', { exact: true })).toBeVisible()
    const elapsed = page.getByText(/^(7|8)s$/).first()
    await expect(elapsed).toBeVisible()
    // It keeps counting.
    await expect(page.getByText(/^(9|10)s$/).first()).toBeVisible({ timeout: 4000 })
    await expectNoPlaceholders(page)
  })

  test('serving shows how long the load took, and real usage instead of a made-up capacity', async ({ page }) => {
    await page.goto('/node?demo=true&phase=serving&usage=42')

    await expect(page.getByText('Llama 3.1 8B Instruct, loaded in 12.4s.')).toBeVisible()
    await expect(page.getByTestId('usage-ring')).toContainText('42%')
    await expectNoPlaceholders(page)
  })

  test('stalled says the download starts again', async ({ page }) => {
    await gotoPhase(page, 'stalled')

    await expect(page.getByText('The model failed. Retry to start again.')).toBeVisible()
    await expectNoPlaceholders(page)
  })
})

test.describe('Node usage ring (demo mode, #317)', () => {
  const ring = (page: Page) => page.getByTestId('usage-ring')

  for (const pct of [0, 50, 100]) {
    test(`serving: the ring shows the machine's real usage (${pct}%)`, async ({ page }) => {
      await page.goto(`/node?demo=true&phase=serving&usage=${pct}`)

      await expect(ring(page)).toContainText(`${pct}%`)
      await expect(ring(page)).toContainText('GPU in use')
      await expect(ring(page)).not.toContainText('71%')
    })
  }

  test('serving but idle: the ring reads 0%, not a made-up figure', async ({ page }) => {
    await gotoPhase(page, 'serving')

    await expect(ring(page)).toContainText('0%')
    await expect(ring(page)).toHaveAttribute('title', /GPU utilization.*every app/)
  })

  test('without a GPU reading the ring says it is CPU', async ({ page }) => {
    await page.goto('/node?demo=true&phase=serving&usage=33&usageSource=cpu')

    await expect(ring(page)).toContainText('33%')
    await expect(ring(page)).toContainText('CPU in use')
  })

  test('paused still shows real usage, since other apps keep running', async ({ page }) => {
    await page.goto('/node?demo=true&phase=paused&usage=20')

    await expect(ring(page)).toContainText('20%')
  })

  test('sidebar widget shows no percentage while sharing, even when idle', async ({ page }) => {
    await gotoPhase(page, 'serving')

    await expect(page.getByText('Sharing', { exact: true }).first()).toBeVisible()
    await expect(page.getByTestId('widget-value')).toHaveCount(0)
    await expect(page.locator('button[aria-pressed]')).not.toContainText('100%')
  })

  test('sidebar widget still shows download progress', async ({ page }) => {
    await gotoPhase(page, 'downloading')

    await expect(page.getByTestId('widget-value')).toHaveText('62%')
  })
})

test.describe('Node throughput chart (demo mode)', () => {
  test('draws one bar per bucket of the selected range', async ({ page }) => {
    await gotoPhase(page, 'serving')
    const bars = page.getByTestId('throughput-bar')

    await expect(page.getByText('Tokens · 24h')).toBeVisible()
    await expect(bars).toHaveCount(25) // 1d: hourly

    await page.getByRole('button', { name: '1h', exact: true }).click()
    await expect(bars).toHaveCount(31) // 2-minute buckets
    await expect(page.getByText('−60m', { exact: true })).toBeVisible()
    await expect(page.getByText('−30m', { exact: true })).toBeVisible()

    await page.getByRole('button', { name: '1w', exact: true }).click()
    await expect(bars).toHaveCount(29) // 6-hour buckets
  })
})
