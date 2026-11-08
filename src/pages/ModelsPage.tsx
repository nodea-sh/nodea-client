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
import { AddModelsDialog } from '@/components/AddModelsDialog'
import { ConsoleLink } from '@/components/ConsoleLink'
import { AdminOnlyNote, DangerButton, EmptyState, GhostButton, PageHeader, PrimaryButton } from '@/components/nocturne'
import { useAuth } from '@/contexts/AuthContext'
import { announceModelsChanged } from '@/hooks/useNodePhase'
import { apiClient, type OrgModel } from '@/lib/api'
import { formatMemory, modelDisplayName } from '@/lib/recommendedModels'
import { consoleModelUrl } from '@/lib/webConsole'

/* The organization's models. Every member sees them; only admins add,
 * enable/disable and remove - as the backend enforces. */
export function ModelsPage() {
  const { isAdmin } = useAuth()
  const [models, setModels] = useState<OrgModel[] | null>(null)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [adding, setAdding] = useState(false)
  const [removing, setRemoving] = useState<OrgModel | null>(null)
  const [toggling, setToggling] = useState<string | null>(null)

  const load = useCallback(() => {
    setLoadError(null)
    apiClient.getOrgModels()
      .then(setModels)
      .catch((err) => setLoadError(err instanceof Error ? err.message : 'Could not load the models'))
  }, [])

  useEffect(load, [load])

  const toggle = async (m: OrgModel) => {
    setToggling(m.ref)
    try {
      const updated = await apiClient.setModelEnabled(m.ref, !m.enabled)
      setModels((prev) => prev?.map((x) => (x.ref === m.ref ? updated : x)) ?? prev)
      announceModelsChanged()
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Could not update the model')
    } finally {
      setToggling(null)
    }
  }

  const addButton = isAdmin && <PrimaryButton onClick={() => setAdding(true)}>Add models</PrimaryButton>

  return (
    <div className="p-[30px]">
      <PageHeader title="Models"
                  sub="The models your organization's devices serve. Nodea decides which device runs which, based on memory and demand."
                  action={models && models.length > 0 ? addButton : undefined} />

      {!isAdmin && <AdminOnlyNote>Only admins can add or remove models. Ask an admin to add the one you need.</AdminOnlyNote>}

      {loadError ? (
        <EmptyState title="Couldn't load the models" body={loadError}
                    action={<GhostButton onClick={load}>Try again</GhostButton>} />
      ) : models === null ? (
        <p className="text-[14px] text-[var(--n-text-4)]">Loading…</p>
      ) : models.length === 0 ? (
        <EmptyState title="No models yet"
                    body={isAdmin
                      ? 'Add one so your devices have something to serve.'
                      : 'Once an admin adds a model, this device starts serving it on its own.'}
                    action={addButton || undefined} />
      ) : (
        <div className="overflow-hidden rounded-[12px] border border-[var(--n-line)]">
          <div className="grid grid-cols-[2fr_0.9fr_0.9fr_auto] gap-3 bg-[var(--n-sidebar)] px-[18px] py-2.5 text-[12.5px] font-semibold uppercase leading-none tracking-[0.06em] text-[var(--n-text-3)]">
            <span>Model</span>
            <span>Memory</span>
            <span>Serving on</span>
            <span className="w-[150px]" />
          </div>
          {models.map((m) => (
            <div key={m.ref} data-testid="model-row"
                 className="grid grid-cols-[2fr_0.9fr_0.9fr_auto] items-center gap-3 border-t border-[var(--n-line)] px-[18px] py-3.5">
              <span className="min-w-0">
                <span className="block truncate text-[15px] font-semibold leading-[1.3]"
                      style={{ color: m.enabled ? 'var(--n-text)' : 'var(--n-text-4)' }}>
                  <ConsoleLink href={consoleModelUrl(m.ref)}
                               label={`Open ${modelDisplayName(m.ref)} in the web console`}>
                    {modelDisplayName(m.ref)}
                  </ConsoleLink>
                </span>
                <span className="mt-[3px] block truncate font-mono text-[12.5px] leading-[1.3] text-[var(--n-text-4)]">{m.ref}</span>
              </span>
              <span className="text-[14px] text-[var(--n-text-3)]">{formatMemory(m.required_memory_mb)}</span>
              <span className="text-[14px] text-[var(--n-text-3)]">
                {m.enabled
                  ? `${m.device_count} ${m.device_count === 1 ? 'device' : 'devices'}${m.pending_device_count ? ` · ${m.pending_device_count} starting` : ''}`
                  : 'Disabled'}
              </span>
              <span className="flex w-[150px] items-center justify-end gap-3">
                {isAdmin && (
                  <>
                    <Switch on={m.enabled} disabled={toggling === m.ref} onChange={() => toggle(m)}
                            label={`${m.enabled ? 'Disable' : 'Enable'} ${modelDisplayName(m.ref)}`} />
                    <GhostButton onClick={() => setRemoving(m)} aria-label={`Remove ${modelDisplayName(m.ref)}`}>Remove</GhostButton>
                  </>
                )}
              </span>
            </div>
          ))}
        </div>
      )}

      <AddModelsDialog open={adding} onOpenChange={setAdding}
                       onAdded={(m) => {
                         setModels((prev) => [...(prev ?? []), m])
                         announceModelsChanged()
                         toast.success(`Added ${modelDisplayName(m.ref)}`)
                       }} />

      <RemoveDialog model={removing} onClose={() => setRemoving(null)}
                    onRemoved={(m) => {
                      setModels((prev) => prev?.filter((x) => x.ref !== m.ref) ?? prev)
                      announceModelsChanged()
                    }} />
    </div>
  )
}

function Switch({ on, onChange, disabled, label }: { on: boolean; onChange: () => void; disabled?: boolean; label: string }) {
  return (
    <button role="switch" aria-checked={on} aria-label={label} onClick={onChange} disabled={disabled}
            className="relative inline-block h-5 w-9 flex-none rounded-[10px] transition-colors duration-[180ms] disabled:opacity-50"
            style={{ background: on ? 'var(--n-accent-700)' : 'var(--n-border)' }}>
      <span className="absolute top-0.5 h-4 w-4 rounded-full transition-[left,background] duration-[180ms]"
            style={{ left: on ? 18 : 2, background: on ? 'var(--n-accent-200)' : 'var(--n-text-4)' }} />
    </button>
  )
}

function RemoveDialog({
  model, onClose, onRemoved,
}: {
  model: OrgModel | null
  onClose: () => void
  onRemoved: (model: OrgModel) => void
}) {
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const close = () => {
    if (busy) return
    setError(null)
    onClose()
  }

  const remove = async () => {
    if (!model) return
    setBusy(true)
    setError(null)
    try {
      await apiClient.removeModel(model.ref)
      onRemoved(model)
      setBusy(false)
      onClose()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Could not remove the model')
      setBusy(false)
    }
  }

  return (
    <Dialog open={model !== null} onOpenChange={(open) => !open && close()}>
      <DialogContent>
        <div className="grid gap-4">
          <DialogHeader>
            <DialogTitle>Remove {model ? modelDisplayName(model.ref) : ''}?</DialogTitle>
            <DialogDescription>
              Devices serving it stop, and requests for it fail. Its usage history is kept, and you can
              add it again later.
            </DialogDescription>
          </DialogHeader>
          {error && <p role="alert" className="text-[13.5px] text-[var(--n-text-2)]">{error}</p>}
          <DialogFooter>
            <GhostButton onClick={close}>Cancel</GhostButton>
            <DangerButton onClick={remove} disabled={busy}>{busy ? 'Removing…' : 'Remove model'}</DangerButton>
          </DialogFooter>
        </div>
      </DialogContent>
    </Dialog>
  )
}
