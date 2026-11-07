import { test, expect } from '@playwright/test'

test.describe('Signup', () => {
  test('signs the new user in and goes on to onboarding, without a check-your-email stop', async ({ page }) => {
    // Demo mode authenticates locally, so this exercises the form, the submit
    // handler and the redirect without a Supabase project or a backend.
    await page.goto('/signup?demo=true')

    await page.fill('#email', 'new-user@getnodea.com')
    await page.fill('#password', 'password1')
    await page.click('button[type="submit"]')

    await page.waitForURL((url) => !url.pathname.includes('/signup'), { timeout: 10000 })
    await expect(page.getByText(/check your email/i)).toHaveCount(0)
    expect(page.url()).not.toContain('/login')
  })
})
