import { useEffect, useMemo, useState } from 'react'
import { MagnifyingGlass } from '@phosphor-icons/react'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { GhostButton, PrimaryButton } from '@/components/nocturne'
import { useMemoryBudget } from '@/hooks/useMemoryBudget'
import { apiClient, type CatalogModel, type OrgModel } from '@/lib/api'
import {
  estimateMemoryMb,
  formatMemory,
  modelDisplayName,
  modelTagline,
  recommend,
} from '@/lib/recommendedModels'

/* Load the catalog once per open. A 502 means Hugging Face is down and the
 * backend has nothing cached. */
export function useCatalog(enabled: boolean) {
  const [catalog, setCatalog] = useState<CatalogModel[] | null>(null)
  const [stale, setStale] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [attempt, setAttempt] = useState(0)

  useEffect(() => {
    if (!enabled) return
    let cancelled = false
    setError(null)
    apiClient.getModelCatalog()
      .then((c) => {
        if (cancelled) return
        setCatalog(c.items)
        setStale(c.stale)
      })
      .catch((err) => {
        if (cancelled) return
        setError(err?.status === 502
          ? "Hugging Face is unreachable, so the catalog can't load. Try again in a moment."
          : err instanceof Error ? err.message : 'Could not load the catalog')
      })
    return () => { cancelled = true }
  }, [enabled, attempt])

  return { catalog, stale, error, retry: () => setAttempt((a) => a + 1), setCatalog }
}

export function AddModelsDialog({
  open, onOpenChange, onAdded,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  onAdded: (model: OrgModel) => void
}) {
  const budget = useMemoryBudget()
  const { catalog, stale, error, retry, setCatalog } = useCatalog(open)
  const [query, setQuery] = useState('')
  const [adding, setAdding] = useState<string | null>(null)
  const [addError, setAddError] = useState<string | null>(null)

  // Chosen once per open: a model just added stays in the list, marked
  // "Added", instead of disappearing from under the cursor.
  const [recommendedRefs, setRecommendedRefs] = useState<string[] | null>(null)
  useEffect(() => {
    if (!open) setRecommendedRefs(null)
    else if (recommendedRefs === null && catalog && budget !== null) {
      setRecommendedRefs(recommend(catalog, budget).map((r) => r.model.ref))
    }
  }, [open, catalog, budget, recommendedRefs])
  const recommended = useMemo(
    () => (recommendedRefs ?? []).flatMap((ref) => catalog?.filter((m) => m.ref === ref) ?? []),
    [recommendedRefs, catalog]
  )

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase()
    return (catalog ?? [])
      .filter((m) => !q || m.hf_repo.toLowerCase().includes(q))
      .sort((a, b) => b.downloads - a.downloads)
  }, [catalog, query])

  const add = async (ref: string) => {
    setAdding(ref)
    setAddError(null)
    try {
      const model = await apiClient.addModel(ref)
      setCatalog((prev) => prev?.map((m) => (m.ref === ref ? { ...m, in_organization: true } : m)) ?? prev)
      onAdded(model)
    } catch (err) {
      setAddError(err instanceof Error ? err.message : 'Could not add the model')
    } finally {
      setAdding(null)
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-[640px]">
        <DialogHeader>
          <DialogTitle>Add models</DialogTitle>
          <DialogDescription>
            Open GGUF models from Hugging Face, about 4 bits per weight. Added models start enabled,
            and your devices pick them up on their own.
          </DialogDescription>
        </DialogHeader>

        {error ? (
          <div className="grid gap-3 py-4">
            <p role="alert" className="text-[14px] text-[var(--n-text-2)]">{error}</p>
            <div><GhostButton onClick={retry}>Try again</GhostButton></div>
          </div>
        ) : catalog === null ? (
          <p className="py-6 text-[14px] text-[var(--n-text-4)]">Loading the catalog…</p>
        ) : (
          <div className="grid gap-4">
            {stale && (
              <p className="text-[13px] text-[var(--n-text-4)]">
                Hugging Face is slow to answer, so this list may be a few hours old.
              </p>
            )}
            {addError && <p role="alert" className="text-[13.5px] text-[var(--n-text-2)]">{addError}</p>}

            {!query && recommended.length > 0 && (
              <section aria-label="Recommended for this device">
                <h3 className="mb-2 text-[12.5px] font-semibold uppercase tracking-[0.06em] text-[var(--n-text-3)]">
                  Recommended for this device
                </h3>
                <div className="grid gap-1.5">
                  {recommended.map((model) => (
                    <CatalogRow key={model.ref} model={model} adding={adding} onAdd={add} />
                  ))}
                </div>
              </section>
            )}

            <div className="relative">
              <MagnifyingGlass size={15} className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-[var(--n-text-4)]" />
              <Input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search models"
                     aria-label="Search models" className="pl-9" />
            </div>

            <section aria-label="All models" className="max-h-[320px] overflow-y-auto pr-1">
              <div className="grid gap-1.5">
                {filtered.map((model) => (
                  <CatalogRow key={model.ref} model={model} adding={adding} onAdd={add} />
                ))}
                {filtered.length === 0 && (
                  <p className="py-4 text-[14px] text-[var(--n-text-4)]">No models match "{query}".</p>
                )}
              </div>
            </section>
          </div>
        )}
      </DialogContent>
    </Dialog>
  )
}

function CatalogRow({
  model, adding, onAdd,
}: {
  model: CatalogModel
  adding: string | null
  onAdd: (ref: string) => void
}) {
  const memory = estimateMemoryMb(model.hf_repo)
  const name = modelDisplayName(model.ref)
  return (
    <div data-testid="catalog-row"
         className="flex items-center gap-3 rounded-[10px] border border-[var(--n-line)] bg-[var(--n-sidebar)] px-3.5 py-2.5">
      <span className="min-w-0 flex-1">
        <span className="block truncate text-[14.5px] font-semibold leading-[1.3] text-[var(--n-text)]">{name}</span>
        <span className="mt-0.5 block truncate text-[12.5px] leading-[1.3] text-[var(--n-text-3)]">
          {modelTagline(model.hf_repo)} · {model.publisher} · {model.quantization}
          {memory !== null && ` · ~${formatMemory(memory)}`}
        </span>
      </span>
      {model.in_organization ? (
        <span className="text-[13.5px] font-medium text-[var(--n-accent-300)]">Added</span>
      ) : (
        <PrimaryButton onClick={() => onAdd(model.ref)} disabled={adding !== null} aria-label={`Add ${name}`}>
          {adding === model.ref ? 'Adding…' : 'Add'}
        </PrimaryButton>
      )}
    </div>
  )
}
