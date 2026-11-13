import { useState, useEffect, useRef } from 'react'
import { Link, useNavigate, useSearchParams } from 'react-router-dom'
import { useAuth } from '@/contexts/AuthContext'
import { apiClient } from '@/lib/api'

// Only accounts created before signups could log in unconfirmed still get
// this from Supabase; confirming (or resending) the link unblocks them.
function isEmailNotConfirmedError(message: string): boolean {
  return message.toLowerCase().includes('email not confirmed')
}

// How long a desktop Google sign-in may wait for the browser before the screen
// says so. A launcher that fails to start a browser (no default browser on a
// minimal Linux desktop) still reports success, so silence is the only signal.
const STILL_WAITING_MS = 15_000

/* Keeps the existing auth calls (invitation sign-up, unconfirmed-email
 * resend, post-login redirect) — only the presentation changes. */
export function LoginPage() {
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(false)
  const [unconfirmed, setUnconfirmed] = useState(false)
  const [resending, setResending] = useState(false)
  const [resent, setResent] = useState(false)
  const [invitationDetails, setInvitationDetails] = useState<{
    email: string
    organizationName: string
    role: string
  } | null>(null)
  const [invitationToken, setInvitationToken] = useState<string | null>(null)
  const [googleWaiting, setGoogleWaiting] = useState(false)
  const [googleSignInUrl, setGoogleSignInUrl] = useState<string | null>(null)
  const [stillWaiting, setStillWaiting] = useState(false)
  const [copied, setCopied] = useState(false)
  const googleCancelled = useRef(false)
  const { login, signupWithInvitation, loginWithGoogle, cancelGoogleLogin } = useAuth()
  const navigate = useNavigate()
  const [searchParams] = useSearchParams()
  const redirectTo = searchParams.get('redirect') || '/node'

  const isInvitedSignup = !!(invitationDetails && invitationToken)

  useEffect(() => {
    const checkPendingInvitation = async () => {
      const token = sessionStorage.getItem('pending_invitation_token')
      if (!token) return

      try {
        const response = await apiClient.checkInvitation(token)
        if (response.valid) {
          setInvitationDetails({
            email: response.email || '',
            organizationName: response.organization_name || '',
            role: response.role || 'standard',
          })
          setInvitationToken(token)
          setEmail(response.email || '')
        }
      } catch (err) {
        console.error('Failed to check invitation:', err)
      }
    }

    checkPendingInvitation()
  }, [])

  const submit = async (e: React.FormEvent) => {
    e.preventDefault()
    setError('')
    setUnconfirmed(false)
    setResent(false)
    setLoading(true)

    try {
      if (isInvitedSignup && invitationToken) {
        // Backend creates the account and links the organization in one
        // step, so there's nothing left to "accept" - go straight in.
        await signupWithInvitation(invitationToken, email, password)
        sessionStorage.removeItem('pending_invitation_token')
        sessionStorage.removeItem('just_signed_up')
        navigate('/node')
        return
      }

      await login(email, password)
      navigate(redirectTo)
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Authentication failed'
      if (isEmailNotConfirmedError(message)) {
        setUnconfirmed(true)
      } else {
        setError(message)
      }
    } finally {
      setLoading(false)
    }
  }

  const handleResendConfirmation = async () => {
    setResending(true)
    try {
      await apiClient.resendConfirmation(email)
      setResent(true)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to resend confirmation email')
    } finally {
      setResending(false)
    }
  }

  useEffect(() => {
    if (!googleWaiting) return
    const timer = setTimeout(() => setStillWaiting(true), STILL_WAITING_MS)
    return () => clearTimeout(timer)
  }, [googleWaiting])

  const resetGoogleSignIn = () => {
    setGoogleWaiting(false)
    setGoogleSignInUrl(null)
    setStillWaiting(false)
    setCopied(false)
    setLoading(false)
  }

  const handleGoogleLogin = async () => {
    setError('')
    setLoading(true)
    setGoogleWaiting(true)
    googleCancelled.current = false
    try {
      await loginWithGoogle(setGoogleSignInUrl)
      navigate(redirectTo)
    } catch (err) {
      if (!googleCancelled.current) {
        setError(err instanceof Error ? err.message : 'Google login failed')
      }
    } finally {
      resetGoogleSignIn()
    }
  }

  const handleCancelGoogle = async () => {
    googleCancelled.current = true
    resetGoogleSignIn()
    try {
      await cancelGoogleLogin()
    } catch (err) {
      console.error('Failed to cancel Google sign-in:', err)
    }
  }

  const handleCopySignInUrl = async () => {
    if (!googleSignInUrl) return
    try {
      await navigator.clipboard.writeText(googleSignInUrl)
      setCopied(true)
    } catch (err) {
      console.error('Failed to copy the sign-in link:', err)
    }
  }

  return (
    <div className="flex h-screen flex-col justify-center bg-[var(--n-bg)] px-[54px]">
      <div className="n-drag absolute left-0 right-0 top-0 h-[46px]" data-tauri-drag-region />

      <span className="flex h-[34px] w-[34px] items-center justify-center rounded-[10px] bg-[var(--n-accent)] text-[16px] font-semibold leading-none text-[var(--n-bg)]">
        N
      </span>

      <h2 className="mb-1.5 mt-[22px] text-[25px] font-semibold leading-[1.25] tracking-[-0.02em] text-[var(--n-text)]">
        {isInvitedSignup ? 'Create your account' : 'Welcome back'}
      </h2>
      {searchParams.get('account') === 'deleted' && (
        <p role="status" className="mb-3 max-w-[360px] text-[14.5px] leading-[1.5] text-[var(--n-text-2)]">
          This account was deleted. This device stopped sharing compute.
        </p>
      )}
      <p className="mb-[22px] max-w-[360px] text-[14.5px] leading-[1.5] text-[var(--n-text-3)]">
        {isInvitedSignup
          ? <>Joining <strong className="text-[var(--n-text-2)]">{invitationDetails?.organizationName}</strong> as {invitationDetails?.role}.</>
          : 'Sign in and this device joins the network.'}
      </p>

      <form onSubmit={submit} className="flex max-w-[360px] flex-col gap-2">
        {unconfirmed && (
          <div className="rounded-[8px] border border-[var(--n-border)] bg-[var(--n-sidebar)] p-3 text-[14px] leading-[1.5]">
            <p className="font-semibold text-[var(--n-text)]">Please confirm your email</p>
            <p className="mt-1 text-[var(--n-text-3)]">Check your inbox for a confirmation link before signing in.</p>
            {resent ? (
              <p className="mt-1.5 text-[var(--n-text-3)]">Confirmation email resent.</p>
            ) : (
              <button type="button" onClick={handleResendConfirmation} disabled={resending}
                      className="mt-1.5 text-[var(--n-accent-400)] hover:text-[var(--n-accent-300)]">
                {resending ? 'Resending…' : 'Resend confirmation email'}
              </button>
            )}
          </div>
        )}

        {error && <p role="alert" className="text-[13.5px] leading-[1.4] text-[var(--n-text-2)]">{error}</p>}

        <input id="email" type="email" value={email}
               onChange={(e) => !invitationDetails && setEmail(e.target.value)}
               readOnly={!!invitationDetails} required autoFocus
               placeholder="you@company.com"
               className="rounded-[8px] border border-[var(--n-border)] bg-[var(--n-sidebar)] px-3 py-2.5 text-[15px] leading-[1.2] text-[var(--n-text)] outline-none placeholder:text-[var(--n-text-4)] focus:border-[var(--n-accent-800)] disabled:opacity-60" />
        <input id="password" type="password" value={password}
               onChange={(e) => setPassword(e.target.value)}
               required placeholder="Password"
               className="rounded-[8px] border border-[var(--n-border)] bg-[var(--n-sidebar)] px-3 py-2.5 text-[15px] leading-[1.2] text-[var(--n-text)] outline-none placeholder:text-[var(--n-text-4)] focus:border-[var(--n-accent-800)]" />

        <button type="submit" disabled={loading}
                className="mt-1.5 rounded-[8px] border border-[var(--n-accent)] bg-transparent px-3 py-2.5 text-[15px] font-semibold leading-[1.2] text-[var(--n-accent-300)] transition-colors hover:bg-[rgba(145,132,217,.14)] disabled:opacity-45">
          {loading ? 'Signing in…' : isInvitedSignup ? 'Sign up' : 'Sign in'}
        </button>

        {!isInvitedSignup && (
          <p className="mt-1 text-[13.5px] leading-none text-[var(--n-text-3)]">
            Don't have an account?{' '}
            <Link to="/signup" className="font-medium text-[var(--n-accent-400)] hover:text-[var(--n-accent-300)]">
              Sign up
            </Link>
          </p>
        )}
      </form>

      {googleWaiting ? (
        <div role="status" aria-label="Google sign-in"
             className="mt-3.5 flex max-w-[360px] flex-col gap-2 text-[13.5px] leading-[1.45] text-[var(--n-text-3)]">
          <p className="text-[var(--n-text-2)]">
            {stillWaiting ? 'Still waiting for the browser…' : 'Finish signing in with Google in your browser.'}
          </p>
          {googleSignInUrl && (
            <>
              <p>
                Browser didn't open? Copy this link into a browser on this computer.
                Signing in from another device won't work.
              </p>
              <div className="flex gap-2">
                <input readOnly value={googleSignInUrl} aria-label="Sign-in link"
                       onFocus={(e) => e.currentTarget.select()}
                       className="min-w-0 flex-1 rounded-[8px] border border-[var(--n-border)] bg-[var(--n-sidebar)] px-2.5 py-1.5 text-[13px] text-[var(--n-text-2)] outline-none" />
                <button type="button" onClick={handleCopySignInUrl}
                        className="text-[var(--n-accent-400)] hover:text-[var(--n-accent-300)]">
                  {copied ? 'Copied' : 'Copy link'}
                </button>
              </div>
            </>
          )}
          {stillWaiting && (
            <p>No browser on this computer? Cancel and sign in with your email and password instead.</p>
          )}
          <button type="button" onClick={handleCancelGoogle}
                  className="self-start text-[var(--n-accent)] hover:text-[var(--n-accent-300)]">
            Cancel
          </button>
        </div>
      ) : (
        <div className="mt-3.5 text-[13.5px] leading-none text-[var(--n-text-3)]">
          or{' '}
          <button onClick={handleGoogleLogin} disabled={loading}
                  className="text-[var(--n-accent)] hover:text-[var(--n-accent-300)]">
            continue with Google
          </button>
        </div>
      )}
    </div>
  )
}
