import { useState, useEffect, useRef, useCallback } from 'react'
import { useNavigate } from 'react-router-dom'
import { useAuth } from '../contexts/AuthContext'
import { apiClient } from '../lib/api'
import { Button } from '../components/ui/button'
import { Card } from '../components/ui/card'
import { toast } from 'sonner'

/* A new user's organization is created without asking for a name: the backend
 * names it after them ("{name}'s Organization"), and they can rename it later.
 * Someone who just left an organization is asked first, since they may rather
 * open an invitation. */
export function OnboardingPage() {
  const [isCreating, setIsCreating] = useState(false)
  const [error, setError] = useState('')
  const started = useRef(false)
  const { refreshOrganization, leftOrganizationName, organization } = useAuth()
  const navigate = useNavigate()

  // Joined an organization elsewhere (an invitation accepted in the browser,
  // or re-added) while this page was open: the membership check picks it up,
  // and the node page is where sharing resumes. Not for a fresh signup, whom
  // ProtectedRoute keeps here on purpose until they finish.
  useEffect(() => {
    if (organization && !isCreating && sessionStorage.getItem('just_signed_up') !== 'true') {
      navigate('/node', { replace: true })
    }
  }, [organization, isCreating, navigate])

  const createOrganization = useCallback(async () => {
    setError('')
    setIsCreating(true)

    try {
      await apiClient.createOrganizationOnboarding()

      // Refresh organization in auth context
      await refreshOrganization()

      // Clear the "just signed up" flag
      sessionStorage.removeItem('just_signed_up')

      // The creator is the organization's admin, and a new organization has
      // no models yet: pick one next, so this Mac has something to serve.
      navigate('/onboarding/model')
    } catch (err) {
      // Already in one: this page showed while the organization was still
      // loading. Nothing to set up.
      if ((err as { status?: number }).status === 422) {
        await refreshOrganization()
        navigate('/node', { replace: true })
        return
      }
      console.error('Failed to create organization:', err)
      const message = err instanceof Error ? err.message : 'Failed to create organization'
      setError(message)
      toast.error(message)
      setIsCreating(false)
    }
  }, [refreshOrganization, navigate])

  useEffect(() => {
    if (organization || leftOrganizationName || started.current) return
    started.current = true
    void createOrganization()
  }, [organization, leftOrganizationName, createOrganization])

  return (
    <div className="min-h-screen flex items-center justify-center bg-background p-4">
      <Card className="w-full max-w-md p-6">
        <div className="text-center mb-6">
          {leftOrganizationName ? (
            <>
              <h1 className="text-2xl font-bold mb-2">You're no longer in {leftOrganizationName}</h1>
              <p className="text-muted-foreground">
                This device stopped sharing compute. Create an organization of your own, or open an
                invitation link to join one.
              </p>
            </>
          ) : (
            <>
              <h1 className="text-2xl font-bold mb-2">Welcome to Nodea!</h1>
              <p className="text-muted-foreground">
                {error ? "We couldn't set up your organization" : 'Setting up your organization…'}
              </p>
            </>
          )}
        </div>

        {error && (
          <p role="alert" className="text-sm text-destructive mb-4">{error}</p>
        )}

        {(leftOrganizationName || error) && (
          <Button className="w-full" onClick={() => void createOrganization()} disabled={isCreating}>
            {isCreating ? 'Creating...' : leftOrganizationName ? 'Create my organization' : 'Try again'}
          </Button>
        )}
      </Card>
    </div>
  )
}
