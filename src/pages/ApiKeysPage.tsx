import { useCallback, useEffect, useState } from 'react'
import { toast } from 'sonner'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { CreateApiKeyDialog } from '@/components/CreateApiKeyDialog'
import { AdminOnlyNote, DangerButton, EmptyState, GhostButton, PageHeader, PrimaryButton } from '@/components/nocturne'
import { useAuth } from '@/contexts/AuthContext'
import { apiClient, type ApiKeyResponse } from '@/lib/api'

/* Every member can see the organization's keys (masked); only admins create
 * and revoke them - the same rules the backend enforces. */
export function ApiKeysPage() {
  const { isAdmin } = useAuth()
  const [keys, setKeys] = useState<ApiKeyResponse[] | null>(null)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [creating, setCreating] = useState(false)
  const [revoking, setRevoking] = useState<ApiKeyResponse | null>(null)

  const load = useCallback(() => {
    setLoadError(null)
    apiClient.getApiKeys()
      .then(setKeys)
      .catch((err) => setLoadError(err instanceof Error ? err.message : 'Could not load the keys'))
  }, [])

  useEffect(load, [load])

  const createButton = isAdmin && <PrimaryButton onClick={() => setCreating(true)}>Create key</PrimaryButton>

  return (
    <div className="p-[30px]">
      <PageHeader title="API keys"
                  sub="Keys let your tools and scripts call your organization's models with the OpenAI API."
                  action={keys && keys.length > 0 ? createButton : undefined} />

      {!isAdmin && <AdminOnlyNote>Only admins can create or revoke keys. Ask an admin if you need one.</AdminOnlyNote>}

      {loadError ? (
        <EmptyState title="Couldn't load the keys" body={loadError}
                    action={<GhostButton onClick={load}>Try again</GhostButton>} />
      ) : keys === null ? (
        <p className="text-[14px] text-[var(--n-text-4)]">Loading…</p>
      ) : keys.length === 0 ? (
        <EmptyState title="No API keys yet"
                    body={isAdmin
                      ? 'Create a key to call your models from code, scripts or other tools.'
                      : 'Once an admin creates a key, it shows up here.'}
                    action={createButton || undefined} />
      ) : (
        <div className="overflow-hidden rounded-[12px] border border-[var(--n-line)]">
          <div className="grid grid-cols-[1.6fr_1.2fr_1.4fr_0.9fr_auto] gap-3 bg-[var(--n-sidebar)] px-[18px] py-2.5 text-[12.5px] font-semibold uppercase leading-none tracking-[0.06em] text-[var(--n-text-3)]">
            <span>Name</span>
            <span>Key</span>
            <span>Created by</span>
            <span>Created</span>
            <span className="w-[68px]" />
          </div>
          {keys.map((k) => (
            <div key={k.id} data-testid="api-key-row"
                 className="grid grid-cols-[1.6fr_1.2fr_1.4fr_0.9fr_auto] items-center gap-3 border-t border-[var(--n-line)] px-[18px] py-3.5">
              <span className="min-w-0">
                <span className="block truncate text-[15px] font-semibold leading-[1.3] text-[var(--n-text)]">{k.name}</span>
                {k.disabled && (
                  <span className="mt-1 block text-[12.5px] leading-[1.3] text-[var(--n-text-4)]">
                    Disabled: its creator left the organization
                  </span>
                )}
              </span>
              <code className="truncate font-mono text-[13.5px] text-[var(--n-text-3)]">{k.value}</code>
              <span className="truncate text-[14px] text-[var(--n-text-3)]">{k.created_by ?? '—'}</span>
              <span className="text-[14px] text-[var(--n-text-3)]">{new Date(k.created_at).toLocaleDateString()}</span>
              <span className="flex w-[68px] justify-end">
                {isAdmin && <GhostButton onClick={() => setRevoking(k)} aria-label={`Revoke ${k.name}`}>Revoke</GhostButton>}
              </span>
            </div>
          ))}
        </div>
      )}

      <CreateApiKeyDialog open={creating} onOpenChange={setCreating}
                          onCreated={() => load()} />

      <RevokeDialog apiKey={revoking} onClose={() => setRevoking(null)}
                    onRevoked={(k) => {
                      setKeys((prev) => prev?.filter((x) => x.id !== k.id) ?? prev)
                      toast.success(`Revoked "${k.name}"`)
                    }} />
    </div>
  )
}

function RevokeDialog({
  apiKey, onClose, onRevoked,
}: {
  apiKey: ApiKeyResponse | null
  onClose: () => void
  onRevoked: (key: ApiKeyResponse) => void
}) {
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const close = () => {
    if (busy) return
    setError(null)
    onClose()
  }

  const revoke = async () => {
    if (!apiKey) return
    setBusy(true)
    setError(null)
    try {
      await apiClient.deleteApiKey(apiKey.id)
      onRevoked(apiKey)
      setBusy(false)
      onClose()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Could not revoke the key')
      setBusy(false)
    }
  }

  return (
    <Dialog open={apiKey !== null} onOpenChange={(open) => !open && close()}>
      <DialogContent>
        <div className="grid gap-4">
          <DialogHeader>
            <DialogTitle>Revoke "{apiKey?.name}"?</DialogTitle>
            <DialogDescription>
              Anything using this key stops working straight away. This can't be undone.
            </DialogDescription>
          </DialogHeader>
          {error && <p role="alert" className="text-[13.5px] text-[var(--n-text-2)]">{error}</p>}
          <DialogFooter>
            <GhostButton onClick={close}>Cancel</GhostButton>
            <DangerButton onClick={revoke} disabled={busy}>{busy ? 'Revoking…' : 'Revoke key'}</DangerButton>
          </DialogFooter>
        </div>
      </DialogContent>
    </Dialog>
  )
}
