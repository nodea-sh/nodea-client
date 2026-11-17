import { useState } from 'react'
import { Link, useLocation } from 'react-router-dom'
import { apiClient } from '@/lib/api'

/* Same layout as LoginPage, which links here with the email typed there. */
export function ForgotPasswordPage() {
  const location = useLocation()
  const [email, setEmail] = useState<string>((location.state as { email?: string } | null)?.email ?? '')
  const [sending, setSending] = useState(false)
  const [sent, setSent] = useState(false)
  const [error, setError] = useState('')

  const submit = async (e: React.FormEvent) => {
    e.preventDefault()
    setError('')
    setSending(true)
    try {
      await apiClient.requestPasswordReset(email)
      setSent(true)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Could not send a reset link')
    } finally {
      setSending(false)
    }
  }

  return (
    <div className="flex h-screen flex-col justify-center bg-[var(--n-bg)] px-[54px]">
      <div className="n-drag absolute left-0 right-0 top-0 h-[46px]" data-tauri-drag-region />

      <span className="flex h-[34px] w-[34px] items-center justify-center rounded-[10px] bg-[var(--n-accent)] text-[16px] font-semibold leading-none text-[var(--n-bg)]">
        N
      </span>

      <h2 className="mb-1.5 mt-[22px] text-[25px] font-semibold leading-[1.25] tracking-[-0.02em] text-[var(--n-text)]">
        Reset your password
      </h2>

      {sent ? (
        <>
          {/* The backend answers the same for any address, so neither can we. */}
          <p role="status" className="mb-[22px] max-w-[360px] text-[14.5px] leading-[1.5] text-[var(--n-text-3)]">
            If an account exists for <strong className="text-[var(--n-text-2)]">{email}</strong>, a link to
            reset its password is on its way. Open it on this computer to come back to the app. It works for
            one hour.
          </p>
          <Link to="/login" className="text-[13.5px] font-medium text-[var(--n-accent-400)] hover:text-[var(--n-accent-300)]">
            Back to sign in
          </Link>
        </>
      ) : (
        <>
          <p className="mb-[22px] max-w-[360px] text-[14.5px] leading-[1.5] text-[var(--n-text-3)]">
            Enter the email of your account and we'll send you a link to choose a new password.
          </p>

          <form onSubmit={submit} className="flex max-w-[360px] flex-col gap-2">
            {error && <p role="alert" className="text-[13.5px] leading-[1.4] text-[var(--n-text-2)]">{error}</p>}

            <input id="email" type="email" value={email}
                   onChange={(e) => setEmail(e.target.value)}
                   required autoFocus
                   placeholder="you@company.com"
                   className="rounded-[8px] border border-[var(--n-border)] bg-[var(--n-sidebar)] px-3 py-2.5 text-[15px] leading-[1.2] text-[var(--n-text)] outline-none placeholder:text-[var(--n-text-4)] focus:border-[var(--n-accent-800)]" />

            <button type="submit" disabled={sending || !email}
                    className="mt-1.5 rounded-[8px] border border-[var(--n-accent)] bg-transparent px-3 py-2.5 text-[15px] font-semibold leading-[1.2] text-[var(--n-accent-300)] transition-colors hover:bg-[rgba(145,132,217,.14)] disabled:opacity-45">
              {sending ? 'Sending…' : 'Send reset link'}
            </button>

            <p className="mt-1 text-[13.5px] leading-none text-[var(--n-text-3)]">
              <Link to="/login" className="font-medium text-[var(--n-accent-400)] hover:text-[var(--n-accent-300)]">
                Back to sign in
              </Link>
            </p>
          </form>
        </>
      )}
    </div>
  )
}
