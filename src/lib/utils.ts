import { type ClassValue, clsx } from "clsx"
import { twMerge } from "tailwind-merge"

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}

/** Bytes as GB (1 decimal) from 1 GB up, else whole MB. Binary units, like Settings. */
export function formatBytes(bytes: number): string {
  if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(1)} GB`
  return `${Math.round(bytes / 1024 ** 2)} MB`
}

/** Bytes per second as MB/s, whole numbers from 10 up. */
export function formatRate(bytesPerSec: number): string {
  const mb = bytesPerSec / 1024 ** 2
  return `${mb >= 10 ? Math.round(mb) : mb.toFixed(1)} MB/s`
}

/** "12.4s" under a minute, else "1m 05s". */
export function formatDuration(ms: number): string {
  const s = ms / 1000
  if (s < 60) return `${s.toFixed(1)}s`
  const whole = Math.round(s)
  return `${Math.floor(whole / 60)}m ${String(whole % 60).padStart(2, '0')}s`
}

/** "1.2 GB of 4.6 GB" plus the speed once there is one; null before the size is known. */
export function downloadSummary(downloaded: number, total: number, bytesPerSec: number, speedJoiner: string): string | null {
  if (total <= 0) return null
  const sizes = `${formatBytes(downloaded)} of ${formatBytes(total)}`
  return bytesPerSec > 0 ? `${sizes}${speedJoiner}${formatRate(bytesPerSec)}` : sizes
}
