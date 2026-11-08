import type { CatalogModel } from './api'

/* Picks the models to suggest for this Mac out of the backend's catalog.
 *
 * The catalog (GET /v1/model-catalog) has no file size - the backend only
 * prices a model's memory when it is first added - so the size is estimated
 * from the parameter count in the repo name ("Llama-3.2-3B-Instruct" -> 3B),
 * at the ~4.85 bits per weight of the Q4 quantisations the catalog prefers,
 * plus the same 20% + 512 MB headroom the backend adds
 * (services/models_catalog.py). A name without a parameter count is never
 * recommended, only listed. */

const GB_PER_BILLION_PARAMS_Q4 = 0.61

export function parameterCountB(name: string): number | null {
  // "30B-A3B" (mixture of experts) matches 30B first: priced by its total
  // size, which is what has to fit in memory.
  const match = /(?:^|[-_.])(\d+(?:\.\d+)?)[Bb](?=$|[-_.:])/.exec(name)
  return match ? Number(match[1]) : null
}

export function estimateMemoryMb(name: string): number | null {
  const params = parameterCountB(name)
  if (params === null) return null
  return Math.ceil(params * GB_PER_BILLION_PARAMS_Q4 * 1024 * 1.2) + 512
}

/* "bartowski/Llama-3.2-3B-Instruct-GGUF:Q4_K_M" -> "Llama 3.2 3B Instruct".
 * Registry keys from older orgs ("llama-3.2-1b") pass through unchanged. */
export function modelDisplayName(ref: string): string {
  const repo = ref.split(':')[0]
  const name = repo.includes('/') ? repo.split('/').slice(1).join('/') : repo
  return name.replace(/-GGUF$/i, '').replace(/-/g, ' ')
}

export function formatMemory(mb: number): string {
  return `${(mb / 1024).toFixed(1)} GB`
}

export function modelTagline(name: string): string {
  if (/coder|code/i.test(name)) return 'Good for code'
  if (/vl|vision/i.test(name)) return 'Text and images'
  return 'Good for chat'
}

// Chat-tuned models answer an OpenAI chat request sensibly; base models
// don't, so only these are suggested to someone setting up.
const INSTRUCT = /instruct|[-_]it(?:$|[-_])|chat/i

export interface Recommendation {
  model: CatalogModel
  memoryMb: number
}

/**
 * Up to `limit` catalog models that fit in `budgetMb` of memory and are not
 * already in the organization: chat-tuned first, then the most downloaded.
 * When nothing fits, the smallest estimable model is offered alone so the
 * screen is never empty.
 */
export function recommend(catalog: CatalogModel[], budgetMb: number, limit = 3): Recommendation[] {
  const candidates = catalog
    .filter((m) => !m.in_organization)
    .map((model) => ({ model, memoryMb: estimateMemoryMb(model.hf_repo) }))
    .filter((r): r is Recommendation => r.memoryMb !== null)

  const fitting = candidates
    .filter((r) => r.memoryMb <= budgetMb)
    .sort((a, b) => {
      const chat = Number(INSTRUCT.test(b.model.hf_repo)) - Number(INSTRUCT.test(a.model.hf_repo))
      return chat || b.model.downloads - a.model.downloads
    })

  if (fitting.length > 0) return fitting.slice(0, limit)

  const smallest = [...candidates].sort((a, b) => a.memoryMb - b.memoryMb)[0]
  return smallest ? [smallest] : []
}
