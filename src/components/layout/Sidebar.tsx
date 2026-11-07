import { useState } from 'react'
import { NavLink } from 'react-router-dom'
import { Broadcast, Monitor, Gear, CaretDown, Robot, Bug, ArrowClockwise, X, Cube, Key, ArrowSquareOut, Users } from '@phosphor-icons/react'
import { useAuth } from '@/contexts/AuthContext'
import { NodeStatusWidget } from '@/components/NodeStatusWidget'
import type { NodeState } from '@/hooks/useNodePhase'
import { useUpdater, type Updater } from '@/hooks/useUpdater'
import { WEB_CONSOLE_URL, openInBrowser } from '@/lib/webConsole'

const NAV = [
  { to: '/node', label: 'Node', Icon: Broadcast },
  { to: '/fleet', label: 'Fleet', Icon: Monitor },
  { to: '/models', label: 'Models', Icon: Cube },
  { to: '/api-keys', label: 'API keys', Icon: Key },
  { to: '/team', label: 'Team', Icon: Users },
  { to: '/settings', label: 'Settings', Icon: Gear },
]

// Debug-only tools carried over from the old dashboard - never shown in a
// production build (import.meta.env.DEV is statically false there, so
// Vite drops this branch entirely rather than just hiding it at runtime).
const DEV_NAV = [
  { to: '/playground', label: 'Playground', Icon: Robot },
  { to: '/dev-logs', label: 'Dev Logs', Icon: Bug },
]

/* Shown only once there is something to act on: a downloaded update, or the
 * restart the user asked for. Checking and downloading stay silent. */
function UpdateItem({ updater }: { updater: Updater }) {
  const { update, dismissed, dismiss, restart } = updater

  if (update.state === 'installing') {
    return (
      <div role="status" className="flex items-center gap-2 border-t border-[var(--n-line-strong)] px-3.5 py-2.5 text-[13.5px] leading-[1.3] text-[var(--n-text-3)]">
        <ArrowClockwise size={14} className="animate-spin" />
        {update.finishing_requests ? 'Finishing current request…' : 'Restarting…'}
      </div>
    )
  }
  if (update.state !== 'ready' || dismissed) return null

  return (
    <div className="flex items-center gap-1 border-t border-[var(--n-line-strong)] py-1.5 pl-2.5 pr-1.5">
      <button onClick={restart}
        className="flex min-w-0 flex-1 items-center gap-2 rounded-[7px] px-1 py-1.5 text-left text-[13.5px] font-semibold leading-[1.2] text-[var(--n-accent-200)] transition-colors hover:bg-[var(--n-tint-hover)]">
        <ArrowClockwise size={14} className="flex-none" />
        <span className="truncate">Update ready · Restart</span>
      </button>
      <button onClick={dismiss} aria-label="Dismiss update"
        className="flex h-6 w-6 flex-none items-center justify-center rounded-[6px] text-[var(--n-text-4)] transition-colors hover:bg-[var(--n-tint-hover)] hover:text-[var(--n-text-2)]">
        <X size={12} />
      </button>
    </div>
  )
}

export function Sidebar({ node }: { node: NodeState }) {
  const { user, organization, isAdmin, logout } = useAuth()
  const [accountOpen, setAccountOpen] = useState(false)
  const updater = useUpdater()

  const initials = (user?.name || user?.email || 'U').slice(0, 2).toUpperCase()

  return (
    <div className="flex h-screen w-[248px] flex-none flex-col border-r border-[var(--n-line-strong)] bg-[var(--n-sidebar)]">
      {/* Frameless chrome: reserve space for the OS's own traffic lights
          (no placeholder drawn - Tauri v1 can't reposition them to match
          exact CSS coordinates, so we just leave clear space) and make the
          strip draggable */}
      <div className="n-drag h-9" data-tauri-drag-region />

      <div className="px-[18px] pb-5">
        <div className="text-[19px] font-semibold leading-none tracking-[-0.01em] text-[var(--n-text)]">Nodea</div>
        {organization && (
          <div className="mt-[5px] truncate text-[13px] leading-none text-[var(--n-text-3)]">
            {organization.name}
          </div>
        )}
      </div>

      <nav className="flex flex-col gap-0.5 px-2.5">
        {[...NAV, ...(import.meta.env.DEV ? DEV_NAV : [])].map(({ to, label, Icon }) => (
          <NavLink key={to} to={to}
            className={({ isActive }) =>
              `flex items-center gap-2.5 rounded-[8px] px-2.5 py-2 text-[15px] font-semibold leading-none transition-colors ${
                isActive
                  ? 'bg-[var(--n-tint-active)] text-[var(--n-accent-200)]'
                  : 'text-[var(--n-text-4)] hover:bg-[var(--n-tint-hover)] hover:text-[var(--n-text-2)]'
              }`
            }>
            <Icon size={16} />
            {label}
          </NavLink>
        ))}
      </nav>

      <div className="flex-1" />

      <NodeStatusWidget node={node} />

      <UpdateItem updater={updater} />

      {/* Always visible, right above the account menu */}
      <div className="border-t border-[var(--n-line-strong)] px-2.5 py-1.5">
        <button onClick={() => openInBrowser(WEB_CONSOLE_URL)}
          className="flex w-full items-center gap-2.5 rounded-[8px] px-2.5 py-2 text-left text-[14px] font-semibold leading-none text-[var(--n-text-4)] transition-colors hover:bg-[var(--n-tint-hover)] hover:text-[var(--n-text-2)]">
          <ArrowSquareOut size={15} />
          Open Web Console
        </button>
      </div>

      <div className="relative border-t border-[var(--n-line-strong)]">
        {accountOpen && (
          <div className="absolute bottom-[calc(100%+6px)] left-2.5 right-2.5 z-[2] rounded-[10px] bg-[var(--n-raised)] p-1.5 shadow-[var(--n-shadow-md)]">
            <div className="px-[9px] pb-[9px] pt-2">
              <span className="block text-[14px] font-semibold leading-[1.3] text-[var(--n-text)]">
                {user?.name || 'User'}
              </span>
              <span className="mt-0.5 block text-[12.5px] leading-[1.3] text-[var(--n-text-3)]">
                {isAdmin ? 'Admin' : 'Member'} · {organization?.name}
              </span>
            </div>
            <div className="mx-0.5 mb-[5px] h-px bg-[var(--n-border)]" />
            {[
              { label: 'Check for updates', onClick: () => { setAccountOpen(false); updater.check() } },
              { label: 'Sign out', onClick: logout },
            ].map(({ label, onClick }) => (
              <button key={label} onClick={onClick}
                className="block w-full rounded-[7px] px-[9px] py-[7px] text-left text-[14px] font-medium leading-none text-[var(--n-text-2)] transition-colors hover:bg-[var(--n-tint-selected)] hover:text-[var(--n-accent-200)]">
                {label}
              </button>
            ))}
          </div>
        )}

        <button onClick={() => setAccountOpen((o) => !o)}
          className="flex w-full items-center gap-[9px] px-3.5 py-3 text-left transition-colors"
          style={{ background: accountOpen ? 'rgba(145,132,217,.1)' : 'transparent' }}>
          <span className="flex h-[26px] w-[26px] flex-none items-center justify-center rounded-full bg-[var(--n-accent-800)] text-[12px] font-semibold leading-none text-[var(--n-accent-300)]">
            {initials}
          </span>
          <span className="min-w-0 flex-1">
            <span className="block text-[14px] font-semibold leading-[1.2] text-[var(--n-text-2)]">
              {user?.name || 'User'}
            </span>
            <span className="block truncate text-[12.5px] leading-[1.2] text-[var(--n-text-3)]">
              {user?.email}
            </span>
          </span>
          <CaretDown size={13} color="var(--n-text-4)"
            style={{ transform: accountOpen ? 'rotate(180deg)' : 'rotate(0deg)', transition: 'transform .18s ease' }} />
        </button>
      </div>
    </div>
  )
}
