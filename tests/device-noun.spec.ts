import { test, expect, type Page } from '@playwright/test'
import * as fs from 'fs'
import * as path from 'path'
import { fileURLToPath } from 'url'
import { deviceNoun, osFromUserAgent, thisDevice, thisDeviceCapitalized } from '../src/lib/device'

/**
 * The UI names the machine after the OS it runs on (#315): "this Mac" on
 * macOS, "this PC" on Windows, "this computer" on Linux and anything else.
 * Pages are rendered in demo mode with `?os=<name>` pinning the platform.
 */

const SRC = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../src')

const UA = {
  macos: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)',
  windows: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36 Edg/130.0.0.0',
  linux: 'Mozilla/5.0 (X11; Ubuntu; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko)',
}

test.describe('deviceNoun helper', () => {
  test('maps each OS to its noun', () => {
    expect(deviceNoun('macos')).toBe('Mac')
    expect(deviceNoun('windows')).toBe('PC')
    expect(deviceNoun('linux')).toBe('computer')
    expect(deviceNoun('unknown')).toBe('computer')
  })

  test('builds the phrases used in copy', () => {
    expect(thisDevice('linux')).toBe('this computer')
    expect(thisDeviceCapitalized('windows')).toBe('This PC')
    expect(thisDeviceCapitalized('macos')).toBe('This Mac')
  })

  test('reads the OS from each Tauri webview user agent', () => {
    expect(osFromUserAgent(UA.macos)).toBe('macos')
    expect(osFromUserAgent(UA.windows)).toBe('windows')
    expect(osFromUserAgent(UA.linux)).toBe('linux')
    expect(osFromUserAgent('Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X)')).toBe('unknown')
    expect(osFromUserAgent('Mozilla/5.0 (Linux; Android 14)')).toBe('unknown')
    expect(osFromUserAgent('')).toBe('unknown')
  })
})

test('no hardcoded "this Mac" in src/', () => {
  const offenders: string[] = []
  const walk = (dir: string) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name)
      if (entry.isDirectory()) walk(full)
      else if (/\.(tsx?|jsx?|css|html)$/.test(entry.name)) {
        fs.readFileSync(full, 'utf-8').split('\n').forEach((line, i) => {
          if (/this Mac\b/i.test(line)) offenders.push(`${path.relative(SRC, full)}:${i + 1}`)
        })
      }
    }
  }
  walk(SRC)
  expect(offenders, 'use thisDevice() / thisDeviceCapitalized() from src/lib/device.ts').toEqual([])
})

async function expectNoMac(page: Page) {
  // "MacBook Pro" (a demo device name) is fine; the word "Mac" on its own is not.
  await expect(page.locator('body')).not.toHaveText(/\bMac\b/)
}

test.describe('pages on Linux say "computer" (demo mode)', () => {
  for (const phase of ['downloading', 'connecting', 'offline', 'refused', 'serving', 'paused']) {
    test(`Node page: ${phase}`, async ({ page }) => {
      await page.goto(`/node?demo=true&phase=${phase}&os=linux`)
      await expect(page.locator('h1')).toBeVisible()
      await expect(page.locator('body')).toContainText(/this computer/i)
      await expectNoMac(page)
    })
  }

  test('Login page, including the deleted-account notice', async ({ page }) => {
    await page.goto('/login?account=deleted&os=linux')
    await expect(page.getByText('Sign in and this computer joins the network.')).toBeVisible()
    await expect(page.getByText('This account was deleted. This computer stopped sharing compute.')).toBeVisible()
    await expectNoMac(page)
  })

  test('Fleet page badges the current device', async ({ page }) => {
    await page.goto('/fleet?demo=true&os=linux')
    await expect(page.getByText('this computer', { exact: true })).toBeVisible()
    await expectNoMac(page)
  })

  test('Settings page and the delete-account dialog', async ({ page }) => {
    await page.goto('/settings?demo=true&os=linux')
    await expect(page.getByText('While Nodea is running and this computer is awake.')).toBeVisible()
    await page.getByRole('button', { name: 'Delete' }).click()
    await expect(page.getByRole('dialog')).toContainText('Downloaded models stay on this computer.')
    await expectNoMac(page)
  })
})

test('pages on Windows say "PC" (demo mode)', async ({ page }) => {
  await page.goto('/node?demo=true&phase=serving&os=windows')
  await expect(page.locator('h1')).toHaveText('This PC is serving the network.')
  await expectNoMac(page)
})
