import { useEffect, useState } from 'react'
import { Check, CheckCircle, CircleDashed, CircleNotch, Copy, X } from '@phosphor-icons/react'
import { AddModelsDialog } from '@/components/AddModelsDialog'
import { CreateApiKeyDialog } from '@/components/CreateApiKeyDialog'
import { GhostButton, PrimaryButton } from '@/components/nocturne'
import { useAuth } from '@/contexts/AuthContext'
import { announceModelsChanged, type NodeState } from '@/hooks/useNodePhase'
import { apiClient, OPENAI_API_BASE_URL, type OnboardingStatus } from '@/lib/api'
import { SNIPPET_LANGUAGES, buildSnippet, maskApiKey, type SnippetLanguage } from '@/lib/apiSnippets'

const POLL_MS = 3000

type StepState = 'done' | 'current' | 'todo'

/* "Get set up" card on the Node page. Every tick comes from the server
 * (GET /v1/onboarding/status, the same steps as the web console's Get
 * started page) or from the node itself - never from a click - so it can't
 * claim something happened that didn't. */
export function SetupChecklist({
  node, onVisibleChange,
}: {
  node: NodeState
  /** Told whether the checklist is on screen, so the page can drop what it repeats. */
  onVisibleChange?: (visible: boolean) => void
}) {
  const { isAdmin } = useAuth()
  const [status, setStatus] = useState<OnboardingStatus | null>(null)
  // Shown when the checklist finishes while open, instead of vanishing.
  const [finishedHere, setFinishedHere] = useState(false)
  const [creatingKey, setCreatingKey] = useState(false)
  const [addingModel, setAddingModel] = useState(false)
  // The new key's secret, kept in memory only, so the example runs as pasted.
  const [createdKey, setCreatedKey] = useState<string | null>(null)

  useEffect(() => {
    let stopped = false
    let wasOpen = false
    let timer: ReturnType<typeof setTimeout> | undefined
    const poll = async () => {
      try {
        const s = await apiClient.getOnboardingStatus()
        if (stopped) return
        setStatus(s)
        if (!s.completed && !s.dismissed) wasOpen = true
        if (s.completed && wasOpen) setFinishedHere(true)
        if (s.completed || s.dismissed) return
      } catch (err) {
        console.warn('[Setup] status check failed', err)
      }
      if (!stopped) timer = setTimeout(poll, POLL_MS)
    }
    poll()
    return () => {
      stopped = true
      clearTimeout(timer)
    }
  }, [])

  const visible = !!status && !status.dismissed && !(status.completed && !finishedHere)
  useEffect(() => {
    onVisibleChange?.(visible)
  }, [visible, onVisibleChange])

  if (!status || !visible) return null

  const dismiss = () => {
    setStatus({ ...status, dismissed: true })
    apiClient.updateOnboarding({ dismissed: true }).catch(console.error)
  }

  if (status.completed) {
    return (
      <Card onDismiss={dismiss} title="You're all set">
        <p className="px-[18px] pb-4 text-[14.5px] leading-[1.55] text-[var(--n-text-3)]">
          This device serves your organization and your first request went through. Keys and models
          live under API keys and Models in the sidebar.
        </p>
      </Card>
    )
  }

  const modelDone = node.orgHasModels === true
  const connected = status.app_installed
  const keyDone = status.api_key_created || !status.api_key_step_applicable
  const steps: { id: string; title: string; sub: string; state: StepState; body?: React.ReactNode }[] = []

  // A step is actionable once its own prerequisite is met, not only when
  // every step above it is done: a member can't add the model, and a request
  // only needs a key.
  const stateOf = (done: boolean, ready = true): StepState => (done ? 'done' : ready ? 'current' : 'todo')

  steps.push({
    id: 'model',
    title: isAdmin ? 'Add a model' : 'An admin adds a model',
    sub: modelDone
      ? 'Your organization has a model to serve.'
      : isAdmin ? 'Pick one that fits your devices.' : "This device starts serving on its own once there's one.",
    state: stateOf(modelDone),
    body: !modelDone && isAdmin && <PrimaryButton onClick={() => setAddingModel(true)}>Add a model</PrimaryButton>,
  })
  steps.push({
    id: 'node',
    title: 'Connect this device',
    sub: connected ? 'This device has joined your organization.' : 'Signing in here connects it. It takes a few seconds.',
    state: stateOf(connected),
  })
  steps.push({
    id: 'key',
    title: 'Create an API key',
    sub: !status.api_key_step_applicable
      ? 'Only admins create keys. Ask one for a key to call the API.'
      : status.api_key_created ? 'Your tools can now call the API.' : 'Keys let your tools and scripts call your models.',
    state: stateOf(keyDone),
    body: status.api_key_step_applicable && !status.api_key_created &&
      <PrimaryButton onClick={() => setCreatingKey(true)}>Create key</PrimaryButton>,
  })
  steps.push({
    id: 'request',
    title: 'Make your first request',
    sub: status.first_request_made
      ? 'A request reached your organization.'
      : 'The API speaks the OpenAI chat format, so OpenAI SDKs work too.',
    state: stateOf(status.first_request_made, keyDone),
    body: !status.first_request_made && keyDone && (
      <FirstRequest model={node.orgModelRef} apiKey={createdKey}
                    onConfirm={isAdmin ? undefined : async () => setStatus(await apiClient.updateOnboarding({ request_confirmed: true }))} />
    ),
  })

  const done = steps.filter((s) => s.state === 'done').length

  return (
    <Card onDismiss={dismiss} title="Get set up" meta={`${done} of ${steps.length} done`}>
      {steps.map((s) => (
        <div key={s.id} data-testid={`setup-step-${s.id}`} data-state={s.state}
             className="border-t border-[var(--n-line)] px-[18px] py-3.5">
          <div className="flex items-start gap-3">
            <span className="mt-px">
              {s.state === 'done' && <CheckCircle size={17} color="var(--n-accent-400)" />}
              {s.state === 'current' && <CircleNotch size={17} color="var(--n-accent-400)" className={s.id === 'node' ? 'n-spin' : undefined} />}
              {s.state === 'todo' && <CircleDashed size={17} color="var(--n-mute)" />}
            </span>
            <span className="min-w-0 flex-1">
              <span className="block text-[15px] font-semibold leading-[1.3]"
                    style={{ color: s.state === 'todo' ? 'var(--n-text-4)' : s.state === 'done' ? 'var(--n-text-2)' : 'var(--n-text)' }}>
                {s.title}
              </span>
              <span className="mt-[3px] block text-[13.5px] leading-[1.4] text-[var(--n-text-3)]">{s.sub}</span>
              {s.state === 'current' && s.body && <div className="mt-3">{s.body}</div>}
            </span>
          </div>
        </div>
      ))}

      <AddModelsDialog open={addingModel} onOpenChange={setAddingModel}
                       onAdded={() => {
                         setAddingModel(false)
                         announceModelsChanged()
                       }} />

      <CreateApiKeyDialog open={creatingKey} onOpenChange={setCreatingKey}
                          onCreated={(k) => {
                            setCreatedKey(k.value)
                            setStatus((prev) => (prev ? { ...prev, api_key_created: true } : prev))
                          }} />
    </Card>
  )
}

function Card({ title, meta, onDismiss, children }: { title: string; meta?: string; onDismiss: () => void; children: React.ReactNode }) {
  return (
    <section aria-label="Setup checklist"
             className="mt-[26px] max-w-[680px] overflow-hidden rounded-[12px] border border-[var(--n-line)] bg-[var(--n-sidebar)]">
      <div className="flex items-center justify-between px-[18px] py-3">
        <span className="text-[15px] font-semibold text-[var(--n-text)]">
          {title}
          {meta && <span className="ml-2 text-[13.5px] font-normal text-[var(--n-text-4)]">{meta}</span>}
        </span>
        <button onClick={onDismiss} aria-label="Dismiss setup checklist"
                className="flex h-6 w-6 items-center justify-center rounded-[6px] text-[var(--n-text-4)] transition-colors hover:bg-[var(--n-tint-hover)] hover:text-[var(--n-text-2)]">
          <X size={13} />
        </button>
      </div>
      {children}
    </section>
  )
}

function FirstRequest({
  model, apiKey, onConfirm,
}: {
  model: string | null
  apiKey: string | null
  onConfirm?: () => Promise<void>
}) {
  const [language, setLanguage] = useState<SnippetLanguage>('shell')
  const [copied, setCopied] = useState(false)
  const [confirming, setConfirming] = useState(false)
  const [served, setServed] = useState<string | null>(null)

  // A model some device serves right now answers the example; an enabled one
  // that hasn't loaded yet would fail it.
  useEffect(() => {
    apiClient.getAvailableModels()
      .then((models) => setServed(models[0] ?? null))
      .catch(() => setServed(null))
  }, [model])

  const modelRef = served ?? model ?? '<model>'

  const shown = buildSnippet(language, OPENAI_API_BASE_URL, modelRef, apiKey && maskApiKey(apiKey))
  const full = buildSnippet(language, OPENAI_API_BASE_URL, modelRef, apiKey)

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(full)
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    } catch {
      setCopied(false)
    }
  }

  return (
    <div className="grid gap-2.5">
      {apiKey && (
        <p className="text-[13px] leading-[1.45] text-[var(--n-text-4)]">
          This uses the key you just created. It's hidden here; copying takes the real key with it.
        </p>
      )}
      <div className="overflow-hidden rounded-[10px] border border-[var(--n-border)] bg-[var(--n-bg)]">
        <div className="flex items-center justify-between border-b border-[var(--n-line)] px-2 py-1.5">
          <div role="tablist" aria-label="Language" className="flex gap-1">
            {SNIPPET_LANGUAGES.map((l) => (
              <button key={l.id} role="tab" aria-selected={language === l.id} onClick={() => setLanguage(l.id)}
                      className="rounded-[6px] px-2.5 py-1 text-[13px] font-semibold"
                      style={{
                        background: language === l.id ? 'var(--n-tint-active)' : 'transparent',
                        color: language === l.id ? 'var(--n-accent-200)' : 'var(--n-text-4)',
                      }}>
                {l.label}
              </button>
            ))}
          </div>
          <button onClick={copy} aria-label="Copy example"
                  className="flex items-center gap-1.5 rounded-[6px] px-2 py-1 text-[13px] font-semibold text-[var(--n-text-3)] hover:text-[var(--n-text)]">
            {copied ? <Check size={13} /> : <Copy size={13} />}
            {copied ? 'Copied' : 'Copy'}
          </button>
        </div>
        <pre data-testid="first-request-snippet"
             className="overflow-x-auto px-3.5 py-3 font-mono text-[12.5px] leading-[1.55] text-[var(--n-text-2)]">{shown}</pre>
      </div>
      <div className="flex items-center gap-2 text-[13.5px] text-[var(--n-text-3)]">
        <CircleNotch size={14} className="n-spin" color="var(--n-accent-400)" />
        Waiting for your first request…
        {onConfirm && (
          <span className="ml-auto">
            <GhostButton disabled={confirming}
                         onClick={async () => { setConfirming(true); await onConfirm().catch(console.error); setConfirming(false) }}>
              I've made a request
            </GhostButton>
          </span>
        )}
      </div>
    </div>
  )
}
