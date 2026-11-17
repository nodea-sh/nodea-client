import { test, expect, type Page, type Request } from '@playwright/test'

/**
 * Forgot password, with the backend's three /v1/password-reset routes stubbed.
 * Not demo mode: these go through the real src/lib/api.ts, so the request each
 * page sends is checked as well.
 */

const GOOD_TOKEN = 'good-token'

async function stubPasswordReset(page: Page) {
  const requests: Request[] = []
  await page.route(
    (url) => url.pathname.startsWith('/site/v1/password-reset'),
    (route) => {
      const request = route.request()
      requests.push(request)
      const token = new URL(request.url()).pathname.split('/')[4]

      if (!token) {
        return route.fulfill({ status: 202, json: { message: 'If an account exists for this email, a reset link is on its way' } })
      }
      if (token !== GOOD_TOKEN) {
        return route.fulfill({ status: 410, json: { error: 'Invalid or expired password reset link' } })
      }
      return request.method() === 'GET'
        ? route.fulfill({ json: { email: 'person@getnodea.com' } })
        : route.fulfill({ json: { message: 'Password updated', email: 'person@getnodea.com' } })
    }
  )
  return requests
}

test.describe('Forgot password', () => {
  test('asks for a link that opens this app, keeping the email typed on the login form', async ({ page }) => {
    const requests = await stubPasswordReset(page)

    await page.goto('/login')
    await page.fill('#email', 'person@getnodea.com')
    await page.getByRole('link', { name: /forgot password/i }).click()

    await expect(page).toHaveURL(/\/forgot-password$/)
    await expect(page.locator('#email')).toHaveValue('person@getnodea.com')
    await page.getByRole('button', { name: /send reset link/i }).click()

    await expect(page.getByRole('status')).toContainText(/if an account exists for/i)
    expect(requests).toHaveLength(1)
    expect(requests[0].method()).toBe('POST')
    expect(requests[0].postDataJSON()).toEqual({ email: 'person@getnodea.com', client: 'desktop' })
  })

  test('sets a new password and lands on sign in with the email filled in', async ({ page }) => {
    const requests = await stubPasswordReset(page)

    await page.goto(`/reset-password/${GOOD_TOKEN}`)
    await expect(page.getByText('person@getnodea.com')).toBeVisible()

    // Refused before anything is sent, so the link is not spent.
    await page.getByLabel('New password', { exact: true }).fill('short')
    await page.getByLabel('Confirm new password').fill('short')
    await page.getByRole('button', { name: /set new password/i }).click()
    await expect(page.getByRole('alert')).toContainText(/at least 8 characters/i)

    await page.getByLabel('New password', { exact: true }).fill('newpass123')
    await page.getByLabel('Confirm new password').fill('newpass124')
    await page.getByRole('button', { name: /set new password/i }).click()
    await expect(page.getByRole('alert')).toContainText(/do not match/i)
    expect(requests.filter((r) => r.method() === 'POST')).toHaveLength(0)

    await page.getByLabel('Confirm new password').fill('newpass123')
    await page.getByRole('button', { name: /set new password/i }).click()

    await expect(page).toHaveURL(/\/login$/)
    await expect(page.getByRole('status')).toContainText(/your password was changed/i)
    await expect(page.locator('#email')).toHaveValue('person@getnodea.com')
    const reset = requests.find((r) => r.method() === 'POST')!
    expect(reset.postDataJSON()).toEqual({ password: 'newpass123' })
  })

  test('a link that no longer works offers a new one', async ({ page }) => {
    await stubPasswordReset(page)

    await page.goto('/reset-password/spent-token')

    await expect(page.getByText(/link expired or invalid/i)).toBeVisible()
    await page.getByRole('link', { name: /send a new link/i }).click()
    await expect(page).toHaveURL(/\/forgot-password$/)
  })
})
