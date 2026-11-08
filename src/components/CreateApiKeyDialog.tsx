import { useState } from 'react'
import { Check, Copy } from '@phosphor-icons/react'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { GhostButton, PrimaryButton } from '@/components/nocturne'
import { apiClient, ApiError, type ApiKeyResponse } from '@/lib/api'

/* Two steps: name the key, then show its secret - the only time the backend
 * returns it unmasked. Closing the second step needs a deliberate click, so a
 * stray Escape or click outside can't lose the secret. */
export function CreateApiKeyDialog({
  open, onOpenChange, onCreated,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  onCreated: (key: ApiKeyResponse) => void
}) {
  const [name, setName] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [created, setCreated] = useState<ApiKeyResponse | null>(null)
  const [copied, setCopied] = useState(false)

  const reset = () => {
    setName('')
    setError(null)
    setCreated(null)
    setCopied(false)
  }

  const handleOpenChange = (next: boolean) => {
    if (busy || (created && !next)) return
    if (!next) reset()
    onOpenChange(next)
  }

  const finish = () => {
    reset()
    onOpenChange(false)
  }

  const submit = async (e: React.FormEvent) => {
    e.preventDefault()
    setBusy(true)
    setError(null)
    try {
      const key = await apiClient.createApiKey(name.trim())
      setCreated(key)
      onCreated(key)
    } catch (err) {
      setError(
        err instanceof ApiError && err.status === 409
          ? 'A key with this name already exists. Pick another name.'
          : err instanceof Error ? err.message : 'Could not create the key'
      )
    } finally {
      setBusy(false)
    }
  }

  const copy = async () => {
    if (!created) return
    try {
      await navigator.clipboard.writeText(created.value)
      setCopied(true)
    } catch {
      setCopied(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent onEscapeKeyDown={(e) => created && e.preventDefault()}
                     onPointerDownOutside={(e) => created && e.preventDefault()}>
        {created ? (
          <div className="grid gap-4">
            <DialogHeader>
              <DialogTitle>API key created</DialogTitle>
              <DialogDescription>
                Copy it now and store it somewhere safe. It won't be shown again.
              </DialogDescription>
            </DialogHeader>
            <div className="flex items-center gap-2 rounded-[8px] border border-[var(--n-border)] bg-[var(--n-bg)] py-1.5 pl-3 pr-1.5">
              <code data-testid="new-api-key" className="min-w-0 flex-1 truncate font-mono text-[13.5px] text-[var(--n-text)]">
                {created.value}
              </code>
              <GhostButton onClick={copy} aria-label="Copy key">
                <span className="flex items-center gap-1.5">
                  {copied ? <Check size={14} /> : <Copy size={14} />}
                  {copied ? 'Copied' : 'Copy'}
                </span>
              </GhostButton>
            </div>
            <DialogFooter>
              <PrimaryButton onClick={finish}>I've saved it</PrimaryButton>
            </DialogFooter>
          </div>
        ) : (
          <form onSubmit={submit} className="grid gap-4">
            <DialogHeader>
              <DialogTitle>Create an API key</DialogTitle>
              <DialogDescription>
                Keys let your tools and scripts call your organization's models. Give it a name that
                says where it will be used, like "CI" or "Support bot".
              </DialogDescription>
            </DialogHeader>
            <label className="grid gap-2 text-[14px] text-[var(--n-text-2)]">
              Name
              <Input value={name} autoFocus autoComplete="off" disabled={busy} maxLength={255}
                     onChange={(e) => { setName(e.target.value); setError(null) }} />
            </label>
            {error && <p role="alert" className="text-[13.5px] text-[var(--n-text-2)]">{error}</p>}
            <DialogFooter>
              <GhostButton onClick={() => handleOpenChange(false)}>Cancel</GhostButton>
              <PrimaryButton type="submit" disabled={busy || !name.trim()}>
                {busy ? 'Creating…' : 'Create key'}
              </PrimaryButton>
            </DialogFooter>
          </form>
        )}
      </DialogContent>
    </Dialog>
  )
}
