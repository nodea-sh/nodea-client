import { useCallback, useEffect, useState } from 'react'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { GhostButton, PageHeader, PrimaryButton } from '@/components/nocturne'
import { useAuth } from '@/contexts/AuthContext'
import {
  apiClient,
  ApiError,
  type Organization,
  type OrganizationMember,
  type UserInvitation,
} from '@/lib/api'

/* The organization's people, as on the web console's Team page
 * (frontend/src/pages/TeamPage.tsx): everyone sees the members; admins also
 * invite, change roles and revoke pending invitations. */

type Role = 'admin' | 'standard'

const memberName = (m: OrganizationMember) => m.full_name ?? m.email ?? 'this member'

function formatDate(value: string | null | undefined): string {
  if (!value) return '—'
  const parsed = new Date(value)
  return Number.isNaN(parsed.getTime()) ? '—' : parsed.toLocaleDateString()
}

const selectClass =
  'h-10 rounded-[8px] border border-[var(--n-border)] bg-[var(--n-bg)] px-3 text-[14px] text-[var(--n-text)] focus:outline-none focus:ring-1 focus:ring-[var(--n-accent)]'

function RoleBadge({ role }: { role: Role }) {
  return (
    <span className="rounded-[5px] border px-1.5 py-0.5 text-[12.5px] font-medium leading-none"
          style={role === 'admin'
            ? { borderColor: 'var(--n-accent-800)', color: 'var(--n-accent-300)' }
            : { borderColor: 'var(--n-mute-4)', color: 'var(--n-text-3)' }}>
      {role === 'admin' ? 'Admin' : 'Member'}
    </span>
  )
}

function Notice({ children, error }: { children: React.ReactNode; error?: boolean }) {
  return (
    <p role={error ? 'alert' : 'status'} className="mt-3 text-[13.5px] leading-[1.5]"
       style={{ color: error ? 'var(--n-text)' : 'var(--n-text-3)' }}>
      {children}
    </p>
  )
}

export function TeamPage() {
  const { isAdmin, emailConfirmed, user } = useAuth()
  const [members, setMembers] = useState<OrganizationMember[] | null>(null)
  const [invitations, setInvitations] = useState<UserInvitation[] | null>(null)
  const [organization, setOrganization] = useState<Organization | null>(null)
  const [error, setError] = useState<string | null>(null)

  const [inviteEmail, setInviteEmail] = useState('')
  const [inviteRole, setInviteRole] = useState<Role>('standard')
  const [inviting, setInviting] = useState(false)
  const [inviteNotice, setInviteNotice] = useState<{ text: string; error?: boolean } | null>(null)

  const [editing, setEditing] = useState<OrganizationMember | null>(null)
  const [roleNotice, setRoleNotice] = useState<string | null>(null)

  const isSelf = (m: OrganizationMember) =>
    m.uuid === user?.uuid || (!!user?.email && m.email?.toLowerCase() === user.email.toLowerCase())

  const loadMembers = useCallback(async () => {
    try {
      const { users } = await apiClient.getOrganizationMembers()
      setMembers(users)
      setError(null)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Could not load your team')
    }
  }, [])

  const loadInvitations = useCallback(async () => {
    try {
      setInvitations(await apiClient.getInvitations())
    } catch (err) {
      // Members get 403: they don't manage invitations, which isn't an error.
      if (err instanceof ApiError && err.status === 403) setInvitations(null)
      else setError(err instanceof Error ? err.message : 'Could not load pending invitations')
    }
  }, [])

  useEffect(() => {
    void loadMembers()
    // Plan and usage are supplementary: a failure here doesn't block the page.
    apiClient.getOrganization().then(setOrganization).catch(() => {})
  }, [loadMembers])

  useEffect(() => {
    if (isAdmin) void loadInvitations()
  }, [isAdmin, loadInvitations])

  const invite = async (e: React.FormEvent) => {
    e.preventDefault()
    setInviteNotice(null)
    setInviting(true)
    try {
      const created = await apiClient.createInvitation({ email: inviteEmail.trim(), role: inviteRole })
      setInviteEmail('')
      setInviteRole('standard')
      setInviteNotice({ text: `Invitation sent to ${created.email}.` })
      await loadInvitations()
    } catch (err) {
      setInviteNotice({ text: err instanceof Error ? err.message : 'Could not send the invitation', error: true })
    } finally {
      setInviting(false)
    }
  }

  const revoke = async (invitation: UserInvitation) => {
    setInviteNotice(null)
    try {
      await apiClient.revokeInvitation(invitation.uuid)
      setInviteNotice({ text: `Invitation to ${invitation.email} revoked.` })
      await loadInvitations()
    } catch (err) {
      setInviteNotice({ text: err instanceof Error ? err.message : 'Could not revoke the invitation', error: true })
    }
  }

  if (!members) {
    return (
      <div className="p-[30px]">
        {error
          ? <Notice error>{error}</Notice>
          : <p role="status" className="text-[14px] text-[var(--n-text-4)]">Loading team…</p>}
      </div>
    )
  }

  const pending = (invitations ?? []).filter((i) => i.status === 'pending')
  const plan = organization?.plan
  const connected = organization?.connected_device_count
  const atLimit = plan && connected !== undefined && connected >= plan.max_devices

  return (
    <div className="p-[30px]">
      <PageHeader title={`${members.length} ${members.length === 1 ? 'member' : 'members'}`}
                  sub="People who can sign in to this organization and register devices to it." />

      {plan && connected !== undefined && (
        <div data-testid="plan-usage" className="-mt-2.5 mb-5 flex flex-wrap items-center gap-2 text-[14px]">
          <span className="rounded-[5px] border border-[var(--n-mute-4)] px-1.5 py-0.5 text-[12.5px] font-medium text-[var(--n-text-3)]">
            {plan.name} plan
          </span>
          <span style={{ color: atLimit ? 'var(--n-accent-300)' : 'var(--n-text-4)' }}>
            {connected} of {plan.max_devices} devices connected
          </span>
          {atLimit && <a href="mailto:hello@getnodea.com">Contact us to increase your limit</a>}
        </div>
      )}

      {error && <Notice error>{error}</Notice>}

      {isAdmin && (
        <section aria-labelledby="invite-heading"
                 className="mb-6 rounded-[12px] border border-[var(--n-line)] bg-[var(--n-sidebar)] p-[18px]">
          <h2 id="invite-heading" className="text-[15px] font-semibold text-[var(--n-text)]">Invite a teammate</h2>
          <p className="mt-1 text-[14px] leading-[1.5] text-[var(--n-text-4)]">
            They receive an email with a link to set a password and join this organization.
          </p>
          {!emailConfirmed && (
            <p data-testid="confirm-email-note" className="mt-2 text-[14px] leading-[1.5] text-[var(--n-text-3)]">
              Confirm your email to invite teammates, change roles or revoke invitations. Use the link
              we sent to {user?.email ?? 'your inbox'}.
            </p>
          )}
          <form onSubmit={invite} className="mt-3 flex flex-wrap items-start gap-2">
            <div className="min-w-[16rem] flex-1">
              <Input type="email" required placeholder="teammate@company.com" value={inviteEmail}
                     aria-label="Email address" disabled={inviting || !emailConfirmed}
                     onChange={(e) => setInviteEmail(e.target.value)} />
            </div>
            <select value={inviteRole} disabled={inviting || !emailConfirmed} className={selectClass} aria-label="Role"
                    onChange={(e) => setInviteRole(e.target.value as Role)}>
              <option value="standard">Member</option>
              <option value="admin">Admin</option>
            </select>
            <PrimaryButton type="submit" disabled={inviting || !emailConfirmed}>{inviting ? 'Sending…' : 'Send invite'}</PrimaryButton>
          </form>
          {inviteNotice && <Notice error={inviteNotice.error}>{inviteNotice.text}</Notice>}
        </section>
      )}

      <section aria-labelledby="members-heading">
        <h2 id="members-heading" className="mb-2.5 text-[15px] font-semibold text-[var(--n-text)]">Members</h2>
        {roleNotice && <p role="status" className="mb-2.5 text-[13.5px] text-[var(--n-text-3)]">{roleNotice}</p>}
        <div className="overflow-hidden rounded-[12px] border border-[var(--n-line)]">
          <div className="grid grid-cols-[2fr_0.8fr_1.1fr_0.9fr_auto] gap-3 bg-[var(--n-sidebar)] px-[18px] py-2.5 text-[12.5px] font-semibold uppercase leading-none tracking-[0.06em] text-[var(--n-text-3)]">
            <span>Member</span>
            <span>Role</span>
            <span>Devices</span>
            <span>Joined</span>
            <span className="w-[70px]" />
          </div>
          {members.map((m) => (
            <div key={m.uuid} data-testid="member-row"
                 className="grid grid-cols-[2fr_0.8fr_1.1fr_0.9fr_auto] items-center gap-3 border-t border-[var(--n-line)] px-[18px] py-3.5">
              <span className="min-w-0">
                <span className="block truncate text-[15px] font-semibold leading-[1.3]"
                      style={{ color: m.is_active ? 'var(--n-text)' : 'var(--n-text-4)' }}>
                  {m.full_name ?? m.email ?? 'Unknown'}
                  {isSelf(m) && <span className="ml-1.5 text-[12.5px] font-normal text-[var(--n-text-4)]">(you)</span>}
                </span>
                {m.full_name && m.email && (
                  <span className="mt-[3px] block truncate text-[13px] text-[var(--n-text-3)]">{m.email}</span>
                )}
                {!m.is_active && <span className="mt-[3px] block text-[13px] text-[var(--n-text-4)]">Deactivated</span>}
              </span>
              <span><RoleBadge role={m.role} /></span>
              <span className="text-[14px] text-[var(--n-text-3)]">{m.connected ? 'Device online' : 'No device online'}</span>
              <span className="text-[14px] text-[var(--n-text-3)]">{formatDate(m.created_at)}</span>
              <span className="flex w-[70px] justify-end">
                {isAdmin && m.is_active && !isSelf(m) && (
                  <GhostButton aria-label={`Edit role for ${memberName(m)}`} disabled={!emailConfirmed}
                               onClick={() => { setRoleNotice(null); setEditing(m) }}>
                    Edit
                  </GhostButton>
                )}
              </span>
            </div>
          ))}
        </div>
      </section>

      {isAdmin && (
        <section aria-labelledby="pending-heading" className="mt-6">
          <h2 id="pending-heading" className="mb-2.5 text-[15px] font-semibold text-[var(--n-text)]">Pending invitations</h2>
          {pending.length === 0 ? (
            <div className="rounded-[12px] border border-dashed border-[var(--n-border)] px-4 py-8 text-center text-[14px] text-[var(--n-text-4)]">
              No invitations waiting to be accepted.
            </div>
          ) : (
            <div className="overflow-hidden rounded-[12px] border border-[var(--n-line)]">
              {pending.map((inv, i) => (
                <div key={inv.uuid} data-testid="invitation-row"
                     className={`grid grid-cols-[2fr_0.8fr_1fr_auto] items-center gap-3 px-[18px] py-3.5 ${i ? 'border-t border-[var(--n-line)]' : ''}`}>
                  <span className="truncate text-[15px] font-semibold text-[var(--n-text)]">{inv.email}</span>
                  <span><RoleBadge role={inv.role} /></span>
                  <span className="text-[14px] text-[var(--n-text-3)]">Expires {formatDate(inv.expires_at)}</span>
                  <GhostButton aria-label={`Revoke invitation to ${inv.email}`} disabled={!emailConfirmed}
                               onClick={() => void revoke(inv)}>Revoke</GhostButton>
                </div>
              ))}
            </div>
          )}
        </section>
      )}

      <EditRoleDialog member={editing} onClose={() => setEditing(null)}
                      onSaved={(member, role) => {
                        setMembers((prev) => prev?.map((x) => (x.uuid === member.uuid ? { ...x, role } : x)) ?? prev)
                        setEditing(null)
                        setRoleNotice(`${memberName(member)} is now ${role === 'admin' ? 'an Admin' : 'a Member'}.`)
                      }} />
    </div>
  )
}

function EditRoleDialog({
  member, onClose, onSaved,
}: {
  member: OrganizationMember | null
  onClose: () => void
  onSaved: (member: OrganizationMember, role: Role) => void
}) {
  const [role, setRole] = useState<Role>('standard')
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (member) {
      setRole(member.role)
      setError(null)
    }
  }, [member])

  const save = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!member) return
    setSaving(true)
    setError(null)
    try {
      await apiClient.updateOrganizationMember(member.uuid, { role })
      onSaved(member, role)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Could not change the role')
    } finally {
      setSaving(false)
    }
  }

  return (
    <Dialog open={member !== null} onOpenChange={(open) => !open && !saving && onClose()}>
      <DialogContent>
        <form onSubmit={save} className="grid gap-4">
          <DialogHeader>
            <DialogTitle>Edit role</DialogTitle>
            <DialogDescription>
              Admins can invite people, manage models and API keys, and change roles. Members can use
              the organization and register devices.
            </DialogDescription>
          </DialogHeader>
          <label className="grid gap-2 text-[14px] text-[var(--n-text-2)]">
            Role for {member ? memberName(member) : ''}
            <select value={role} disabled={saving} className={selectClass}
                    onChange={(e) => setRole(e.target.value as Role)}>
              <option value="standard">Member</option>
              <option value="admin">Admin</option>
            </select>
          </label>
          {error && <p role="alert" className="text-[13.5px] text-[var(--n-text-2)]">{error}</p>}
          <DialogFooter>
            <GhostButton onClick={() => !saving && onClose()}>Cancel</GhostButton>
            <PrimaryButton type="submit" disabled={saving || !member || role === member.role}>
              {saving ? 'Saving…' : 'Save'}
            </PrimaryButton>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
