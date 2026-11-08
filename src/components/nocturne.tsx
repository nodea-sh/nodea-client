/* Buttons and page chrome shared by the Models, API keys and onboarding
 * screens, in the Nocturne look (src/nodea-theme.css). */

type ButtonProps = {
  children: React.ReactNode
  onClick?: () => void
  disabled?: boolean
  type?: 'button' | 'submit'
  'aria-label'?: string
}

export function PrimaryButton({ children, onClick, disabled, type = 'button', ...rest }: ButtonProps) {
  return (
    <button type={type} onClick={onClick} disabled={disabled} aria-label={rest['aria-label']}
      className="flex-none rounded-[8px] bg-[var(--n-accent-700)] px-[14px] py-[8px] text-[14px] font-semibold leading-none text-[var(--n-accent-100)] transition-colors hover:bg-[var(--n-accent-600)] disabled:opacity-40 disabled:hover:bg-[var(--n-accent-700)]">
      {children}
    </button>
  )
}

export function GhostButton({ children, onClick, disabled, type = 'button', ...rest }: ButtonProps) {
  return (
    <button type={type} onClick={onClick} disabled={disabled} aria-label={rest['aria-label']}
      className="flex-none rounded-[8px] border border-[var(--n-mute-4)] px-[13px] py-[7px] text-[14px] font-semibold leading-none text-[var(--n-text-4)] transition-colors hover:border-[var(--n-mute-2)] hover:text-[var(--n-text-2)] disabled:opacity-40">
      {children}
    </button>
  )
}

export function DangerButton({ children, onClick, disabled, type = 'button' }: ButtonProps) {
  return (
    <button type={type} onClick={onClick} disabled={disabled}
      className="rounded-[8px] bg-[var(--n-danger,#b42318)] px-[13px] py-[7px] text-[14px] font-semibold leading-none text-white disabled:opacity-40">
      {children}
    </button>
  )
}

export function PageHeader({ title, sub, action }: { title: string; sub?: string; action?: React.ReactNode }) {
  return (
    <div className="mb-[22px] flex items-start justify-between gap-6">
      <div>
        <h1 className="text-[26px] font-semibold leading-[1.2] tracking-[-0.02em] text-[var(--n-text)]">{title}</h1>
        {sub && <p className="mt-1.5 max-w-[520px] text-[14.5px] leading-[1.5] text-[var(--n-text-4)]">{sub}</p>}
      </div>
      {action}
    </div>
  )
}

/* Explains why a list is empty and what to do next. */
export function EmptyState({ title, body, action }: { title: string; body: string; action?: React.ReactNode }) {
  return (
    <div className="rounded-[12px] border border-dashed border-[var(--n-border)] bg-[var(--n-sidebar)] px-6 py-10 text-center">
      <div className="text-[16px] font-semibold leading-[1.3] text-[var(--n-text)]">{title}</div>
      <p className="mx-auto mt-2 max-w-[420px] text-[14px] leading-[1.55] text-[var(--n-text-4)]">{body}</p>
      {action && <div className="mt-5 flex justify-center">{action}</div>}
    </div>
  )
}

/* Shown to members in place of an admin-only control, so the missing button
 * is explained instead of just absent. */
export function AdminOnlyNote({ children }: { children: React.ReactNode }) {
  return (
    <p className="mb-4 rounded-[10px] border border-[var(--n-line)] bg-[var(--n-sidebar)] px-4 py-3 text-[14px] leading-[1.5] text-[var(--n-text-4)]">
      {children}
    </p>
  )
}
