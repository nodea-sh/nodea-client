import { useEffect, useState } from 'react'
import { Link, useNavigate, useParams } from 'react-router-dom'
import { apiClient } from '@/lib/api'
import { useAuth } from '@/contexts/AuthContext'
import { validatePassword } from '@/lib/validation'

type LinkState = 'checking' | 'valid' | 'invalid'

// Read by LoginPage to say the password changed and prefill the email.
export interface PasswordResetLoginState {
  passwordReset: true
  email: string
}

const inputClass =
  'rounded-[8px] border border-[var(--n-border)] bg-[var(--n-sidebar)] px-3 py-2.5 text-[15px] leading-[1.2] text-[var(--n-text)] outline-none placeholder:text-[var(--n-text-4)] focus:border-[var(--n-accent-800)]'

/* Opened from the emailed nodea:///reset-password/<token> link (see
 * DeepLinkListener). Same layout as LoginPage. */
export function ResetPasswordPage() {
  const { token } = useParams<{ token: string }>()
  const navigate = useNavigate()
  const { isAuthenticated, logout } = useAuth()

  const [linkState, setLinkState] = useState<LinkState>('checking')
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const [confirm, setConfirm] = useState('')
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState('')

  // Checking does not use the link up, so a second call (StrictMode) is harmless.
  useEffect(() => {
    if (!token) {
      setLinkState('invalid')
      return
    }

    let cancelled = false
    apiClient
      .checkPasswordReset(token)
      .then(({ email }) => {
        if (cancelled) return
        setEmail(email)
        setLinkState('valid')
      })
      .catch(() => {
        if (!cancelled) setLinkState('invalid')
      })
    return () => {
      cancelled = true
    }
  }, [token])

  const submit = async (e: React.FormEvent) => {
    e.preventDefault()
    setError('')

    const passwordError = validatePassword(password)
    if (passwordError) {
      setError(passwordError)
      return
    }
    if (password !== confirm) {
      setError('Passwords do not match')
      return
    }

    setSaving(true)
    try {
      const result = await apiClient.resetPassword(token!, password)
      // The reset signed the account out everywhere, this app included.
      if (isAuthenticated) await logout().catch(() => {})
      const state: PasswordResetLoginState = { passwordReset: true, email: result.email }
      navigate('/login', { replace: true, state })
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Could not reset your password')
      setSaving(false)
    }
  }

  return (
    <div className="flex h-screen flex-col justify-center bg-[var(--n-bg)] px-[54px]">
      <div className="n-drag absolute left-0 right-0 top-0 h-[46px]" data-tauri-drag-region />

      <span className="flex h-[34px] w-[34px] items-center justify-center rounded-[10px] bg-[var(--n-accent)] text-[16px] font-semibold leading-none text-[var(--n-bg)]">
        N
      </span>

      <h2 className="mb-1.5 mt-[22px] text-[25px] font-semibold leading-[1.25] tracking-[-0.02em] text-[var(--n-text)]">
        {linkState === 'invalid' ? 'Link expired or invalid' : 'Choose a new password'}
      </h2>

      {linkState === 'checking' && (
        <p className="max-w-[360px] text-[14.5px] leading-[1.5] text-[var(--n-text-3)]">Checking your link…</p>
      )}

      {linkState === 'invalid' && (
        <>
          <p className="mb-[22px] max-w-[360px] text-[14.5px] leading-[1.5] text-[var(--n-text-3)]">
            This reset link has expired, was already used, or a newer one was sent.
          </p>
          <Link to="/forgot-password" className="text-[13.5px] font-medium text-[var(--n-accent-400)] hover:text-[var(--n-accent-300)]">
            Send a new link
          </Link>
        </>
      )}

      {linkState === 'valid' && (
        <>
          <p className="mb-[22px] max-w-[360px] text-[14.5px] leading-[1.5] text-[var(--n-text-3)]">
            For <strong className="text-[var(--n-text-2)]">{email}</strong>. You'll be signed out everywhere
            and can sign in with it straight away.
          </p>

          <form onSubmit={submit} className="flex max-w-[360px] flex-col gap-2">
            {error && <p role="alert" className="text-[13.5px] leading-[1.4] text-[var(--n-text-2)]">{error}</p>}

            <input id="new-password" type="password" value={password} aria-label="New password"
                   onChange={(e) => setPassword(e.target.value)}
                   required autoFocus autoComplete="new-password"
                   placeholder="New password"
                   className={inputClass} />
            <input id="confirm-password" type="password" value={confirm} aria-label="Confirm new password"
                   onChange={(e) => setConfirm(e.target.value)}
                   required autoComplete="new-password"
                   placeholder="Confirm new password"
                   className={inputClass} />
            <p className="text-[12.5px] leading-[1.4] text-[var(--n-text-4)]">At least 8 characters, with a letter and a digit.</p>

            <button type="submit" disabled={saving}
                    className="mt-1.5 rounded-[8px] border border-[var(--n-accent)] bg-transparent px-3 py-2.5 text-[15px] font-semibold leading-[1.2] text-[var(--n-accent-300)] transition-colors hover:bg-[rgba(145,132,217,.14)] disabled:opacity-45">
              {saving ? 'Saving…' : 'Set new password'}
            </button>
          </form>
        </>
      )}
    </div>
  )
}
