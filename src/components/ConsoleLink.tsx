import { ArrowUpRight } from '@phosphor-icons/react'
import { openInBrowser } from '@/lib/webConsole'

/* A link to one of the web console's pages. A real <a href> - so it reads as
 * a link and shows where it goes - whose click opens the system browser. */
export function ConsoleLink({
  href, label, children, className,
}: {
  href: string
  /** Accessible name, e.g. "Open Llama 3.2 3B in the web console". */
  label: string
  children: React.ReactNode
  className?: string
}) {
  return (
    <a href={href} aria-label={label} title="Open in the web console"
       onClick={(e) => {
         e.preventDefault()
         openInBrowser(href)
       }}
       className={`group inline-flex max-w-full items-center gap-1 text-inherit hover:text-[var(--n-accent-300)] ${className ?? ''}`}>
      <span className="truncate">{children}</span>
      <ArrowUpRight size={12} className="flex-none opacity-0 transition-opacity group-hover:opacity-100" />
    </a>
  )
}
