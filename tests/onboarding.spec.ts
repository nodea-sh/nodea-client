import { test, expect } from '@playwright/test'

/**
 * First run, in demo mode: an admin names the organization, picks its first
 * model, and finishes the setup checklist on the Node page, all without the
 * web console. `?setup=fresh` is a brand-new organization (no models, no
 * keys, nothing done); `?account=orgless` a user with no organization yet.
 */
test.describe('In-app onboarding (demo mode)', () => {
  test('an admin goes from naming the organization to a first model', async ({ page }) => {
    await page.goto('/node?demo=true&account=orgless&setup=fresh')
    await expect(page).toHaveURL(/\/onboarding$/)

    await page.getByLabel('Organization Name').fill('Acme')
    await page.getByRole('button', { name: 'Continue' }).click()

    await expect(page).toHaveURL(/\/onboarding\/model/)
    await expect(page.getByRole('heading', { name: 'Pick a model for your organization' })).toBeVisible()

    // Suggestions fit this (mock 16 GB) Mac, and the first is preselected so
    // one click is enough.
    const options = page.getByRole('radio')
    await expect(options).toHaveCount(3)
    await expect(options.first()).toHaveAttribute('aria-checked', 'true')
    await expect(page.getByRole('radiogroup')).not.toContainText('70B')

    await page.getByRole('button', { name: 'Add and start' }).click()
    await expect(page).toHaveURL(/\/node/)
  })

  test('joining a new organization reconnects this device to it', async ({ page }) => {
    // The mock logs every native command (src/lib/mockTauri.ts).
    const commands: string[] = []
    page.on('console', (msg) => {
      const match = /\[Mock Tauri\] Invoking command: (\w+)/.exec(msg.text())
      if (match) commands.push(match[1])
    })

    await page.goto('/node?demo=true&account=orgless&setup=fresh&phase=serving')
    await expect(page).toHaveURL(/\/onboarding$/)
    await page.getByLabel('Organization Name').fill('Acme')
    await page.getByRole('button', { name: 'Continue' }).click()
    await expect(page).toHaveURL(/\/onboarding\/model/)
    commands.length = 0

    await page.getByRole('button', { name: 'Add and start' }).click()
    await expect(page).toHaveURL(/\/node/)

    // The old connection belongs to the organization the user left, even
    // though it still looks connected: it is replaced, without the user
    // having to toggle sharing.
    await expect.poll(() => commands.filter((c) => c.endsWith('_grpc'))).toEqual(['disconnect_grpc', 'connect_grpc'])
  })

  test('the download progress list gives way to the setup checklist', async ({ page }) => {
    await page.goto('/node?demo=true&setup=fresh&phase=downloading')
    await expect(page.getByRole('region', { name: 'Setup checklist' })).toBeVisible()
    await expect(page.getByText('Fetching weights')).toHaveCount(0)

    // Without the checklist the list is back.
    await page.goto('/node?demo=true&phase=downloading')
    await expect(page.getByText('Fetching weights')).toBeVisible()
  })

  test('skipping the model lands on a Node page that says what is missing', async ({ page }) => {
    await page.goto('/onboarding/model?demo=true&setup=fresh&phase=connecting')
    await page.getByRole('button', { name: 'Skip for now' }).click()

    await expect(page).toHaveURL(/\/node/)
    await expect(page.getByRole('heading', { name: 'Add a model to start serving.' })).toBeVisible()
    await expect(page.getByRole('main').getByRole('button', { name: 'Add a model' }).first()).toBeVisible()
  })

  test('a member with no models is told to wait for an admin', async ({ page }) => {
    await page.goto('/node?demo=true&models=none&role=standard&phase=connecting')

    await expect(page.getByRole('heading', { name: 'Waiting for an admin to add a model.' })).toBeVisible()
    await expect(page.getByRole('main').getByRole('button', { name: 'Add a model' })).toHaveCount(0)
  })

  test('the model step is skipped for an organization that already has one', async ({ page }) => {
    await page.goto('/onboarding/model?demo=true&phase=serving')
    await expect(page).toHaveURL(/\/node/)
  })

  test('the checklist walks an admin through a key and a first request', async ({ page }) => {
    await page.goto('/node?demo=true&setup=fresh&phase=connecting')

    const checklist = page.getByRole('region', { name: 'Setup checklist' })
    await expect(checklist).toContainText('Get set up')
    await expect(page.getByTestId('setup-step-model')).toHaveAttribute('data-state', 'current')

    // Add the model from the checklist itself.
    await checklist.getByRole('button', { name: 'Add a model' }).click()
    await page.getByRole('dialog').getByRole('region', { name: 'Recommended for this device' })
      .getByRole('button', { name: /^Add / }).first().click()
    await expect(page.getByTestId('setup-step-model')).toHaveAttribute('data-state', 'done')

    // Then the key, created in place.
    await expect(page.getByTestId('setup-step-key')).toHaveAttribute('data-state', 'current')
    await checklist.getByRole('button', { name: 'Create key' }).click()
    const dialog = page.getByRole('dialog')
    await dialog.getByLabel('Name').fill('My laptop')
    await dialog.getByRole('button', { name: 'Create key' }).click()
    const secret = (await dialog.getByTestId('new-api-key').textContent())!
    await dialog.getByRole('button', { name: "I've saved it" }).click()

    // The example uses that key - masked on screen - and the model just added.
    await expect(page.getByTestId('setup-step-key')).toHaveAttribute('data-state', 'done')
    const snippet = page.getByTestId('first-request-snippet')
    await expect(snippet).toContainText(`${secret.slice(0, 3)}***`)
    await expect(snippet).not.toContainText(secret)
    await expect(snippet).toContainText('/openai/v1/chat/completions')
    await expect(checklist).toContainText('Waiting for your first request…')

    // The mock reports the request a few seconds after the key exists.
    await expect(checklist).toContainText("You're all set", { timeout: 15000 })
  })

  test('a member confirms their own first request', async ({ page }) => {
    await page.goto('/node?demo=true&setup=fresh&role=standard&phase=serving')

    const checklist = page.getByRole('region', { name: 'Setup checklist' })
    await expect(page.getByTestId('setup-step-key')).toContainText('Only admins create keys')
    await checklist.getByRole('button', { name: "I've made a request" }).click()
    await expect(checklist).toContainText("You're all set")
  })

  test('the checklist can be dismissed', async ({ page }) => {
    await page.goto('/node?demo=true&setup=fresh&phase=connecting')

    const checklist = page.getByRole('region', { name: 'Setup checklist' })
    await expect(checklist).toBeVisible()
    await checklist.getByRole('button', { name: 'Dismiss setup checklist' }).click()
    await expect(checklist).toHaveCount(0)
  })
})
