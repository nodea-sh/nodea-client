import { useEffect, useMemo, useState } from 'react'
import { Navigate, useNavigate } from 'react-router-dom'
import { Check } from '@phosphor-icons/react'
import { toast } from 'sonner'
import { AddModelsDialog, useCatalog } from '@/components/AddModelsDialog'
import { GhostButton, PrimaryButton } from '@/components/nocturne'
import { useAuth } from '@/contexts/AuthContext'
import { useMemoryBudget } from '@/hooks/useMemoryBudget'
import { apiClient } from '@/lib/api'
import { formatMemory, modelDisplayName, modelTagline, recommend } from '@/lib/recommendedModels'

/* Step 2 of setting up an organization: a new organization has no models,
 * so its Macs have nothing to serve until an admin adds one. Offer a few that
 * fit this Mac and add the chosen one in a click. */
export function OnboardingModelPage() {
  const { organization, isAdmin } = useAuth()
  const navigate = useNavigate()
  const budget = useMemoryBudget()
  const { catalog, error, retry } = useCatalog(true)
  const [hasModels, setHasModels] = useState<boolean | null>(null)
  const [selected, setSelected] = useState<string | null>(null)
  const [adding, setAdding] = useState(false)
  const [browsing, setBrowsing] = useState(false)

  useEffect(() => {
    apiClient.getOrgModels()
      .then((models) => setHasModels(models.length > 0))
      .catch(() => setHasModels(false))
  }, [])

  const recommended = useMemo(
    () => (catalog && budget !== null ? recommend(catalog, budget) : []),
    [catalog, budget]
  )

  // Preselect the first suggestion, so "Add and start" works in one click.
  useEffect(() => {
    if (selected === null && recommended.length > 0) setSelected(recommended[0].model.ref)
  }, [recommended, selected])

  // Nothing to set up: a member can't add models, and an organization that
  // already has one doesn't need this step.
  if (!isAdmin || hasModels) return <Navigate to="/node" replace />

  const start = () => navigate('/node', { replace: true })

  const addSelected = async () => {
    if (!selected) return
    setAdding(true)
    try {
      await apiClient.addModel(selected)
      toast.success(`Added ${modelDisplayName(selected)}`)
      start()
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Could not add the model')
      setAdding(false)
    }
  }

  return (
    <div className="flex min-h-screen items-center justify-center bg-[var(--n-bg)] p-6">
      <div className="n-drag fixed inset-x-0 top-0 h-9" data-tauri-drag-region />
      <div className="w-full max-w-[560px]">
        <div className="text-[13px] font-semibold uppercase tracking-[0.09em] text-[var(--n-accent-400)]">
          Step 2 of 2{organization ? ` · ${organization.name}` : ''}
        </div>
        <h1 className="mt-3 text-[30px] font-semibold leading-[1.15] tracking-[-0.02em] text-[var(--n-text)]">
          Pick a model for your organization
        </h1>
        <p className="mt-2.5 text-[15px] leading-[1.6] text-[var(--n-text-4)]">
          Your devices serve the models you add. These fit this device's memory. You can add more, or
          change them, any time from Models.
        </p>

        <div className="mt-6">
          {error ? (
            <div className="rounded-[12px] border border-[var(--n-line)] bg-[var(--n-sidebar)] p-5">
              <p role="alert" className="text-[14px] leading-[1.5] text-[var(--n-text-2)]">{error}</p>
              <div className="mt-3"><GhostButton onClick={retry}>Try again</GhostButton></div>
            </div>
          ) : catalog === null || budget === null || hasModels === null ? (
            <p className="text-[14px] text-[var(--n-text-4)]">Finding models that fit this device…</p>
          ) : (
            <div role="radiogroup" aria-label="Recommended models" className="grid gap-2">
              {recommended.map(({ model, memoryMb }) => {
                const on = selected === model.ref
                return (
                  <button key={model.ref} role="radio" aria-checked={on} onClick={() => setSelected(model.ref)}
                          className="flex items-center gap-3.5 rounded-[12px] border bg-[var(--n-sidebar)] px-4 py-3.5 text-left transition-colors hover:bg-[var(--n-tint-hover)]"
                          style={{ borderColor: on ? 'var(--n-accent-600)' : 'var(--n-line)' }}>
                    <span className="flex h-[18px] w-[18px] flex-none items-center justify-center rounded-full border"
                          style={{ borderColor: on ? 'var(--n-accent-400)' : 'var(--n-mute-2)', background: on ? 'var(--n-accent-700)' : 'transparent' }}>
                      {on && <Check size={11} weight="bold" color="var(--n-accent-100)" />}
                    </span>
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-[15.5px] font-semibold leading-[1.3] text-[var(--n-text)]">
                        {modelDisplayName(model.ref)}
                      </span>
                      <span className="mt-[3px] block text-[13.5px] leading-[1.3] text-[var(--n-text-3)]">
                        {modelTagline(model.hf_repo)} · {model.publisher}
                      </span>
                    </span>
                    <span className="font-mono text-[13.5px] text-[var(--n-text-2)]">~{formatMemory(memoryMb)}</span>
                  </button>
                )
              })}
            </div>
          )}
        </div>

        <div className="mt-6 flex items-center justify-between gap-3">
          <div className="flex items-center gap-4">
            <button onClick={() => setBrowsing(true)} disabled={catalog === null}
                    className="text-[14px] font-semibold text-[var(--n-accent-400)] hover:text-[var(--n-accent-300)] disabled:opacity-40">
              Browse all models
            </button>
            <button onClick={start} className="text-[14px] font-medium text-[var(--n-text-4)] hover:text-[var(--n-text-2)]">
              Skip for now
            </button>
          </div>
          <PrimaryButton onClick={addSelected} disabled={!selected || adding}>
            {adding ? 'Adding…' : 'Add and start'}
          </PrimaryButton>
        </div>
      </div>

      <AddModelsDialog open={browsing} onOpenChange={setBrowsing}
                       onAdded={(m) => {
                         toast.success(`Added ${modelDisplayName(m.ref)}`)
                         start()
                       }} />
    </div>
  )
}
